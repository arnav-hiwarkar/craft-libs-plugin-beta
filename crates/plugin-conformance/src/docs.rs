use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::Error;
use crate::markdown::{self, Document};

pub(crate) struct DocSet {
    root: PathBuf,
    pub(crate) docs: BTreeMap<PathBuf, Document>,
}

pub(crate) struct Target {
    pub(crate) path: PathBuf,
    pub(crate) anchor: Option<String>,
}

impl DocSet {
    pub(crate) fn load(root: &Path) -> Result<DocSet, Error> {
        let mut docs = BTreeMap::new();
        let mut pending = vec![PathBuf::new()];
        while let Some(dir) = pending.pop() {
            let full = root.join(&dir);
            let entries = fs::read_dir(&full).map_err(|source| Error::Read { path: full.clone(), source })?;
            for entry in entries {
                let entry = entry.map_err(|source| Error::Read { path: full.clone(), source })?;
                let kind = entry.file_type().map_err(|source| Error::Read { path: entry.path(), source })?;
                let relative = dir.join(entry.file_name());
                if kind.is_dir() {
                    pending.push(relative);
                } else if kind.is_file() && relative.extension().is_some_and(|ext| ext == "md") {
                    docs.insert(relative, read(&entry.path())?);
                }
            }
        }
        Ok(DocSet { root: root.to_owned(), docs })
    }

    pub(crate) fn require(&self, path: &str) -> Result<(&Path, &Document), Error> {
        self.docs
            .get_key_value(Path::new(path))
            .map(|(path, document)| (path.as_path(), document))
            .ok_or_else(|| Error::MissingDocument(PathBuf::from(path)))
    }

    pub(crate) fn resolve(&self, from: &Path, dest: &str) -> Option<Target> {
        let before_path_end = dest.split(['/', '#', '?']).next().unwrap_or_default();
        if before_path_end.contains(':') {
            return None;
        }
        let (path, anchor) = match dest.split_once('#') {
            Some((path, anchor)) => (path, Some(anchor.to_owned())),
            None => (dest, None),
        };
        let path = if path.is_empty() { from.to_owned() } else { normalize(&from.parent().unwrap_or(Path::new("")).join(path)) };
        Some(Target { path, anchor })
    }

    pub(crate) fn exists(&self, path: &Path) -> bool {
        self.docs.contains_key(path) || self.root.join(path).exists()
    }

    pub(crate) fn has_anchor(&self, path: &Path, anchor: &str) -> Result<bool, Error> {
        if let Some(document) = self.docs.get(path) {
            return Ok(document.anchors.contains(anchor));
        }
        if path.extension().is_some_and(|ext| ext == "md") {
            return Ok(read(&self.root.join(path))?.anchors.contains(anchor));
        }
        Ok(true)
    }
}

fn read(path: &Path) -> Result<Document, Error> {
    let text = fs::read_to_string(path).map_err(|source| Error::Read { path: path.to_owned(), source })?;
    Ok(markdown::parse(&text))
}

fn normalize(path: &Path) -> PathBuf {
    let mut parts: Vec<Component> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir if matches!(parts.last(), Some(Component::Normal(_))) => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.iter().collect()
}
