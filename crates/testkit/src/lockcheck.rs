#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Deserialize;

pub const MAX_LOCK_BYTES: usize = 16 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Repo {
    Upstream,
    Fork,
}

impl Repo {
    const ALL: [Repo; 2] = [Repo::Upstream, Repo::Fork];

    pub fn url(self) -> &'static str {
        match self {
            Repo::Upstream => "https://github.com/storytold/craft-libs",
            Repo::Fork => "https://github.com/arnav-hiwarkar/craft-libs-plugin-beta",
        }
    }

    fn identify(url: &str) -> Option<Repo> {
        let url = canonical(url);
        Repo::ALL.into_iter().find(|repo| canonical(repo.url()) == url)
    }
}

fn canonical(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let rest = rest.split_once('@').filter(|(user, _)| !user.contains('/')).map_or(rest, |(_, host)| host);
    let rest = rest.trim_end_matches('/');
    rest.strip_suffix(".git").unwrap_or(rest).to_ascii_lowercase()
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Commit(String);

impl Commit {
    pub fn parse(text: &str) -> Option<Commit> {
        let hex = text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        (matches!(text.len(), 40 | 64) && hex).then(|| Commit(text.to_owned()))
    }
}

impl fmt::Display for Commit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Source {
    pub repo: Repo,
    pub reference: String,
    pub commit: Commit,
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.reference.as_str() {
            "" => write!(f, "{}#{}", self.repo.url(), self.commit),
            reference => write!(f, "{}?{reference}#{}", self.repo.url(), self.commit),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Policy {
    exceptions: BTreeMap<String, String>,
}

impl Policy {
    pub fn except(mut self, package: &str, reason: &str) -> Result<Policy, Error> {
        if !is_package_name(package) {
            return Err(Error::ExceptionName(package.to_owned()));
        }
        if reason.trim().is_empty() || reason.chars().any(char::is_control) {
            return Err(Error::Reason(package.to_owned()));
        }
        if self.exceptions.insert(package.to_owned(), reason.to_owned()).is_some() {
            return Err(Error::DuplicateException(package.to_owned()));
        }
        Ok(self)
    }
}

fn is_package_name(text: &str) -> bool {
    (1..=64).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Finding {
    Tied(Vec<Source>),
    Duplicate { package: String, sources: Vec<Source> },
    Stray { package: String, source: Source, main: Source },
    UnusedException { package: String, reason: String },
}

fn list(sources: &[Source]) -> String {
    sources.iter().map(Source::to_string).collect::<Vec<_>>().join(", ")
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Finding::Tied(sources) => write!(f, "no single framework source: {} hold the same number of packages", list(sources)),
            Finding::Duplicate { package, sources } => write!(f, "{package} resolves from {} sources: {}", sources.len(), list(sources)),
            Finding::Stray { package, source, main } => write!(f, "{package} resolves from {source}, not the framework source {main}"),
            Finding::UnusedException { package, reason } => {
                write!(f, "the exception for {package} ({reason}) is unused: no copy of it resolves outside the framework source")
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    TooLarge(usize),
    Parse(String),
    Empty,
    NoFramework,
    Commit { package: String, source: String },
    ExceptionName(String),
    Reason(String),
    DuplicateException(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::TooLarge(bytes) => write!(f, "the lock is {bytes} bytes, more than {MAX_LOCK_BYTES}"),
            Error::Parse(message) => write!(f, "the lock is not TOML: {message}"),
            Error::Empty => write!(f, "the lock lists no packages"),
            Error::NoFramework => write!(f, "the lock has no craft-libs package outside the exceptions"),
            Error::Commit { package, source } => write!(f, "{package}: `{source}` is a craft-libs source without a full commit"),
            Error::ExceptionName(package) => write!(f, "`{package}` is not a package name"),
            Error::Reason(package) => write!(f, "the exception for {package} needs a one-line reason"),
            Error::DuplicateException(package) => write!(f, "{package} is excepted twice"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Deserialize)]
struct Lock {
    #[serde(default)]
    package: Vec<Package>,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    source: Option<String>,
}

impl Package {
    fn framework_source(&self) -> Result<Option<Source>, Error> {
        let Some(git) = self.source.as_deref().and_then(|source| source.strip_prefix("git+")) else {
            return Ok(None);
        };
        let (located, commit) = git.split_once('#').unwrap_or((git, ""));
        let (url, reference) = located.split_once('?').unwrap_or((located, ""));
        let Some(repo) = Repo::identify(url) else {
            return Ok(None);
        };
        let commit = Commit::parse(commit).ok_or_else(|| Error::Commit { package: self.name.clone(), source: git.to_owned() })?;
        Ok(Some(Source { repo, reference: reference.to_owned(), commit }))
    }
}

pub fn check(lock: &str, policy: &Policy) -> Result<Vec<Finding>, Error> {
    if lock.len() > MAX_LOCK_BYTES {
        return Err(Error::TooLarge(lock.len()));
    }
    let lock: Lock = toml::from_str(lock).map_err(|e| Error::Parse(e.message().to_owned()))?;
    if lock.package.is_empty() {
        return Err(Error::Empty);
    }
    let mut resolved: BTreeMap<String, BTreeSet<Source>> = BTreeMap::new();
    for package in lock.package {
        if let Some(source) = package.framework_source()? {
            resolved.entry(package.name).or_default().insert(source);
        }
    }
    let excepted = |package: &str| policy.exceptions.contains_key(package);
    let mut held: BTreeMap<&Source, BTreeSet<&str>> = BTreeMap::new();
    for (package, sources) in resolved.iter().filter(|(package, _)| !excepted(package)) {
        for source in sources {
            held.entry(source).or_default().insert(package);
        }
    }
    let most = held.values().map(BTreeSet::len).max().ok_or(Error::NoFramework)?;
    let mains: Vec<Source> = held.into_iter().filter(|(_, packages)| packages.len() == most).map(|(source, _)| source.clone()).collect();

    let mut findings = Vec::new();
    let main = match <[Source; 1]>::try_from(mains) {
        Ok([main]) => Some(main),
        Err(tied) => {
            findings.push(Finding::Tied(tied));
            None
        }
    };
    for (package, sources) in resolved.iter().filter(|(_, sources)| sources.len() > 1) {
        findings.push(Finding::Duplicate { package: package.clone(), sources: sources.iter().cloned().collect() });
    }
    let Some(main) = main else {
        return Ok(findings);
    };
    for (package, sources) in resolved.iter().filter(|(package, _)| !excepted(package)) {
        for source in sources.iter().filter(|source| **source != main) {
            findings.push(Finding::Stray { package: package.clone(), source: source.clone(), main: main.clone() });
        }
    }
    for (package, reason) in &policy.exceptions {
        if resolved.get(package).is_none_or(|sources| sources.iter().all(|source| *source == main)) {
            findings.push(Finding::UnusedException { package: package.clone(), reason: reason.clone() });
        }
    }
    Ok(findings)
}
