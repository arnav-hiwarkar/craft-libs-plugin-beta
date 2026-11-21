use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde::Deserialize;

use crate::name::{ArgsHash, Class, Name, NameError};

pub const FORMAT: i64 = 1;
pub const MAX_LOCK_BYTES: usize = 8 << 20;
pub const MAX_ENTRIES: usize = 65_536;
pub const MAX_ALIASES: usize = 64;
pub const MAX_MIGRATIONS: usize = 64;
pub const MAX_TOMBSTONES: usize = 65_536;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub from: ArgsHash,
    pub name: Name,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub(crate) id: Name,
    pub(crate) current: Name,
    pub(crate) class: Class,
    pub(crate) args: Option<ArgsHash>,
    pub(crate) aliases: Vec<Name>,
    pub(crate) migrations: Vec<Step>,
}

impl Entry {
    pub(crate) fn new(id: Name, class: Class, args: Option<ArgsHash>) -> Entry {
        Entry { current: id.clone(), id, class, args, aliases: Vec::new(), migrations: Vec::new() }
    }

    pub fn id(&self) -> &Name {
        &self.id
    }

    pub fn current(&self) -> &Name {
        &self.current
    }

    pub fn class(&self) -> &Class {
        &self.class
    }

    pub fn args(&self) -> Option<ArgsHash> {
        self.args
    }

    pub fn aliases(&self) -> &[Name] {
        &self.aliases
    }

    pub fn migrations(&self) -> &[Step] {
        &self.migrations
    }

