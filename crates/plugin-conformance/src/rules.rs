use std::collections::BTreeMap;
use std::path::Path;

use crate::docs::{DocSet, Resolution};
use crate::id::{Reference, TestId};
use crate::markdown::{Document, Table};
use crate::{ClosureLink, DefectField, Error, Finding, LinkFault, Location};

const QUESTION_CLOSURE: &str = "question-closure.md";
const PROOF_INVENTORY: &str = "implementation/proof-inventory.md";

type Registry = BTreeMap<TestId, Location>;

pub(crate) fn run(docs: &DocSet) -> Result<Vec<Finding>, Error> {
    let mut findings = Vec::new();
    let registry = registry(docs, &mut findings)?;
    references(docs, &registry, &mut findings);
    unmarked_references(docs, &registry, &mut findings);
    closures(docs, &mut findings)?;
    links(docs, &mut findings);
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

fn id_tables(document: &Document) -> impl Iterator<Item = &Table> {
    document.tables.iter().filter(|table| table.first_header() == "ID")
}

fn registry(docs: &DocSet, findings: &mut Vec<Finding>) -> Result<Registry, Error> {
    let mut registry = Registry::new();
    let mut acceptance_files = 0;
    for (path, document) in docs.docs.iter().filter(|(path, _)| is_acceptance(path)) {
        acceptance_files += 1;
        let rows: Vec<_> = id_tables(document).flat_map(|table| &table.rows).collect();
        if rows.is_empty() {
            return Err(Error::MissingTable { file: path.clone(), header: "ID" });
        }
        for row in rows {
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
    if acceptance_files == 0 {
        return Err(Error::NoAcceptanceFiles);
    }
    Ok(registry)
}

fn references(docs: &DocSet, registry: &Registry, findings: &mut Vec<Finding>) {
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

fn unmarked_references(docs: &DocSet, registry: &Registry, findings: &mut Vec<Finding>) {
    for (path, document) in &docs.docs {
        let defines = |table: usize| is_acceptance(path) && document.tables.get(table).is_some_and(|table| table.first_header() == "ID");
        for prose in document.prose.iter().filter(|prose| !prose.first_cell_of_table.is_some_and(defines)) {
            let tokens = prose.text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'));
            for id in tokens.filter_map(TestId::parse).filter(|id| registry.contains_key(id)) {
                findings.push(Finding::UnmarkedReference { at: at(path, prose.line), id });
            }
        }
    }
}

fn is_open(closure: &str) -> bool {
    closure.split_whitespace().next().map(|word| word.trim_matches(|c: char| !c.is_alphanumeric())) == Some("Open")
}

fn closures(docs: &DocSet, findings: &mut Vec<Finding>) -> Result<(), Error> {
    let (path, document) = docs.require(QUESTION_CLOSURE)?;
    let mut rows = 0;
    for table in &document.tables {
        let Some(closure) = table.column("Closure") else {
            continue;
        };
        for row in &table.rows {
            rows += 1;
            let question = row.text(Some(0)).to_owned();
            let here = at(path, row.line);
            if is_open(row.text(Some(closure))) {
                findings.push(Finding::OpenQuestion { at: here, question });
                continue;
            }
            let linked_dirs: Vec<_> = row
                .cell(Some(closure))
                .into_iter()
                .flat_map(|cell| &cell.links)
                .filter_map(|dest| match docs.resolve(path, dest) {
                    Resolution::Doc { path, .. } => path.components().next(),
                    Resolution::External | Resolution::Broken(_) => None,
                })
                .collect();
            for (dir, missing) in [("decisions", ClosureLink::Decision), ("contracts", ClosureLink::Contract)] {
                if !linked_dirs.iter().any(|linked| linked.as_os_str() == dir) {
                    findings.push(Finding::UnlinkedClosure { at: here.clone(), question: question.clone(), missing });
                }
            }
        }
    }
    if rows == 0 {
        return Err(Error::MissingTable { file: path.to_owned(), header: "Closure" });
    }
    Ok(())
}

fn links(docs: &DocSet, findings: &mut Vec<Finding>) {
    for (path, document) in &docs.docs {
        for link in &document.links {
            let fault = match docs.resolve(path, &link.dest) {
                Resolution::External => None,
                Resolution::Broken(fault) => Some(fault),
                Resolution::Doc { document, anchor, .. } => match anchor {
                    Some(anchor) if !anchor.is_empty() && !document.anchors.contains(anchor) => Some(LinkFault::MissingAnchor),
                    _ => None,
                },
            };
            if let Some(fault) = fault {
                findings.push(Finding::BrokenLink { at: at(path, link.line), target: link.dest.clone(), fault });
            }
        }
    }
}

fn names_an_app(target: &str) -> bool {
    target.split(|c: char| !c.is_alphanumeric()).any(|word| matches!(word, "Both" | "EC" | "PC"))
}

fn proof_targets(docs: &DocSet, findings: &mut Vec<Finding>) -> Result<(), Error> {
    let (path, document) = docs.require(PROOF_INVENTORY)?;
    let mut proofs = 0;
    for table in id_tables(document) {
        let target = table.column("Target");
        for row in &table.rows {
            let reference = row.text(Some(0)).split_whitespace().next().unwrap_or_default();
            if !reference.starts_with("REF-") {
                continue;
            }
            proofs += 1;
            if !names_an_app(row.text(target)) {
                findings.push(Finding::UntargetedRef { at: at(path, row.line), reference: reference.to_owned() });
            }
        }
    }
    if proofs == 0 {
        return Err(Error::MissingTable { file: path.to_owned(), header: "ID" });
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
