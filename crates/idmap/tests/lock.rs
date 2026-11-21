use craft_idmap::{ArgsHash, Class, EditError, Lock, MAX_ALIASES, MAX_LOCK_BYTES, MAX_NAME_BYTES, Name, NameError, ParseError};

fn n(s: &str) -> Name {
    Name::new(s).unwrap()
}

const TEXT: &str = r#"format = 1
tombstones = [
  "edit.gone",
]

[[entry]]
id = "filter.blur"
current = "filter.blur.gaussian"
class = "command"
args = "00112233445566ff"
aliases = ["filter.gaussian"]
migrations = [{ from = "0000000000000001", name = "px-to-radius" }]

[[entry]]
id = "view.zoom"
class = "shell"
"#;

#[test]
fn a_lock_reads_and_writes_back_byte_for_byte() {
    let lock = Lock::parse(TEXT).unwrap();
    assert_eq!(lock.to_text(), TEXT);
    let first = lock.entries().next().unwrap();
    assert_eq!(first.id().as_str(), "filter.blur");
    assert_eq!(first.current().as_str(), "filter.blur.gaussian");
    assert_eq!(first.class().as_str(), "command");
    assert_eq!(first.args().map(|a| a.to_string()).as_deref(), Some("00112233445566ff"));
    assert_eq!(first.aliases(), &[n("filter.gaussian")]);
    assert_eq!(first.migrations()[0].name.as_str(), "px-to-radius");
    assert_eq!(lock.tombstones().map(Name::as_str).collect::<Vec<_>>(), vec!["edit.gone"]);
}

#[test]
fn entries_are_written_sorted_by_first_shipped_id() {
    let shuffled = "format = 1\n[[entry]]\nid = \"b\"\nclass = \"command\"\n[[entry]]\nid = \"a\"\nclass = \"command\"\n";
    assert_eq!(
        Lock::parse(shuffled).unwrap().to_text(),
        "format = 1\n\n[[entry]]\nid = \"a\"\nclass = \"command\"\n\n[[entry]]\nid = \"b\"\nclass = \"command\"\n"
    );
}

#[test]
fn hostile_lock_text_is_refused_without_a_panic() {
    assert!(matches!(Lock::parse("format = "), Err(ParseError::Syntax(_))));
    assert_eq!(Lock::parse("format = 2"), Err(ParseError::Format(2)));
    assert!(matches!(Lock::parse("format = 1\nsurprise = 1"), Err(ParseError::Syntax(_))));
    assert_eq!(Lock::parse(&" ".repeat(MAX_LOCK_BYTES + 1)), Err(ParseError::TooLarge(MAX_LOCK_BYTES + 1)));
    assert!(matches!(
        Lock::parse("format = 1\n[[entry]]\nid = \"a b\"\nclass = \"command\""),
        Err(ParseError::Name { error: NameError::Character(' '), .. })
    ));
    assert!(matches!(
        Lock::parse("format = 1\n[[entry]]\nid = \"a\"\nclass = \"Command\""),
        Err(ParseError::Name { error: NameError::Class(_), .. })
    ));
    assert!(matches!(
        Lock::parse("format = 1\n[[entry]]\nid = \"a\"\nclass = \"c\"\nargs = \"xyz\""),
        Err(ParseError::Name { error: NameError::Hash(_), .. })
    ));
    assert_eq!(
        Lock::parse("format = 1\n[[entry]]\nid = \"a\"\nclass = \"c\"\n[[entry]]\nid = \"a\"\nclass = \"c\""),
        Err(ParseError::DuplicateEntry(n("a")))
    );
    assert_eq!(
        Lock::parse("format = 1\n[[entry]]\nid = \"a\"\ncurrent = \"b\"\nclass = \"c\"\naliases = [\"b\"]"),
        Err(ParseError::DuplicateName { id: n("a"), name: n("b") })
    );
}

fn with_aliases(count: usize) -> String {
    let aliases: Vec<String> = (0..count).map(|i| format!("\"old{i}\"")).collect();
    format!("format = 1\n[[entry]]\nid = \"a\"\nclass = \"c\"\naliases = [{}]\n", aliases.join(", "))
}

#[test]
fn an_entry_holds_at_most_64_aliases() {
    assert_eq!(MAX_ALIASES, 64);
    assert_eq!(Lock::parse(&with_aliases(64)).unwrap().entries().next().unwrap().aliases().len(), 64);
    assert_eq!(Lock::parse(&with_aliases(65)), Err(ParseError::TooManyAliases(n("a"))));
}

