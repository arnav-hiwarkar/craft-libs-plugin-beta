use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::lock::{Entry, Lock};
use crate::name::{ArgsHash, Class, Name};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Registered {
    pub id: Name,
    pub class: Class,
    pub args: Option<ArgsHash>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Finding {
    EmptyRegistry,
    EmptyLock,
    Unlisted { id: Name },
    Stale { id: Name },
    Duplicate { id: Name },
    Missing { id: Name },
    Reused { id: Name },
    ArgsChanged { id: Name, locked: ArgsHash, current: ArgsHash },
    UnknownMigration { id: Name, migration: Name },
    Conflict { name: Name },
    Cycle { ids: Vec<Name> },
}

impl Finding {
    pub fn needs_person(&self) -> bool {
        !matches!(self, Finding::EmptyLock | Finding::Unlisted { .. } | Finding::Stale { .. })
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Finding::EmptyRegistry => write!(f, "the registry lists no ids, so the lock cannot be checked"),
            Finding::EmptyLock => write!(f, "the lock lists no ids; seed it with `engine-lock --update`"),
            Finding::Unlisted { id } => write!(f, "{id} is registered but not in the lock; run `engine-lock --update`"),
            Finding::Stale { id } => write!(f, "{id}'s class or argument schema changed with a migration in place; run `engine-lock --update`"),
            Finding::Duplicate { id } => write!(f, "{id} is registered twice"),
            Finding::Missing { id } => {
                write!(
                    f,
                    "{id} is in the lock but no longer registered; record its new id with `--rename {id}=NEW` or remove it with `--retire {id}`"
                )
            }
            Finding::Reused { id } => {
                write!(f, "{id} is registered again after it was renamed or retired; saved references to it mean the old command")
            }
            Finding::ArgsChanged { id, locked, current } => {
                write!(f, "{id}'s argument schema changed from {locked} to {current}; add a migration with `--migrate {id}=NAME`")
            }
            Finding::UnknownMigration { id, migration } => write!(f, "{id} names migration {migration}, which the app does not register"),
            Finding::Conflict { name } => write!(f, "{name} is claimed by more than one entry or tombstone"),
            Finding::Cycle { ids } => {
                let ids: Vec<&str> = ids.iter().map(Name::as_str).collect();
                write!(f, "the aliases of {} form a cycle", ids.join(", "))
            }
        }
    }
}

pub fn check(lock: &Lock, registry: &[Registered], migrations: &[&str]) -> Vec<Finding> {
    let mut findings = Vec::new();
    if registry.is_empty() {
        findings.push(Finding::EmptyRegistry);
    }
    if lock.is_empty() {
        findings.push(Finding::EmptyLock);
    }
    findings.extend(conflicts(lock));
    let claims = claims(lock);
    let mut seen = BTreeSet::new();
    for r in registry {
        if !seen.insert(&r.id) {
            findings.push(Finding::Duplicate { id: r.id.clone() });
            continue;
        }
        if lock.tombstones.contains(&r.id) {
            findings.push(Finding::Reused { id: r.id.clone() });
            continue;
        }
        match claims.get(&r.id).map(Vec::as_slice) {
            None | Some([]) => findings.push(Finding::Unlisted { id: r.id.clone() }),
            Some([e]) if e.current != r.id => findings.push(Finding::Reused { id: r.id.clone() }),
            Some([e]) => findings.extend(compare(e, r)),
            Some(_) => {}
        }
    }
    let registered: BTreeSet<&Name> = registry.iter().map(|r| &r.id).collect();
    for e in lock.entries.values() {
        if !registered.contains(&e.current) {
            findings.push(Finding::Missing { id: e.current.clone() });
        }
        for step in &e.migrations {
            if !migrations.contains(&step.name.as_str()) {
                findings.push(Finding::UnknownMigration { id: e.current.clone(), migration: step.name.clone() });
            }
        }
    }
    findings
}

