use std::fs;
use std::path::{Path, PathBuf};

use craft_plugin_conformance::{ClosureLink, DefectField, Error, Finding, LinkFault, Location, TestId, check};

fn copy_into(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).expect("the fixture directory is readable") {
        let entry = entry.expect("a fixture entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a fixture entry type").is_dir() {
            fs::create_dir_all(&target).expect("a fixture directory is created");
            copy_into(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("a fixture file is copied");
        }
    }
}

fn materialize(fixture: &str) -> PathBuf {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let case = Path::new(env!("CARGO_TARGET_TMPDIR")).join("plugin-conformance").join(fixture);
    let docs = case.join("docs");
    if case.exists() {
        fs::remove_dir_all(&case).expect("the previous fixture copy is removed");
    }
    fs::create_dir_all(&docs).expect("the fixture copy is created");
    fs::write(case.join("outside.md"), "# Outside\n").expect("a file outside the docs is written");
    copy_into(&fixtures.join("base"), &docs);
    let overlay = fixtures.join(fixture);
    if fixture != "base" && overlay.exists() {
        copy_into(&overlay, &docs);
    }
    docs
}

fn findings(fixture: &str) -> Vec<Finding> {
    check(&materialize(fixture)).expect("the fixture is readable")
}

fn error(docs: &Path) -> Error {
    match check(docs) {
        Err(error) => error,
        Ok(findings) => panic!("expected an error, got {findings:?}"),
    }
}

fn at(file: &str, line: usize) -> Location {
    Location { file: PathBuf::from(file), line }
}

fn id(text: &str) -> TestId {
    TestId::parse(text).expect("a well-formed test id")
}

#[test]
fn the_base_fixture_is_clean() {
    assert_eq!(findings("base"), Vec::new());
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
fn a_bare_defined_id_outside_its_defining_cell_and_code_is_unmarked() {
    let unmarked = |file: &str, line: usize| Finding::UnmarkedReference { at: at(file, line), id: id("RUN-sink") };
    assert_eq!(
        findings("unmarked-reference"),
        vec![unmarked("implementation/runtime-acceptance.md", 5), unmarked("notes.md", 3), unmarked("notes.md", 3)]
    );
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
fn an_open_question_is_reported_through_punctuation_and_markup() {
    let open = |line: usize, question: &str| Finding::OpenQuestion { at: at("question-closure.md", line), question: question.into() };
    assert_eq!(
        findings("open-question"),
        vec![open(8, "SYS-01.2"), open(9, "SYS-01.3"), open(10, "SYS-01.4"), open(11, "SYS-01.5"), open(12, "SYS-01.6"), open(13, "SYS-01.7")]
    );
}

#[test]
fn a_closure_must_link_an_existing_decision_and_contract_document() {
    let file = "question-closure.md";
    let unlinked =
        |line: usize, question: &str, missing: ClosureLink| Finding::UnlinkedClosure { at: at(file, line), question: question.into(), missing };
    assert_eq!(
        findings("unlinked-closure"),
        vec![
            unlinked(7, "SYS-01.1", ClosureLink::Contract),
            unlinked(8, "SYS-01.2", ClosureLink::Contract),
            unlinked(9, "SYS-01.3", ClosureLink::Decision),
            unlinked(10, "SYS-01.4", ClosureLink::Contract),
            Finding::BrokenLink { at: at(file, 8), target: "contracts/gone.md".into(), fault: LinkFault::MissingFile },
            Finding::BrokenLink { at: at(file, 10), target: "contracts/".into(), fault: LinkFault::MissingFile },
        ]
    );
}

#[test]
fn links_resolve_to_documents_of_the_set_and_github_heading_anchors() {
    let broken = |line: usize, target: &str, fault: LinkFault| Finding::BrokenLink { at: at("guide.md", line), target: target.into(), fault };
    assert_eq!(
        findings("broken-link"),
        vec![
            broken(11, "missing.md", LinkFault::MissingFile),
            broken(11, "other.md#nowhere", LinkFault::MissingAnchor),
            broken(11, "#repeated-2", LinkFault::MissingAnchor),
            broken(11, "sub/", LinkFault::MissingFile),
            broken(11, "Other.md", LinkFault::MissingFile),
            broken(12, "/guide.md", LinkFault::Absolute),
            broken(12, "../outside.md", LinkFault::OutsideDocs),
            broken(12, "sub/../../guide.md", LinkFault::OutsideDocs),
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
fn a_question_closure_without_a_closure_column_is_an_error() {
    let missing = match error(&materialize("renamed-closure")) {
        Error::MissingTable { file, header } => (file, header),
        other => panic!("expected a missing table, got {other:?}"),
    };
    assert_eq!(missing, (PathBuf::from("question-closure.md"), "Closure"));
}

#[test]
fn a_question_table_without_a_closure_column_is_an_error_beside_a_valid_one() {
    let missing = match error(&materialize("one-renamed-closure")) {
        Error::MissingTable { file, header } => (file, header),
        other => panic!("expected a missing table, got {other:?}"),
    };
    assert_eq!(missing, (PathBuf::from("question-closure.md"), "Closure"));
}

#[test]
fn an_acceptance_file_without_id_rows_is_an_error() {
    let missing = match error(&materialize("empty-acceptance")) {
        Error::MissingTable { file, header } => (file, header),
        other => panic!("expected a missing table, got {other:?}"),
    };
    assert_eq!(missing, (PathBuf::from("implementation/empty-acceptance.md"), "ID"));
}

#[test]
fn a_proof_inventory_without_ref_rows_is_an_error() {
    let missing = match error(&materialize("no-proofs")) {
        Error::MissingTable { file, header } => (file, header),
        other => panic!("expected a missing table, got {other:?}"),
    };
    assert_eq!(missing, (PathBuf::from("implementation/proof-inventory.md"), "ID"));
}

#[test]
fn a_doc_set_without_acceptance_files_is_an_error() {
    let docs = materialize("no-acceptance");
    fs::remove_file(docs.join("implementation/base-acceptance.md")).expect("the acceptance file is removed");
    assert!(matches!(error(&docs), Error::NoAcceptanceFiles));
}

#[test]
fn a_doc_set_without_question_closure_or_proof_inventory_is_an_error() {
    let missing = ["question-closure.md", "implementation/proof-inventory.md"].map(|document| {
        let docs = materialize(&format!("without-{}", document.replace('/', "-")));
        fs::remove_file(docs.join(document)).expect("the document is removed");
        match error(&docs) {
            Error::MissingDocument(path) => path,
            other => panic!("expected a missing document, got {other:?}"),
        }
    });
    assert_eq!(missing, [PathBuf::from("question-closure.md"), PathBuf::from("implementation/proof-inventory.md")]);
}

#[test]
fn test_ids_follow_the_registry_grammar() {
    let parsed: Vec<bool> = ["T-hol", "RUN-cost-js", "SDK-examples-7", "UI01-a11y", "RND-3d", "UI-01", "run-sink", "RUN-", "RUN-a--b"]
        .iter()
        .map(|text| TestId::parse(text).is_some())
        .collect();
    assert_eq!(parsed, vec![true, true, true, true, false, false, false, false, false]);
}
