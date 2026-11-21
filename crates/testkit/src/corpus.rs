mod disk;
mod manifest;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub use disk::{Committed, open};
pub use manifest::{Entry, FileDigest, Gap, Manifest};

pub const MANIFEST: &str = "corpus.toml";
pub const ATTRIBUTES: &str = ".gitattributes";
pub const ATTRIBUTES_TEXT: &str = "* -text\n";
pub const MAX_FILE_BYTES: u64 = 16 << 20;
pub const MAX_MANIFEST_BYTES: u64 = 1 << 20;
pub const MAX_MEMBERS: usize = 256;
pub const MAX_FILES: usize = 64;
pub const MAX_EXPECT_CHARS: usize = 240;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Name(String),
    Kind(String),
    Expect(String),
    Path { member: String, path: String },
    Duplicate(String),
    DuplicateFile { member: String, path: String },
    NoFiles(String),
    TooManyMembers,
    TooManyFiles(String),
    TooLarge(PathBuf),
    Io { path: PathBuf, message: String },
    Manifest(String),
    Digest { path: PathBuf, listed: String, actual: String },
    NoMember(String),
    NoFile { member: String, path: String },
    NoKinds,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Name(name) => write!(f, "`{name}` is not a member or app name: 1 to 64 of a-z, 0-9 and -"),
            Error::Kind(kind) => write!(f, "`{kind}` is not a kind name: 1 to 64 of a-z, 0-9 and -"),
            Error::Expect(member) => write!(f, "{member}: the expected behaviour must be one line of 1 to {MAX_EXPECT_CHARS} characters"),
            Error::Path { member, path } => write!(f, "{member}: `{path}` is not a corpus file path"),
            Error::Duplicate(member) => write!(f, "member {member} is listed twice"),
            Error::DuplicateFile { member, path } => write!(f, "{member}: {path} is listed twice"),
            Error::NoFiles(member) => write!(f, "{member} has no files"),
            Error::TooManyMembers => write!(f, "more than {MAX_MEMBERS} members"),
            Error::TooManyFiles(member) => write!(f, "{member} has more than {MAX_FILES} files"),
            Error::TooLarge(path) => write!(f, "{} is larger than {MAX_FILE_BYTES} bytes", path.display()),
            Error::Io { path, message } => write!(f, "{}: {message}", path.display()),
            Error::Manifest(message) => write!(f, "{MANIFEST}: {message}"),
            Error::Digest { path, listed, actual } => write!(f, "{}: sha256 {actual}, {MANIFEST} lists {listed}", path.display()),
            Error::NoMember(member) => write!(f, "the corpus has no member {member}"),
            Error::NoFile { member, path } => write!(f, "member {member} has no file {path}"),
            Error::NoKinds => write!(f, "coverage needs at least one kind"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Drift {
    Missing(String),
    Changed { path: String, committed: String, generated: String },
    Stray(String),
    Manifest,
}

impl fmt::Display for Drift {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Drift::Missing(path) => write!(f, "missing: {path}"),
            Drift::Changed { path, committed, generated } => write!(f, "changed: {path} is {committed}, the generator makes {generated}"),
            Drift::Stray(path) => write!(f, "stray: {path} is not a corpus file"),
            Drift::Manifest => write!(f, "changed: {MANIFEST} differs from the generated listing"),
        }
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Member {
    kind: String,
    expect: String,
    files: BTreeMap<String, Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Corpus {
    app: String,
    members: BTreeMap<String, Member>,
}

impl Corpus {
    pub fn new(app: &str) -> Result<Corpus, Error> {
        if !manifest::is_name(app) {
            return Err(Error::Name(app.to_owned()));
        }
        Ok(Corpus { app: app.to_owned(), members: BTreeMap::new() })
    }

    pub fn add(&mut self, name: &str, kind: &str, expect: &str, files: Vec<(String, Vec<u8>)>) -> Result<(), Error> {
        manifest::check_member(name, kind, expect)?;
        if self.members.contains_key(name) {
            return Err(Error::Duplicate(name.to_owned()));
        }
        if self.members.len() >= MAX_MEMBERS {
            return Err(Error::TooManyMembers);
        }
        if files.is_empty() {
            return Err(Error::NoFiles(name.to_owned()));
        }
        if files.len() > MAX_FILES {
            return Err(Error::TooManyFiles(name.to_owned()));
        }
        let mut by_path = BTreeMap::new();
        for (path, bytes) in files {
            if !manifest::is_file_path(&path) {
                return Err(Error::Path { member: name.to_owned(), path });
            }
            if bytes.len() as u64 > MAX_FILE_BYTES {
                return Err(Error::TooLarge(PathBuf::from(name).join(&path)));
            }
            if by_path.contains_key(&path) {
                return Err(Error::DuplicateFile { member: name.to_owned(), path });
            }
            by_path.insert(path, bytes);
        }
        self.members.insert(name.to_owned(), Member { kind: kind.to_owned(), expect: expect.to_owned(), files: by_path });
        Ok(())
    }

    pub fn manifest(&self) -> Manifest {
        let member = self
            .members
            .iter()
            .map(|(name, m)| Entry {
                name: name.clone(),
                kind: m.kind.clone(),
                app: self.app.clone(),
                expect: m.expect.clone(),
                files: m.files.iter().map(|(path, bytes)| FileDigest { path: path.clone(), sha256: sha256_hex(bytes) }).collect(),
            })
            .collect();
        Manifest { member }
    }

    fn files(&self) -> BTreeMap<String, &[u8]> {
        let mut files: BTreeMap<String, &[u8]> =
            self.members.iter().flat_map(|(name, m)| m.files.iter().map(move |(path, bytes)| (format!("{name}/{path}"), bytes.as_slice()))).collect();
        files.insert(ATTRIBUTES.to_owned(), ATTRIBUTES_TEXT.as_bytes());
        files
    }

    pub fn check(&self, root: &Path) -> Result<Vec<Drift>, Error> {
        let rendered = self.manifest().render();
        let mut drift = Vec::new();
        match disk::read_bounded(&root.join(MANIFEST), MAX_MANIFEST_BYTES)? {
            None => drift.push(Drift::Missing(MANIFEST.to_owned())),
            Some(text) if text != rendered.as_bytes() => drift.push(Drift::Manifest),
            Some(_) => {}
        }
        let files = self.files();
        for (path, bytes) in &files {
            match disk::read_bounded(&root.join(path), MAX_FILE_BYTES)? {
                None => drift.push(Drift::Missing(path.clone())),
                Some(on_disk) if on_disk != *bytes => {
                    drift.push(Drift::Changed { path: path.clone(), committed: sha256_hex(&on_disk), generated: sha256_hex(bytes) })
                }
                Some(_) => {}
            }
        }
        for path in disk::walk(root)? {
            if path != MANIFEST && !files.contains_key(&path) {
                drift.push(Drift::Stray(path));
            }
        }
        Ok(drift)
    }

    pub fn write(&self, root: &Path) -> Result<(), Error> {
        let files = self.files();
        for path in disk::walk(root)? {
            if path != MANIFEST && !files.contains_key(&path) {
                disk::remove(&root.join(&path))?;
            }
        }
        disk::prune_empty_dirs(root)?;
        for (path, bytes) in files {
            disk::write_if_changed(&root.join(path), bytes)?;
        }
        disk::write_if_changed(&root.join(MANIFEST), self.manifest().render().as_bytes())
    }
}
