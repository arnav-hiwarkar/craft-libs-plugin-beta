use std::path::{Path, PathBuf};

use craft_plugin_conformance::{ClosureLink, DefectField, Error, Finding, LinkFault, Location, TestId, check};

fn findings(fixture: &str) -> Vec<Finding> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(fixture);
    check(&root).expect("the fixture is readable")
}

fn at(file: &str, line: usize) -> Location {
    Location { file: PathBuf::from(file), line }
}

fn id(text: &str) -> TestId {
    TestId::parse(text).expect("a well-formed test id")
}

#[test]
fn an_ill_formed_id_in_a_defining_cell_is_reported_and_other_tables_define_nothing() {
    let file = "implementation/runtime-acceptance.md";
    assert_eq!(
        findings("ill-formed-id"),
        vec![
            Finding::IllFormedId { at: at(file, 5), text: "RUN_bad".into() },
            Finding::IllFormedId { at: at(file, 6), text: "PAR-pc-liquify (with spike)".into() },
            Finding::IllFormedId { at: at(file, 7), text: "RND-3d".into() },
        ]
    );
}

#[test]
fn a_second_definition_in_any_acceptance_file_is_a_duplicate() {
    assert_eq!(
        findings("duplicate-id"),
        vec![Finding::DuplicateId {
            id: id("RUN-dup"),
            first: at("implementation/a-acceptance.md", 5),
            again: at("implementation/b-acceptance.md", 5),
        }]
    );
}

#[test]
fn only_code_spans_are_references_so_prose_words_are_never_ids() {
    assert_eq!(findings("undefined-id"), vec![Finding::UndefinedId { at: at("notes.md", 3), id: id("RUN-sinc") }]);
}

#[test]
fn a_wildcard_must_match_a_defined_id() {
    assert_eq!(
        findings("empty-wildcard"),
        vec![
            Finding::EmptyWildcard { at: at("notes.md", 3), pattern: "NET-*".into() },
            Finding::EmptyWildcard { at: at("notes.md", 3), pattern: "PAR-ec-*".into() },
        ]
    );
}

#[test]
fn an_open_question_is_reported() {
    assert_eq!(findings("open-question"), vec![Finding::OpenQuestion { at: at("question-closure.md", 8), question: "SYS-01.2".into() }]);
}

#[test]
fn a_closure_must_link_an_existing_decision_and_contract() {
    let file = "question-closure.md";
    assert_eq!(
        findings("unlinked-closure"),
        vec![
            Finding::UnlinkedClosure { at: at(file, 7), question: "SYS-01.1".into(), missing: ClosureLink::Contract },
            Finding::UnlinkedClosure { at: at(file, 8), question: "SYS-01.2".into(), missing: ClosureLink::Contract },
            Finding::BrokenLink { at: at(file, 8), target: "contracts/gone.md".into(), fault: LinkFault::MissingFile },
        ]
    );
}

#[test]
fn links_resolve_to_files_and_github_heading_anchors() {
    let file = "guide.md";
    assert_eq!(
        findings("broken-link"),
        vec![
            Finding::BrokenLink { at: at(file, 10), target: "missing.md".into(), fault: LinkFault::MissingFile },
            Finding::BrokenLink { at: at(file, 10), target: "other.md#nowhere".into(), fault: LinkFault::MissingAnchor },
            Finding::BrokenLink { at: at(file, 10), target: "#repeated-2".into(), fault: LinkFault::MissingAnchor },
        ]
    );
}

#[test]
fn a_proof_row_must_name_an_app() {
    let file = "implementation/proof-inventory.md";
    assert_eq!(
        findings("untargeted-ref"),
        vec![
            Finding::UntargetedRef { at: at(file, 7), reference: "REF-03".into() },
            Finding::UntargetedRef { at: at(file, 8), reference: "REF-04".into() },
        ]
    );
}

#[test]
fn a_defect_needs_an_owner_and_an_exit_phase() {
    let file = "implementation/defects.md";
    assert_eq!(
        findings("incomplete-defect"),
        vec![
            Finding::IncompleteDefect { at: at(file, 6), defect: "DEFECT-2".into(), missing: DefectField::Owner },
            Finding::IncompleteDefect { at: at(file, 7), defect: "DEFECT-3".into(), missing: DefectField::ExitPhase },
        ]
    );
}

#[test]
fn a_doc_set_without_question_closure_is_an_error() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/missing-closure");
    let missing = match check(&root) {
        Err(Error::MissingDocument(path)) => path,
        other => panic!("expected a missing document, got {other:?}"),
    };
    assert_eq!(missing, PathBuf::from("question-closure.md"));
}

#[test]
fn test_ids_follow_the_registry_grammar() {
    let parsed: Vec<bool> = ["T-hol", "RUN-cost-js", "SDK-examples-7", "UI01-a11y", "RND-3d", "UI-01", "run-sink", "RUN-", "RUN-a--b"]
        .iter()
        .map(|text| TestId::parse(text).is_some())
        .collect();
    assert_eq!(parsed, vec![true, true, true, true, false, false, false, false, false]);
}
