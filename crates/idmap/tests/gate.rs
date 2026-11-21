use craft_idmap::{ArgsHash, Class, Edit, Name, Registered, Request, regenerate, verify};

fn reg(id: &str, keys: &[&str]) -> Registered {
    Registered { id: Name::new(id).unwrap(), class: Class::new("command").unwrap(), args: Some(ArgsHash::of_keys(keys.iter().copied())) }
}

#[test]
fn update_seeds_a_lock_that_then_verifies() {
    let registry = [reg("filter.blur", &["px"])];
    let text = regenerate("format = 1\n", &registry, &[], &[]).unwrap();
    assert_eq!(text, format!("format = 1\n\n[[entry]]\nid = \"filter.blur\"\nclass = \"command\"\nargs = \"{}\"\n", ArgsHash::of_keys(["px"])));
    assert_eq!(verify(&text, &registry, &[]), Ok(()));
    assert_eq!(
        verify(&text.replace("\n\n", "\n"), &registry, &[]),
        Err(vec!["the lock is not in the form `engine-lock --update` writes; run it".to_owned()])
    );
}

#[test]
fn a_rename_and_argument_rename_go_through_recorded_edits() {
    let before = regenerate("format = 1\n", &[reg("filter.blur", &["px"])], &[], &[]).unwrap();
    let after = [reg("filter.blur.gaussian", &["radius"])];
    let refused = verify(&before, &after, &["px-to-radius"]).unwrap_err();
    assert_eq!(refused.len(), 2);
    assert!(refused[1].starts_with("filter.blur is in the lock but no longer registered"), "{refused:?}");
    let edits: Vec<Edit> =
        ["rename filter.blur=filter.blur.gaussian", "migrate filter.blur.gaussian=px-to-radius"].iter().map(|e| e.parse().unwrap()).collect();
    let text = regenerate(&before, &after, &["px-to-radius"], &edits).unwrap();
    assert_eq!(verify(&text, &after, &["px-to-radius"]), Ok(()));
    assert!(text.contains("id = \"filter.blur\"\ncurrent = \"filter.blur.gaussian\"\n"), "{text}");
    assert_eq!(
        regenerate(&before, &after, &[], &edits),
        Err(vec!["filter.blur.gaussian names migration px-to-radius, which the app does not register".to_owned()])
    );
}

#[test]
fn malformed_edits_are_refused() {
    assert_eq!("retire a".parse::<Edit>(), Ok(Edit::Retire(Name::new("a").unwrap())));
    assert_eq!("rename a".parse::<Edit>(), Err("`rename a` needs the form ID=VALUE".to_owned()));
    assert_eq!("drop a".parse::<Edit>(), Err("`drop a` is not `rename OLD=NEW`, `retire ID` or `migrate ID=NAME`".to_owned()));
}

#[test]
fn engine_lock_arguments_become_a_request() {
    let edit = |s: &str| s.parse::<Edit>().unwrap();
    assert_eq!(Request::from_args(&[]), Ok(Request::Check));
    assert_eq!(Request::from_args(&["--check"]), Ok(Request::Check));
    assert_eq!(
        Request::from_args(&["--update", "--rename", "a=b", "--migrate", "b=m", "--retire", "c"]),
        Ok(Request::Update(vec![edit("rename a=b"), edit("migrate b=m"), edit("retire c")]))
    );
    assert_eq!(Request::from_args(&["--retire", "c"]), Err("--rename, --retire and --migrate edit the lock: add --update".into()));
    assert_eq!(Request::from_args(&["--update", "--rename"]), Err("--rename needs a value".into()));
    assert_eq!(Request::from_args(&["--update", "--rename", "a"]), Err("`rename a` needs the form ID=VALUE".into()));
    assert!(Request::from_args(&["--force"]).unwrap_err().starts_with("unknown argument `--force`"));
}

#[test]
fn a_request_checks_or_rewrites_the_lock() {
    let registry = [reg("a", &[])];
    let seeded = Request::Update(vec![]).run("format = 1\n", &registry, &[]).unwrap().unwrap();
    assert_eq!(Request::Check.run(&seeded, &registry, &[]), Ok(None));
    assert_eq!(Request::Check.run("format = 1\n", &registry, &[]).unwrap_err().len(), 2);
}
