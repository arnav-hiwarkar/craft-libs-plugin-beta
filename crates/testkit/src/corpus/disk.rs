use std::collections::BTreeSet;
use std::fs;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use super::manifest::Manifest;
use super::{Error, MANIFEST, MAX_FILE_BYTES, MAX_MANIFEST_BYTES, sha256_hex};

const MAX_DEPTH: usize = 10;
const MAX_ENTRIES: usize = 100_000;

fn io(path: &Path, error: &std::io::Error) -> Error {
    Error::Io { path: path.to_owned(), message: error.to_string() }
}

pub(super) fn read_bounded(path: &Path, max: u64) -> Result<Option<Vec<u8>>, Error> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io(path, &e)),
    };
    if !meta.is_file() {
        return Ok(None);
    }
    let file = fs::File::open(path).map_err(|e| io(path, &e))?;
    let mut bytes = Vec::new();
    file.take(max.saturating_add(1)).read_to_end(&mut bytes).map_err(|e| io(path, &e))?;
    if bytes.len() as u64 > max {
        return Err(Error::TooLarge(path.to_owned()));
    }
    Ok(Some(bytes))
}

pub(super) fn walk(root: &Path) -> Result<Vec<String>, Error> {
    let mut found = Vec::new();
    let mut pending = vec![(root.to_owned(), String::new(), 0usize)];
    let mut seen = 0usize;
    while let Some((dir, prefix, depth)) = pending.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound && depth == 0 => return Ok(found),
            Err(e) => return Err(io(&dir, &e)),
        };
        for entry in entries {
            let entry = entry.map_err(|e| io(&dir, &e))?;
            seen += 1;
            if seen > MAX_ENTRIES {
                return Err(Error::Io { path: root.to_owned(), message: format!("more than {MAX_ENTRIES} entries") });
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let relative = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
            let kind = entry.file_type().map_err(|e| io(&entry.path(), &e))?;
            if kind.is_dir() && depth < MAX_DEPTH {
                pending.push((entry.path(), relative, depth + 1));
            } else {
                found.push(relative);
            }
        }
    }
    found.sort();
    Ok(found)
}

pub(super) fn write_if_changed(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    if read_bounded(path, MAX_FILE_BYTES)?.as_deref() == Some(bytes) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| io(parent, &e))?;
    }
    fs::write(path, bytes).map_err(|e| io(path, &e))
}

#[derive(Debug)]
pub struct Committed {
    root: PathBuf,
    manifest: Manifest,
    read: Mutex<BTreeSet<String>>,
}

pub fn open(root: &Path) -> Result<Committed, Error> {
    let manifest_path = root.join(MANIFEST);
    let bytes = read_bounded(&manifest_path, MAX_MANIFEST_BYTES)?
        .ok_or_else(|| Error::Io { path: manifest_path.clone(), message: "not found".to_owned() })?;
    let text = String::from_utf8(bytes).map_err(|_| Error::Manifest("not UTF-8".to_owned()))?;
    let committed = Committed { root: root.to_owned(), manifest: Manifest::parse(&text)?, read: Mutex::new(BTreeSet::new()) };
    for entry in &committed.manifest.member {
        for file in &entry.files {
            committed.verified(&entry.name, &file.path)?;
        }
    }
    Ok(committed)
}

impl Committed {
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn unread(&self) -> Vec<String> {
        let read = self.read.lock().unwrap_or_else(PoisonError::into_inner);
        self.manifest.member.iter().map(|entry| entry.name.clone()).filter(|name| !read.contains(name)).collect()
    }

    fn mark(&self, member: &str) {
        self.read.lock().unwrap_or_else(PoisonError::into_inner).insert(member.to_owned());
    }

    pub fn dir(&self, member: &str) -> Result<PathBuf, Error> {
        let dir = self.manifest.entry(member).map(|entry| self.root.join(&entry.name)).ok_or_else(|| Error::NoMember(member.to_owned()))?;
        self.mark(member);
        Ok(dir)
    }

    fn listed(&self, member: &str, file: &str) -> Result<(PathBuf, &str), Error> {
        let entry = self.manifest.entry(member).ok_or_else(|| Error::NoMember(member.to_owned()))?;
        let listed = entry.files.iter().find(|f| f.path == file).ok_or_else(|| Error::NoFile { member: member.to_owned(), path: file.to_owned() })?;
        Ok((self.root.join(&entry.name).join(&listed.path), &listed.sha256))
    }

    pub fn path(&self, member: &str, file: &str) -> Result<PathBuf, Error> {
        let path = self.listed(member, file).map(|(path, _)| path)?;
        self.mark(member);
        Ok(path)
    }

    pub fn bytes(&self, member: &str, file: &str) -> Result<Vec<u8>, Error> {
        let bytes = self.verified(member, file)?;
        self.mark(member);
        Ok(bytes)
    }

    fn verified(&self, member: &str, file: &str) -> Result<Vec<u8>, Error> {
        let (path, listed) = self.listed(member, file)?;
        let bytes = read_bounded(&path, MAX_FILE_BYTES)?.ok_or_else(|| Error::Io { path: path.clone(), message: "not found".to_owned() })?;
        let actual = sha256_hex(&bytes);
        if actual != listed {
            return Err(Error::Digest { path, listed: listed.to_owned(), actual });
        }
        Ok(bytes)
    }

    pub fn copy_member(&self, member: &str, to: &Path) -> Result<(), Error> {
        let entry = self.manifest.entry(member).ok_or_else(|| Error::NoMember(member.to_owned()))?;
        for file in &entry.files {
            write_if_changed(&to.join(&file.path), &self.bytes(member, &file.path)?)?;
        }
        Ok(())
    }
}