#[test]
fn a_name_is_at_most_256_bytes() {
    assert_eq!(MAX_NAME_BYTES, 256);
    assert_eq!(Name::new(&"a".repeat(256)).unwrap().as_str().len(), 256);
    assert_eq!(Name::new(&"a".repeat(257)), Err(NameError::TooLong(257)));
    assert_eq!(Name::new(""), Err(NameError::Empty));
}

#[test]
fn the_args_hash_ignores_key_order_and_repeats() {
    let a = ArgsHash::of_keys(["radius", "layer"]);
    assert_eq!(a, ArgsHash::of_keys(["layer", "radius", "radius"]));
    assert_ne!(a, ArgsHash::of_keys(["px", "layer"]));
    assert_eq!(ArgsHash::of_keys([]).to_string(), "e3b0c44298fc1c14");
    assert_eq!(ArgsHash::parse(&a.to_string()), Ok(a));
}

fn seeded() -> Lock {
    Lock::parse("format = 1\n[[entry]]\nid = \"a\"\nclass = \"command\"\nargs = \"0000000000000001\"\n[[entry]]\nid = \"b\"\nclass = \"command\"\n")
        .unwrap()
}

#[test]
fn a_rename_keeps_the_first_id_and_flattens_the_chain() {
    let mut lock = seeded();
    lock.rename(&n("a"), n("a2")).unwrap();
    lock.rename(&n("a2"), n("a3")).unwrap();
    let e = lock.entries().next().unwrap();
    assert_eq!((e.id().as_str(), e.current().as_str()), ("a", "a3"));
    assert_eq!(e.aliases(), &[n("a2")]);
    lock.rename(&n("a3"), n("a")).unwrap();
    let e = lock.entries().next().unwrap();
    assert_eq!((e.current().as_str(), e.aliases()), ("a", &[n("a2"), n("a3")][..]));
}

#[test]
fn a_rename_never_takes_another_entrys_name_or_a_tombstone() {
    let mut lock = seeded();
    assert_eq!(lock.rename(&n("a"), n("b")), Err(EditError::Claimed(n("b"))));
    lock.retire(&n("b")).unwrap();
    assert_eq!(lock.rename(&n("a"), n("b")), Err(EditError::Claimed(n("b"))));
    assert_eq!(lock.rename(&n("zzz"), n("y")), Err(EditError::NotCurrent(n("zzz"))));
}

#[test]
fn renames_stop_at_the_alias_limit_and_leave_the_entry_unchanged() {
    let mut lock = seeded();
    for i in 0..=MAX_ALIASES {
        let from = if i == 0 { n("a") } else { n(&format!("a{i}")) };
        lock.rename(&from, n(&format!("a{}", i + 1))).unwrap();
    }
    let e = lock.entries().next().unwrap();
    assert_eq!((e.aliases().len(), e.current().as_str()), (64, "a65"));
    assert_eq!(lock.rename(&n("a65"), n("a66")), Err(EditError::TooManyAliases(n("a"))));
    assert_eq!(lock.entries().next().unwrap().current().as_str(), "a65");
}

#[test]
fn retiring_tombstones_every_name_of_the_entry() {
    let mut lock = seeded();
    lock.rename(&n("a"), n("a2")).unwrap();
    lock.rename(&n("a2"), n("a3")).unwrap();
    lock.retire(&n("a3")).unwrap();
    assert_eq!(lock.tombstones().map(Name::as_str).collect::<Vec<_>>(), vec!["a", "a2", "a3"]);
    assert_eq!(lock.entries().map(|e| e.id().as_str()).collect::<Vec<_>>(), vec!["b"]);
}

#[test]
fn a_migration_records_the_schema_it_leaves() {
    let mut lock = seeded();
    lock.migrate(&n("a"), n("px-to-radius")).unwrap();
    lock.migrate(&n("a"), n("px-to-radius")).unwrap();
    let e = lock.entries().next().unwrap();
    assert_eq!(e.migrations().len(), 1);
    assert_eq!(e.migrations()[0].from.to_string(), "0000000000000001");
    assert_eq!(lock.migrate(&n("b"), n("m")), Err(EditError::NoArgs(n("b"))));
    assert_eq!(Class::new("command").unwrap().as_str(), "command");
}