pub fn update(lock: &Lock, registry: &[Registered], migrations: &[&str]) -> Result<Lock, Vec<Finding>> {
    let blocking: Vec<Finding> = check(lock, registry, migrations).into_iter().filter(Finding::needs_person).collect();
    if !blocking.is_empty() {
        return Err(blocking);
    }
    let mut next = lock.clone();
    let by_current: BTreeMap<Name, Name> = lock.entries.values().map(|e| (e.current.clone(), e.id.clone())).collect();
    for r in registry {
        match by_current.get(&r.id).and_then(|id| next.entries.get_mut(id)) {
            Some(e) => {
                e.class = r.class.clone();
                e.args = r.args;
            }
            None => {
                next.entries.insert(r.id.clone(), Entry::new(r.id.clone(), r.class.clone(), r.args));
            }
        }
    }
    Ok(next)
}

fn compare(e: &Entry, r: &Registered) -> Option<Finding> {
    match (e.args, r.args) {
        (Some(locked), Some(current)) if locked != current && !e.migrations.iter().any(|m| m.from == locked) => {
            Some(Finding::ArgsChanged { id: r.id.clone(), locked, current })
        }
        (locked, current) if locked != current || e.class != r.class => Some(Finding::Stale { id: r.id.clone() }),
        _ => None,
    }
}

fn claims(lock: &Lock) -> BTreeMap<&Name, Vec<&Entry>> {
    let mut claims: BTreeMap<&Name, Vec<&Entry>> = BTreeMap::new();
    for e in lock.entries.values() {
        for n in e.names() {
            claims.entry(n).or_default().push(e);
        }
    }
    claims
}

fn conflicts(lock: &Lock) -> Vec<Finding> {
    let claims = claims(lock);
    let mut findings: Vec<Finding> = claims
        .iter()
        .filter(|(name, owners)| owners.len() > 1 || lock.tombstones.contains(**name))
        .map(|(name, _)| Finding::Conflict { name: (*name).clone() })
        .collect();
    findings.extend(cycles(lock, &claims).into_iter().map(|ids| Finding::Cycle { ids }));
    findings
}

fn cycles(lock: &Lock, claims: &BTreeMap<&Name, Vec<&Entry>>) -> Vec<Vec<Name>> {
    let ids: Vec<&Name> = lock.entries.keys().collect();
    let index: BTreeMap<&Name, usize> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let mut edges: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); ids.len()];
    for (from, e) in lock.entries.values().enumerate() {
        for n in e.names().filter(|n| **n != e.current) {
            for target in claims.get(n).into_iter().flatten().filter(|t| t.current == *n && t.id != e.id) {
                if let (Some(set), Some(&to)) = (edges.get_mut(from), index.get(&target.id)) {
                    set.insert(to);
                }
            }
        }
    }
    let mut state = vec![0u8; ids.len()];
    let mut found: BTreeSet<Vec<Name>> = BTreeSet::new();
    for start in 0..ids.len() {
        if state.get(start) != Some(&0) {
            continue;
        }
        let mut path: Vec<usize> = vec![start];
        let mut stack: Vec<std::collections::btree_set::Iter<'_, usize>> = edges.get(start).map(|s| s.iter()).into_iter().collect();
        if let Some(s) = state.get_mut(start) {
            *s = 1;
        }
        while let Some(top) = stack.last_mut() {
            match top.next().copied() {
                Some(next) => match state.get(next).copied() {
                    Some(0) => {
                        if let Some(s) = state.get_mut(next) {
                            *s = 1;
                        }
                        path.push(next);
                        stack.extend(edges.get(next).map(|s| s.iter()));
                    }
                    Some(1) => {
                        if let Some(at) = path.iter().position(|&p| p == next) {
                            let mut cycle: Vec<Name> = path.iter().skip(at).filter_map(|&i| ids.get(i).map(|n| (*n).clone())).collect();
                            cycle.sort();
                            found.insert(cycle);
                        }
                    }
                    _ => {}
                },
                None => {
                    stack.pop();
                    if let Some(done) = path.pop()
                        && let Some(s) = state.get_mut(done)
                    {
                        *s = 2;
                    }
                }
            }
        }
    }
    found.into_iter().collect()
}
