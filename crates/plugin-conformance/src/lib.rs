#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod docs;
mod id;
mod markdown;
mod rules;

use std::path::{Path, PathBuf};

pub use id::TestId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub file: PathBuf,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    IllFormedId { at: Location, text: String },
    DuplicateId { id: TestId, first: Location, again: Location },
    UndefinedId { at: Location, id: TestId },
    UnmarkedReference { at: Location, id: TestId },
    EmptyWildcard { at: Location, pattern: String },
    OpenQuestion { at: Location, question: String },
    UnlinkedClosure { at: Location, question: String, missing: ClosureLink },
    BrokenLink { at: Location, target: String, fault: LinkFault },
    UntargetedRef { at: Location, reference: String },
    IncompleteDefect { at: Location, defect: String, missing: DefectField },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosureLink {
    Decision,
    Contract,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkFault {
    Absolute,
    OutsideDocs,
    MissingFile,
    MissingAnchor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefectField {
    Owner,
    ExitPhase,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot read {path}: {source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("the doc set has no {0}")]
    MissingDocument(PathBuf),
    #[error("{file} has no table with a {header} header and at least one row")]
    MissingTable { file: PathBuf, header: &'static str },
    #[error("the doc set has no implementation/*-acceptance.md")]
    NoAcceptanceFiles,
}

pub fn check(docs_root: &Path) -> Result<Vec<Finding>, Error> {
    let docs = docs::DocSet::load(docs_root)?;
    rules::run(&docs)
}
