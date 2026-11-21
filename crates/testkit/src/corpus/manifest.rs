use std::collections::BTreeSet;
use std::fmt::Write as _;

use serde::Deserialize;

use super::{Error, MAX_EXPECT_CHARS, MAX_FILES, MAX_MEMBERS};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    #[serde(default)]
    pub member: Vec<Entry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub name: String,
    pub kind: String,
    pub app: String,
    pub expect: String,
    pub files: Vec<FileDigest>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileDigest {
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Gap {
    Uncovered(String),
    Unknown { member: String, kind: String },
}

pub(super) fn is_name(text: &str) -> bool {
    (1..=64).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub(super) fn is_file_path(path: &str) -> bool {
    let component = |part: &str| {
        (1..=128).contains(&part.len())
            && !part.starts_with('.')
            && !part.ends_with(' ')
            && !part.starts_with(' ')
            && part.bytes().all(|b| b.is_ascii_alphanumeric() || b" ._-".contains(&b))
    };
    let parts: Vec<&str> = path.split('/').collect();
    path.len() <= 512 && parts.len() <= 8 && parts.into_iter().all(component)
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(super) fn check_member(name: &str, kind: &str, expect: &str) -> Result<(), Error> {
    if !is_name(name) {
        return Err(Error::Name(name.to_owned()));
    }
    if !is_name(kind) {
        return Err(Error::Kind(kind.to_owned()));
    }
    let chars = expect.chars().count();
    if chars == 0 || chars > MAX_EXPECT_CHARS || expect.chars().any(char::is_control) || expect.trim() != expect {
        return Err(Error::Expect(name.to_owned()));
    }
    Ok(())
}

fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        if matches!(c, '"' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Manifest, Error> {
        let manifest: Manifest = toml::from_str(text).map_err(|e| Error::Manifest(e.message().to_owned()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<(), Error> {
        if self.member.len() > MAX_MEMBERS {
            return Err(Error::TooManyMembers);
        }
        let mut names = BTreeSet::new();
        for entry in &self.member {
            check_member(&entry.name, &entry.kind, &entry.expect)?;
            if !is_name(&entry.app) {
                return Err(Error::Name(entry.app.clone()));
            }
            if !names.insert(entry.name.as_str()) {
                return Err(Error::Duplicate(entry.name.clone()));
            }
            if entry.files.is_empty() {
                return Err(Error::NoFiles(entry.name.clone()));
            }
            if entry.files.len() > MAX_FILES {
                return Err(Error::TooManyFiles(entry.name.clone()));
            }
            let mut paths = BTreeSet::new();
            for file in &entry.files {
                if !is_file_path(&file.path) {
                    return Err(Error::Path { member: entry.name.clone(), path: file.path.clone() });
                }
                if !is_sha256(&file.sha256) {
                    return Err(Error::Manifest(format!("{}/{}: `{}` is not a lower-case sha256", entry.name, file.path, file.sha256)));
                }
                if !paths.insert(file.path.as_str()) {
                    return Err(Error::DuplicateFile { member: entry.name.clone(), path: file.path.clone() });
                }
            }
        }
        Ok(())
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for (n, entry) in self.member.iter().enumerate() {
            if n > 0 {
                out.push('\n');
            }
            let _ = writeln!(out, "[[member]]");
            let _ = writeln!(out, "name = {}", quoted(&entry.name));
            let _ = writeln!(out, "kind = {}", quoted(&entry.kind));
            let _ = writeln!(out, "app = {}", quoted(&entry.app));
            let _ = writeln!(out, "expect = {}", quoted(&entry.expect));
            let _ = writeln!(out, "files = [");
            for file in &entry.files {
                let _ = writeln!(out, "    {{ path = {}, sha256 = {} }},", quoted(&file.path), quoted(&file.sha256));
            }
            let _ = writeln!(out, "]");
        }
        out
    }

    pub fn entry(&self, name: &str) -> Option<&Entry> {
        self.member.iter().find(|entry| entry.name == name)
    }

    pub fn coverage(&self, kinds: &[&str]) -> Result<Vec<Gap>, Error> {
        if kinds.is_empty() {
            return Err(Error::NoKinds);
        }
        let mut gaps: Vec<Gap> = kinds
            .iter()
            .filter(|kind| !self.member.iter().any(|entry| entry.kind == **kind))
            .map(|kind| Gap::Uncovered((*kind).to_owned()))
            .collect();
        gaps.extend(
            self.member
                .iter()
                .filter(|entry| !kinds.contains(&entry.kind.as_str()))
                .map(|entry| Gap::Unknown { member: entry.name.clone(), kind: entry.kind.clone() }),
        );
        Ok(gaps)
    }
}
