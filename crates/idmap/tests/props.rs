use std::collections::{BTreeMap, BTreeSet};

use craft_idmap::{Class, EditError, Lock, Name, Registered, Resolved, Resolver, check, update};
use proptest::prelude::*;

fn n(s: &str) -> Name {
    Name::new(s).unwrap()
}

#[derive(Clone, Debug)]
enum Op {
    Add(u8),
    Rename(u8, u8),
    Retire(u8),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![(0u8..24).prop_map(Op::Add), (0u8..24, 0u8..24).prop_map(|(a, b)| Op::Rename(a, b)), (0u8..24).prop_map(Op::Retire)]
}

fn registry(currents: &BTreeSet<String>) -> Vec<Registered> {
    currents.iter().map(|c| Registered { id: n(c), class: Class::new("command").unwrap(), args: None }).collect()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    #[test]
    fn idmap_props_renames_and_reuse_attempts(ops in prop::collection::vec(op(), 1..48)) {
        let mut lock = Lock::default();
        let mut model: BTreeMap<String, (String, BTreeSet<String>)> = BTreeMap::new();
        let mut retired: BTreeSet<String> = BTreeSet::new();
        let claimed = |model: &BTreeMap<String, (String, BTreeSet<String>)>, retired: &BTreeSet<String>, name: &str| {
            retired.contains(name) || model.iter().any(|(first, (current, old))| first == name || current == name || old.contains(name))
        };
        for op in ops {
            match op {
                Op::Add(k) => {
                    let name = format!("c{k}");
                    if claimed(&model, &retired, &name) {
                        continue;
                    }
                    let mut currents: BTreeSet<String> = model.values().map(|(c, _)| c.clone()).collect();
                    currents.insert(name.clone());
                    lock = update(&lock, &registry(&currents), &[]).unwrap();
                    model.insert(name.clone(), (name, BTreeSet::new()));
                }
                Op::Rename(k, to) => {
                    let Some(first) = model.keys().nth(usize::from(k) % model.len().max(1)).cloned() else { continue };
                    let to = format!("c{to}");
                    let (current, _) = model[&first].clone();
                    let theirs = retired.contains(&to) || model.iter().any(|(f, (c, old))| f != &first && (f == &to || c == &to || old.contains(&to)));
                    let result = lock.rename(&n(&current), n(&to));
                    if theirs {
                        prop_assert_eq!(result, Err(EditError::Claimed(n(&to))));
                        continue;
                    }
                    prop_assert_eq!(result, Ok(()));
                    let entry = model.get_mut(&first).unwrap();
                    if to != current {
                        entry.1.insert(current);
                        entry.1.remove(&to);
                        entry.0 = to;
                    }
                }
                Op::Retire(k) => {
                    let Some(first) = model.keys().nth(usize::from(k) % model.len().max(1)).cloned() else { continue };
                    let (current, old) = model.remove(&first).unwrap();
                    prop_assert_eq!(lock.retire(&n(&current)), Ok(()));
                    retired.insert(first);
                    retired.insert(current);
                    retired.extend(old);
                }
            }
        }
        let currents: BTreeSet<String> = model.values().map(|(c, _)| c.clone()).collect();
        if !currents.is_empty() {
            prop_assert_eq!(check(&lock, &registry(&currents), &[]), vec![]);
        }
        prop_assert_eq!(Lock::parse(&lock.to_text()), Ok(lock.clone()));
        let resolver = Resolver::new(lock.clone()).unwrap();
        for (first, (current, old)) in &model {
            prop_assert_eq!(resolver.persisted(current).map(Name::as_str), Some(first.as_str()));
            for name in std::iter::once(first).chain(old) {
                match resolver.resolve(name, (), &[]) {
                    Resolved::Current { id, .. } => prop_assert_eq!(id.as_str(), current.as_str()),
                    other => prop_assert!(false, "{} resolved to {:?}", name, other),
                }
            }
        }
        for e in lock.entries() {
            prop_assert!(e.aliases().iter().all(|a| a != e.id() && a != e.current()));
        }
        for name in &retired {
            prop_assert_eq!(resolver.resolve(name, (), &[]), Resolved::Tombstoned);
        }
    }
}
