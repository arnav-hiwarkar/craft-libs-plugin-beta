use craft_idmap::{ArgsHash, Class, Finding, Lock, Name, Registered, check, update};

fn n(s: &str) -> Name {
    Name::new(s).unwrap()
}

fn reg(id: &str, keys: Option<&[&str]>) -> Registered {
    Registered { id: n(id), class: Class::new("command").unwrap(), args: keys.map(|k| ArgsHash::of_keys(k.iter().copied())) }
}

fn seeded(registry: &[Registered]) -> Lock {
    update(&Lock::default(), registry, &[]).unwrap()
}

#[test]
fn an_empty_registry_or_lock_never_passes() {
    assert_eq!(check(&Lock::default(), &[], &[]), vec![Finding::EmptyRegistry, Finding::EmptyLock]);
    let registry = [reg("a", None)];
    assert_eq!(check(&Lock::default(), &registry, &[]), vec![Finding::EmptyLock, Finding::Unlisted { id: n("a") }]);
    let lock = seeded(&registry);
    assert_eq!(check(&lock, &[], &[]), vec![Finding::EmptyRegistry, Finding::Missing { id: n("a") }]);
    assert_eq!(update(&lock, &[], &[]), Err(vec![Finding::EmptyRegistry, Finding::Missing { id: n("a") }]));
}

#[test]
fn a_seeded_lock_matches_its_registry() {
    let registry = [reg("a", Some(&["x"])), reg("b", None)];
    let lock = seeded(&registry);
    assert_eq!(check(&lock, &registry, &[]), vec![]);
    let ids: Vec<(&str, &str)> = lock.entries().map(|e| (e.id().as_str(), e.current().as_str())).collect();
    assert_eq!(ids, vec![("a", "a"), ("b", "b")]);
}

#[test]
fn a_new_command_is_unlisted_until_update_adds_it() {
    let lock = seeded(&[reg("a", None)]);
    let registry = [reg("a", None), reg("c", None)];
    assert_eq!(check(&lock, &registry, &[]), vec![Finding::Unlisted { id: n("c") }]);
    let next = update(&lock, &registry, &[]).unwrap();
    assert_eq!(check(&next, &registry, &[]), vec![]);
}

#[test]
fn an_unaliased_removal_is_refused_and_a_recorded_rename_passes() {
    let lock = seeded(&[reg("a", None), reg("b", None)]);
    let renamed = [reg("a2", None), reg("b", None)];
    assert_eq!(check(&lock, &renamed, &[]), vec![Finding::Unlisted { id: n("a2") }, Finding::Missing { id: n("a") }]);
    assert_eq!(update(&lock, &renamed, &[]), Err(vec![Finding::Missing { id: n("a") }]));
    let mut fixed = lock.clone();
    fixed.rename(&n("a"), n("a2")).unwrap();
    assert_eq!(check(&fixed, &renamed, &[]), vec![]);
    let mut retired = lock;
    retired.retire(&n("a")).unwrap();
    assert_eq!(check(&retired, &[reg("b", None)], &[]), vec![]);
}

#[test]
fn a_retired_or_renamed_away_id_is_never_reused() {
    let mut lock = seeded(&[reg("a", None), reg("b", None)]);
    lock.retire(&n("b")).unwrap();
    lock.rename(&n("a"), n("a2")).unwrap();
    let registry = [reg("a2", None), reg("b", None), reg("a", None)];
    assert_eq!(check(&lock, &registry, &[]), vec![Finding::Reused { id: n("b") }, Finding::Reused { id: n("a") }]);
}

#[test]
fn an_argument_change_needs_a_registered_migration() {
    let lock = seeded(&[reg("a", Some(&["px"]))]);
    let changed = [reg("a", Some(&["radius"]))];
    let locked = ArgsHash::of_keys(["px"]);
    let current = ArgsHash::of_keys(["radius"]);
    assert_eq!(check(&lock, &changed, &[]), vec![Finding::ArgsChanged { id: n("a"), locked, current }]);
    let mut migrated = lock.clone();
    migrated.migrate(&n("a"), n("px-to-radius")).unwrap();
    assert_eq!(
        check(&migrated, &changed, &[]),
        vec![Finding::Stale { id: n("a") }, Finding::UnknownMigration { id: n("a"), migration: n("px-to-radius") }]
    );
    assert_eq!(check(&migrated, &changed, &["px-to-radius"]), vec![Finding::Stale { id: n("a") }]);
    let next = update(&migrated, &changed, &["px-to-radius"]).unwrap();
    assert_eq!(check(&next, &changed, &["px-to-radius"]), vec![]);
    assert_eq!(next.entries().next().unwrap().args(), Some(current));
}

#[test]
fn a_command_that_stops_or_starts_taking_arguments_only_refreshes() {
    let lock = seeded(&[reg("a", Some(&["px"]))]);
    assert_eq!(check(&lock, &[reg("a", None)], &[]), vec![Finding::Stale { id: n("a") }]);
}

#[test]
fn a_name_claimed_twice_and_an_alias_cycle_are_reported() {
    let text = "format = 1\ntombstones = [\"z\"]\n\
        [[entry]]\nid = \"a\"\ncurrent = \"b\"\nclass = \"command\"\n\
        [[entry]]\nid = \"b\"\ncurrent = \"a\"\nclass = \"command\"\n\
        [[entry]]\nid = \"c\"\nclass = \"command\"\naliases = [\"z\"]\n";
    let lock = Lock::parse(text).unwrap();
    let registry = [reg("a", None), reg("b", None), reg("c", None)];
    let findings = check(&lock, &registry, &[]);
    assert_eq!(
        findings,
        vec![
            Finding::Conflict { name: n("a") },
            Finding::Conflict { name: n("b") },
            Finding::Conflict { name: n("z") },
            Finding::Cycle { ids: vec![n("a"), n("b")] },
        ]
    );
    assert_eq!(findings[3].to_string(), "the aliases of a, b form a cycle");
}

#[test]
fn a_registry_that_lists_an_id_twice_is_refused() {
    let lock = seeded(&[reg("a", None)]);
    assert_eq!(check(&lock, &[reg("a", None), reg("a", None)], &[]), vec![Finding::Duplicate { id: n("a") }]);
}
