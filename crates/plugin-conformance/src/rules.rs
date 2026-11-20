use std::collections::BTreeMap;
use std::path::Path;

use crate::docs::DocSet;
use crate::id::{Reference, TestId};
use crate::{ClosureLink, DefectField, Error, Finding, LinkFault, Location};

const QUESTION_CLOSURE: &str = "question-closure.md";
const PROOF_INVENTORY: &str = "implementation/proof-inventory.md";

pub(crate) fn run(docs: &DocSet) -> Result<Vec<Finding>, Error> {
    let mut findings = Vec::new();
    let registry = registry(docs, &mut findings);
    references(docs, &registry, &mut findings);
    closures(docs, &mut findings)?;
    links(docs, &mut findings)?;
    proof_targets(docs, &mut findings)?;
    defects(docs, &mut findings);
    Ok(findings)
}

fn at(file: &Path, line: usize) -> Location {
    Location { file: file.to_owned(), line }
}

fn is_acceptance(path: &Path) -> bool {
    path.parent() == Some(Path::new("implementation"))
        && path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.ends_with("-acceptance.md"))
}

fn registry(docs: &DocSet, findings: &mut Vec<Finding>) -> BTreeMap<TestId, Location> {
    let mut registry = BTreeMap::new();
    let defining_tables = docs
        .docs
        .iter()
        .filter(|(path, _)| is_acceptance(path))
        .flat_map(|(path, document)| document.tables.iter().map(move |table| (path, table)))
        .filter(|(_, table)| table.first_header() == "ID");
    for (path, table) in defining_tables {
        for row in &table.rows {
            for text in row.text(Some(0)).split('/').map(str::trim) {
                let here = at(path, row.line);
                match TestId::parse(text) {
                    None => findings.push(Finding::IllFormedId { at: here, text: text.to_owned() }),
                    Some(id) => match registry.get(&id) {
                        Some(first) => findings.push(Finding::DuplicateId { id, first: Location::clone(first), again: here }),
                        None => {
                            registry.insert(id, here);
                        }
                    },
                }
            }
        }
    }
    registry
}

fn references(docs: &DocSet, registry: &BTreeMap<TestId, Location>, findings: &mut Vec<Finding>) {
    for (path, document) in &docs.docs {
        for span in &document.code_spans {
            let here = at(path, span.line);
            match Reference::parse(&span.text) {
                Some(Reference::Id(id)) if !registry.contains_key(&id) => findings.push(Finding::UndefinedId { at: here, id }),
                Some(Reference::Wildcard(wildcard)) if !registry.keys().any(|id| wildcard.matches(id)) => {
                    findings.push(Finding::EmptyWildcard { at: here, pattern: span.text.clone() });
                }
                _ => {}
            }
        }
    }
}

fn closures(docs: &DocSet, findings: &mut Vec<Finding>) -> Result<(), Error> {
    let (path, document) = docs.require(QUESTION_CLOSURE)?;
    for table in &document.tables {
        let Some(closure) = table.column("Closure") else {
            continue;
        };
        for row in &table.rows {
            let question = row.text(Some(0)).to_owned();
            let here = at(path, row.line);
            if row.text(Some(closure)).split_whitespace().next() == Some("Open") {
                findings.push(Finding::OpenQuestion { at: here, question });
                continue;
            }
            let linked_dirs: Vec<_> = row
                .cell(Some(closure))
                .into_iter()
                .flat_map(|cell| &cell.links)
                .filter_map(|dest| docs.resolve(path, dest))
                .filter(|target| docs.exists(&target.path))
                .filter_map(|target| target.path.components().next().map(|first| first.as_os_str().to_owned()))
                .collect();
            for (dir, missing) in [("decisions", ClosureLink::Decision), ("contracts", ClosureLink::Contract)] {
                if !linked_dirs.iter().any(|linked| linked == dir) {
                    findings.push(Finding::UnlinkedClosure { at: here.clone(), question: question.clone(), missing });
                }
            }
        }
    }
    Ok(())
}

fn links(docs: &DocSet, findings: &mut Vec<Finding>) -> Result<(), Error> {
    for (path, document) in &docs.docs {
        for link in &document.links {
            let Some(target) = docs.resolve(path, &link.dest) else {
                continue;
            };
            let fault = if !docs.exists(&target.path) {
                Some(LinkFault::MissingFile)
            } else {
                match target.anchor.as_deref() {
                    Some(anchor) if !anchor.is_empty() && !docs.has_anchor(&target.path, anchor)? => Some(LinkFault::MissingAnchor),
                    _ => None,
                }
            };
            if let Some(fault) = fault {
                findings.push(Finding::BrokenLink { at: at(path, link.line), target: link.dest.clone(), fault });
            }
        }
    }
    Ok(())
}

fn names_an_app(target: &str) -> bool {
    target.split(|c: char| !c.is_alphanumeric()).any(|word| matches!(word, "Both" | "EC" | "PC"))
}

fn proof_targets(docs: &DocSet, findings: &mut Vec<Finding>) -> Result<(), Error> {
    let (path, document) = docs.require(PROOF_INVENTORY)?;
    for table in document.tables.iter().filter(|table| table.first_header() == "ID") {
        let target = table.column("Target");
        for row in &table.rows {
            let reference = row.text(Some(0)).split_whitespace().next().unwrap_or_default();
            if reference.starts_with("REF-") && !names_an_app(row.text(target)) {
                findings.push(Finding::UntargetedRef { at: at(path, row.line), reference: reference.to_owned() });
            }
        }
    }
    Ok(())
}

fn defects(docs: &DocSet, findings: &mut Vec<Finding>) {
    for (path, document) in &docs.docs {
        for table in document.tables.iter().filter(|table| table.first_header() == "Defect") {
            let columns = [(table.column("Owner"), DefectField::Owner), (table.column("Exit phase"), DefectField::ExitPhase)];
            for row in &table.rows {
                for (column, missing) in columns {
                    if row.text(column).is_empty() {
                        findings.push(Finding::IncompleteDefect { at: at(path, row.line), defect: row.text(Some(0)).to_owned(), missing });
                    }
                }
            }
        }
    }
}