    pub fn names(&self) -> impl Iterator<Item = &Name> {
        std::iter::once(&self.id).chain((self.current != self.id).then_some(&self.current)).chain(&self.aliases)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lock {
    pub(crate) entries: BTreeMap<Name, Entry>,
    pub(crate) tombstones: BTreeSet<Name>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("the lock is {0} bytes, over the {MAX_LOCK_BYTES}-byte limit")]
    TooLarge(usize),
    #[error("the lock is not valid TOML: {0}")]
    Syntax(String),
    #[error("the lock is format {0}; this build reads format {FORMAT}")]
    Format(i64),
    #[error("{at}: {error}")]
    Name { at: String, error: NameError },
    #[error("the lock has more than {MAX_ENTRIES} entries")]
    TooManyEntries,
    #[error("the lock has more than {MAX_TOMBSTONES} tombstones")]
    TooManyTombstones,
    #[error("entry {0} has more than {MAX_ALIASES} aliases")]
    TooManyAliases(Name),
    #[error("entry {0} has more than {MAX_MIGRATIONS} migrations")]
    TooManyMigrations(Name),
    #[error("entry {0} is listed twice")]
    DuplicateEntry(Name),
    #[error("entry {id} names {name} twice")]
    DuplicateName { id: Name, name: Name },
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    #[error("no entry's current id is {0}")]
    NotCurrent(Name),
    #[error("{0} is already a name or a tombstone in the lock")]
    Claimed(Name),
    #[error("entry {0} already has {MAX_ALIASES} aliases")]
    TooManyAliases(Name),
    #[error("entry {0} already has {MAX_MIGRATIONS} migrations")]
    TooManyMigrations(Name),
    #[error("entry {0} records no argument schema to migrate from")]
    NoArgs(Name),
    #[error("the lock already has {MAX_TOMBSTONES} tombstones")]
    TooManyTombstones,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileText {
    format: i64,
    #[serde(default)]
    tombstones: Vec<String>,
    #[serde(default)]
    entry: Vec<EntryText>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryText {
    id: String,
    current: Option<String>,
    class: String,
    args: Option<String>,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    migrations: Vec<StepText>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StepText {
    from: String,
    name: String,
}

fn name(text: &str, at: impl FnOnce() -> String) -> Result<Name, ParseError> {
    Name::new(text).map_err(|error| ParseError::Name { at: at(), error })
}

impl Lock {
    pub fn parse(text: &str) -> Result<Lock, ParseError> {
        if text.len() > MAX_LOCK_BYTES {
            return Err(ParseError::TooLarge(text.len()));
        }
        let file: FileText = toml::from_str(text).map_err(|e| ParseError::Syntax(e.message().to_owned()))?;
        if file.format != FORMAT {
            return Err(ParseError::Format(file.format));
        }
        if file.entry.len() > MAX_ENTRIES {
            return Err(ParseError::TooManyEntries);
        }
        if file.tombstones.len() > MAX_TOMBSTONES {
            return Err(ParseError::TooManyTombstones);
        }
        let mut lock = Lock::default();
        for t in &file.tombstones {
            lock.tombstones.insert(name(t, || "tombstones".to_owned())?);
        }
        for e in file.entry {
            let entry = parse_entry(e)?;
            if lock.entries.contains_key(&entry.id) {
                return Err(ParseError::DuplicateEntry(entry.id));
            }
            lock.entries.insert(entry.id.clone(), entry);
        }
        Ok(lock)
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.values()
    }

    pub fn tombstones(&self) -> impl Iterator<Item = &Name> {
        self.tombstones.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn to_text(&self) -> String {
        let mut out = format!("format = {FORMAT}\n");
        if !self.tombstones.is_empty() {
            out.push_str("tombstones = [\n");
            for t in &self.tombstones {
                let _ = writeln!(out, "  \"{t}\",");
            }
            out.push_str("]\n");
        }
        for e in self.entries.values() {
            let _ = write!(out, "\n[[entry]]\nid = \"{}\"\n", e.id);
            if e.current != e.id {
                let _ = writeln!(out, "current = \"{}\"", e.current);
            }
            let _ = writeln!(out, "class = \"{}\"", e.class);
            if let Some(args) = e.args {
                let _ = writeln!(out, "args = \"{args}\"");
            }
            if !e.aliases.is_empty() {
                let list: Vec<String> = e.aliases.iter().map(|a| format!("\"{a}\"")).collect();
                let _ = writeln!(out, "aliases = [{}]", list.join(", "));
            }
            if !e.migrations.is_empty() {
                let list: Vec<String> = e.migrations.iter().map(|m| format!("{{ from = \"{}\", name = \"{}\" }}", m.from, m.name)).collect();
                let _ = writeln!(out, "migrations = [{}]", list.join(", "));
            }
        }
        out
    }

    fn claimed(&self, name: &Name) -> Option<&Entry> {
        self.entries.values().find(|e| e.names().any(|n| n == name))
    }

    fn by_current(&mut self, current: &Name) -> Result<&mut Entry, EditError> {
        self.entries.values_mut().find(|e| &e.current == current).ok_or_else(|| EditError::NotCurrent(current.clone()))
    }

    pub fn rename(&mut self, current: &Name, to: Name) -> Result<(), EditError> {
        let owner = self.by_current(current)?.id.clone();
        if self.tombstones.contains(&to) || self.claimed(&to).is_some_and(|e| e.id != owner) {
            return Err(EditError::Claimed(to));
        }
        let entry = self.by_current(current)?;
        if &to == current {
            return Ok(());
        }
        entry.aliases.retain(|a| a != &to);
        let old = std::mem::replace(&mut entry.current, to);
        if old != entry.id && !entry.aliases.contains(&old) {
            if entry.aliases.len() >= MAX_ALIASES {
                entry.current = old;
                return Err(EditError::TooManyAliases(entry.id.clone()));
            }
            entry.aliases.push(old);
        }
        Ok(())
    }

    pub fn retire(&mut self, current: &Name) -> Result<(), EditError> {
        let id = self.by_current(current)?.id.clone();
        let Some(entry) = self.entries.get(&id) else { return Err(EditError::NotCurrent(current.clone())) };
        let names: Vec<Name> = entry.names().cloned().collect();
        if self.tombstones.len().saturating_add(names.len()) > MAX_TOMBSTONES {
            return Err(EditError::TooManyTombstones);
        }
        self.entries.remove(&id);
        self.tombstones.extend(names);
        Ok(())
    }

    pub fn migrate(&mut self, current: &Name, migration: Name) -> Result<(), EditError> {
        let entry = self.by_current(current)?;
        let from = entry.args.ok_or_else(|| EditError::NoArgs(entry.id.clone()))?;
        let step = Step { from, name: migration };
        if entry.migrations.contains(&step) {
            return Ok(());
        }
        if entry.migrations.len() >= MAX_MIGRATIONS {
            return Err(EditError::TooManyMigrations(entry.id.clone()));
        }
        entry.migrations.push(step);
        Ok(())
    }
}

fn parse_entry(e: EntryText) -> Result<Entry, ParseError> {
    let id = name(&e.id, || "entry id".to_owned())?;
    let at = |what: &str| format!("entry {}: {what}", e.id);
    let current = match &e.current {
        Some(c) => name(c, || at("current"))?,
        None => id.clone(),
    };
    let class = Class::new(&e.class).map_err(|error| ParseError::Name { at: at("class"), error })?;
    let args = match &e.args {
        Some(a) => Some(ArgsHash::parse(a).map_err(|error| ParseError::Name { at: at("args"), error })?),
        None => None,
    };
    if e.aliases.len() > MAX_ALIASES {
        return Err(ParseError::TooManyAliases(id));
    }
    if e.migrations.len() > MAX_MIGRATIONS {
        return Err(ParseError::TooManyMigrations(id));
    }
    let mut aliases = Vec::with_capacity(e.aliases.len());
    for a in &e.aliases {
        aliases.push(name(a, || at("aliases"))?);
    }
    let mut migrations = Vec::with_capacity(e.migrations.len());
    for m in &e.migrations {
        let from = ArgsHash::parse(&m.from).map_err(|error| ParseError::Name { at: at("migrations"), error })?;
        migrations.push(Step { from, name: name(&m.name, || at("migrations"))? });
    }
    let entry = Entry { id, current, class, args, aliases, migrations };
    let mut seen = BTreeSet::new();
    if let Some(dup) = entry.names().find(|n| !seen.insert(*n)) {
        return Err(ParseError::DuplicateName { id: entry.id.clone(), name: dup.clone() });
    }
    Ok(entry)
}
