use craft_idmap::{Finding, Lock, Migration, Name, Resolved, Resolver};

fn n(s: &str) -> Name {
    Name::new(s).unwrap()
}

const TEXT: &str = "format = 1\ntombstones = [\"edit.gone\"]\n\
    [[entry]]\nid = \"filter.gaussianBlur\"\ncurrent = \"filter.blur.gaussian\"\nclass = \"command\"\nargs = \"00000000000000ff\"\n\
    aliases = [\"filter.blur\"]\nmigrations = [{ from = \"0000000000000001\", name = \"px-to-radius\" }, { from = \"0000000000000002\", name = \"double\" }]\n\
    [[entry]]\nid = \"view.zoom\"\nclass = \"shell\"\n";

fn px_to_radius(mut args: Vec<(String, i64)>) -> Result<Vec<(String, i64)>, String> {
    for (k, _) in &mut args {
        if k == "px" {
            *k = "radius".into();
        }
    }
    Ok(args)
}

fn double(mut args: Vec<(String, i64)>) -> Result<Vec<(String, i64)>, String> {
    for (_, v) in &mut args {
        *v = v.checked_mul(2).ok_or("radius out of range")?;
    }
    Ok(args)
}

const MIGRATIONS: &[Migration<Vec<(String, i64)>>] =
    &[Migration { name: "double", run: double }, Migration { name: "px-to-radius", run: px_to_radius }];

fn resolver() -> Resolver {
    Resolver::new(Lock::parse(TEXT).unwrap()).unwrap()
}

#[test]
fn every_name_an_entry_ever_had_resolves_to_its_current_id() {
    let r = resolver();
    for persisted in ["filter.gaussianBlur", "filter.blur", "filter.blur.gaussian"] {
        match r.resolve(persisted, vec![], MIGRATIONS) {
            Resolved::Current { id, .. } => assert_eq!(id.as_str(), "filter.blur.gaussian", "{persisted}"),
            other => panic!("{persisted}: {other:?}"),
        }
    }
    assert!(matches!(r.resolve("view.zoom", (), &[]), Resolved::Current { id, .. } if id.as_str() == "view.zoom"));
}

#[test]
fn migrations_run_in_the_order_the_lock_records_them() {
    let r = resolver();
    let got = r.resolve("filter.gaussianBlur", vec![("px".into(), 3)], MIGRATIONS);
    assert_eq!(got, Resolved::Current { id: &n("filter.blur.gaussian"), args: vec![("radius".into(), 6)] });
}

#[test]
fn unknown_and_tombstoned_ids_stay_distinct() {
    let r = resolver();
    assert_eq!(r.current("filter.blur"), Resolved::Current { id: &n("filter.blur.gaussian"), args: () });
    assert_eq!(r.current("edit.gone"), Resolved::Tombstoned);
    assert_eq!(r.current("edit.never"), Resolved::Unknown);
    assert_eq!(r.resolve("edit.gone", (), &[]), Resolved::Tombstoned);
    assert_eq!(r.resolve("edit.never", (), &[]), Resolved::Unknown);
    assert_eq!(r.resolve("", (), &[]), Resolved::Unknown);
}

#[test]
fn a_failing_or_missing_migration_is_reported_not_skipped() {
    let r = resolver();
    let got = r.resolve("filter.blur", vec![("px".into(), i64::MAX)], MIGRATIONS);
    assert_eq!(got, Resolved::MigrationFailed { id: &n("filter.blur.gaussian"), migration: &n("double"), reason: "radius out of range".into() });
    let got = r.resolve("filter.blur", vec![], &MIGRATIONS[..1]);
    assert_eq!(
        got,
        Resolved::MigrationFailed {
            id: &n("filter.blur.gaussian"),
            migration: &n("px-to-radius"),
            reason: "the app registers no such migration".into()
        }
    );
}

#[test]
fn saves_write_the_first_shipped_id() {
    let r = resolver();
    assert_eq!(r.persisted("filter.blur.gaussian"), Some(&n("filter.gaussianBlur")));
    assert_eq!(r.persisted("filter.blur"), None);
    assert_eq!(r.persisted("view.zoom"), Some(&n("view.zoom")));
}

#[test]
fn a_lock_with_a_conflict_cannot_build_a_resolver() {
    let lock = Lock::parse("format = 1\n[[entry]]\nid = \"a\"\nclass = \"c\"\naliases = [\"b\"]\n[[entry]]\nid = \"b\"\nclass = \"c\"\n").unwrap();
    assert_eq!(Resolver::new(lock).err(), Some(vec![Finding::Conflict { name: n("b") }]));
}
