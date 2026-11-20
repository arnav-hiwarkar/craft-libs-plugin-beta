use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::markdown::{self, Document};
use crate::{Error, LinkFault};

pub(crate) struct DocSet {
    pub(crate) docs: BTreeMap<PathBuf, Document>,
}

pub(crate) enum Resolution<'a> {
    External,
    Doc { path: &'a Path, document: &'a Document, anchor: Option<&'a str> },
    Broken(LinkFault),
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
                    let path = entry.path();
                    let text = fs::read_to_string(&path).map_err(|source| Error::Read { path, source })?;
                    docs.insert(relative, markdown::parse(&text));
                }
            }
        }
        Ok(DocSet { docs })
    }

    pub(crate) fn require(&self, path: &str) -> Result<(&Path, &Document), Error> {
        self.docs
            .get_key_value(Path::new(path))
            .map(|(path, document)| (path.as_path(), document))
            .ok_or_else(|| Error::MissingDocument(PathBuf::from(path)))
    }

    pub(crate) fn resolve<'a>(&'a self, from: &'a Path, dest: &'a str) -> Resolution<'a> {
        let before_path_end = dest.split(['/', '#', '?']).next().unwrap_or_default();
        if before_path_end.contains(':') {
            return Resolution::External;
        }
        let (path, anchor) = match dest.split_once('#') {
            Some((path, anchor)) => (path, Some(anchor)),
            None => (dest, None),
        };
        if path.starts_with('/') {
            return Resolution::Broken(LinkFault::Absolute);
        }
        let target = if path.is_empty() { Some(from.to_owned()) } else { within_docs(&from.parent().unwrap_or(Path::new("")).join(path)) };
        let Some(target) = target else {
            return Resolution::Broken(LinkFault::OutsideDocs);
        };
        match self.docs.get_key_value(&target) {
            Some((path, document)) => Resolution::Doc { path, document, anchor },
            None => Resolution::Broken(LinkFault::MissingFile),
        }
    }
}

fn within_docs(path: &Path) -> Option<PathBuf> {
    let mut parts: Vec<Component> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(_) => parts.push(component),
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(parts.iter().collect())
}
