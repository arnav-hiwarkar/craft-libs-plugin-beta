use std::collections::{HashMap, HashSet};

use crate::check::Finding;
use crate::lock::{Entry, Lock};
use crate::name::Name;

pub struct Migration<A> {
    pub name: &'static str,
    pub run: fn(A) -> Result<A, String>,
}

#[derive(Debug, PartialEq)]
pub enum Resolved<'r, A> {
    Current { id: &'r Name, args: A },
    Unknown,
    Tombstoned,
    MigrationFailed { id: &'r Name, migration: &'r Name, reason: String },
}

#[derive(Debug)]
pub struct Resolver {
    lock: Lock,
    entries: Vec<Entry>,
    names: HashMap<String, usize>,
    tombstones: HashSet<String>,
}

impl Resolver {
    pub fn new(lock: Lock) -> Result<Resolver, Vec<Finding>> {
        let conflicts: Vec<Finding> =
            crate::check::check(&lock, &[], &[]).into_iter().filter(|f| matches!(f, Finding::Conflict { .. } | Finding::Cycle { .. })).collect();
        if !conflicts.is_empty() {
            return Err(conflicts);
        }
        let entries: Vec<Entry> = lock.entries().cloned().collect();
        let mut names = HashMap::new();
        for (i, e) in entries.iter().enumerate() {
            for n in e.names() {
                names.insert(n.as_str().to_owned(), i);
            }
        }
        let tombstones = lock.tombstones().map(|t| t.as_str().to_owned()).collect();
        Ok(Resolver { lock, entries, names, tombstones })
    }

    pub fn lock(&self) -> &Lock {
        &self.lock
    }

    pub fn entry(&self, persisted: &str) -> Option<&Entry> {
        self.names.get(persisted).and_then(|&i| self.entries.get(i))
    }

    pub fn resolve<A>(&self, persisted: &str, args: A, migrations: &[Migration<A>]) -> Resolved<'_, A> {
        let Some(entry) = self.entry(persisted) else {
            return if self.tombstones.contains(persisted) { Resolved::Tombstoned } else { Resolved::Unknown };
        };
        let mut args = args;
        for step in &entry.migrations {
            let Some(m) = migrations.iter().find(|m| m.name == step.name.as_str()) else {
                return Resolved::MigrationFailed { id: &entry.current, migration: &step.name, reason: "the app registers no such migration".into() };
            };
            args = match (m.run)(args) {
                Ok(next) => next,
                Err(reason) => return Resolved::MigrationFailed { id: &entry.current, migration: &step.name, reason },
            };
        }
        Resolved::Current { id: &entry.current, args }
    }

    pub fn current(&self, persisted: &str) -> Resolved<'_, ()> {
        match self.entry(persisted) {
            Some(entry) => Resolved::Current { id: &entry.current, args: () },
            None if self.tombstones.contains(persisted) => Resolved::Tombstoned,
            None => Resolved::Unknown,
        }
    }

    pub fn persisted(&self, current: &str) -> Option<&Name> {
        self.entry(current).filter(|e| e.current.as_str() == current).map(|e| &e.id)
    }
}
