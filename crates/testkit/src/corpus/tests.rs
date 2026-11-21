use std::fs;

use super::*;

const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn files(list: &[(&str, &[u8])]) -> Vec<(String, Vec<u8>)> {
    list.iter().map(|(path, bytes)| ((*path).to_owned(), bytes.to_vec())).collect()
}

fn sample() -> Corpus {
    let mut corpus = Corpus::new("demo").unwrap();
    corpus.add("module", "abi-one", "loads and doubles \"red\"", files(&[("m.wat", b"abc"), ("sub/x.bin", b"")])).unwrap();
    corpus.add("folder", "plugin-dir", "lists one module", files(&[("Plug-ins/One Module.wasm", b"abc")])).unwrap();
    corpus
}

const SAMPLE_TOML: &str = r#"[[member]]
name = "folder"
kind = "plugin-dir"
app = "demo"
expect = "lists one module"
files = [
    { path = "Plug-ins/One Module.wasm", sha256 = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" },
]

[[member]]
name = "module"
kind = "abi-one"
app = "demo"
expect = "loads and doubles \"red\""
files = [
    { path = "m.wat", sha256 = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" },
    { path = "sub/x.bin", sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" },
]
"#;

#[test]
fn the_listing_is_sorted_and_carries_every_digest() {
    assert_eq!(sample().manifest().render(), SAMPLE_TOML);
    assert_eq!(Manifest::parse(SAMPLE_TOML).unwrap(), sample().manifest());
}

#[test]
fn a_written_corpus_checks_clean_and_reopens_with_its_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = sample();
    corpus.write(dir.path()).unwrap();
    assert_eq!(corpus.check(dir.path()).unwrap(), vec![]);
    assert_eq!(fs::read_to_string(dir.path().join(".gitattributes")).unwrap(), "* -text\n");
    let committed = open(dir.path()).unwrap();
    assert_eq!(committed.bytes("module", "m.wat").unwrap(), b"abc");
    assert_eq!(committed.path("folder", "Plug-ins/One Module.wasm").unwrap(), dir.path().join("folder/Plug-ins/One Module.wasm"));
    assert_eq!(committed.dir("folder").unwrap(), dir.path().join("folder"));
    assert_eq!(committed.bytes("module", "nope"), Err(Error::NoFile { member: "module".into(), path: "nope".into() }));
    assert_eq!(committed.dir("nope"), Err(Error::NoMember("nope".into())));
}

#[test]
fn check_reports_missing_changed_and_stray_files() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = sample();
    corpus.write(dir.path()).unwrap();
    fs::remove_file(dir.path().join("module/sub/x.bin")).unwrap();
    fs::write(dir.path().join("module/m.wat"), b"abd").unwrap();
    fs::write(dir.path().join("module/extra.txt"), b"").unwrap();
    fs::create_dir_all(dir.path().join("gone/deep")).unwrap();
    fs::write(dir.path().join("gone/deep/old.wasm"), b"x").unwrap();
    assert_eq!(
        corpus.check(dir.path()).unwrap(),
        vec![
            Drift::Changed {
                path: "module/m.wat".into(),
                committed: "a52d159f262b2c6ddb724a61840befc36eb30c88877a4030b65cbe86298449c9".into(),
                generated: ABC.into()
            },
            Drift::Missing("module/sub/x.bin".into()),
            Drift::Stray("gone/deep/old.wasm".into()),
            Drift::Stray("module/extra.txt".into()),
        ]
    );
}

#[test]
fn check_fails_on_an_empty_folder_and_on_an_edited_listing() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = sample();
    assert_eq!(
        corpus.check(&dir.path().join("absent")).unwrap(),
        vec![
            Drift::Missing("corpus.toml".into()),
            Drift::Missing(".gitattributes".into()),
            Drift::Missing("folder/Plug-ins/One Module.wasm".into()),
            Drift::Missing("module/m.wat".into()),
            Drift::Missing("module/sub/x.bin".into()),
        ]
    );
    corpus.write(dir.path()).unwrap();
    fs::write(dir.path().join("corpus.toml"), SAMPLE_TOML.replace("lists one module", "lists two modules")).unwrap();
    assert_eq!(corpus.check(dir.path()).unwrap(), vec![Drift::Manifest]);
}

#[test]
fn write_restores_files_keeps_strays_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = sample();
    corpus.write(dir.path()).unwrap();
    fs::write(dir.path().join("module/m.wat"), b"changed").unwrap();
    fs::write(dir.path().join("stray.txt"), b"x").unwrap();
    corpus.write(dir.path()).unwrap();
    corpus.write(dir.path()).unwrap();
    assert_eq!(fs::read(dir.path().join("module/m.wat")).unwrap(), b"abc");
    assert_eq!(fs::read(dir.path().join("stray.txt")).unwrap(), b"x");
    assert_eq!(corpus.check(dir.path()).unwrap(), vec![Drift::Stray("stray.txt".into())]);
}

#[test]
fn write_refuses_a_folder_that_is_not_a_corpus() {
    let dir = tempfile::tempdir().unwrap();
    sample().write(&dir.path().join("absent")).unwrap();
    fs::create_dir(dir.path().join("empty")).unwrap();
    sample().write(&dir.path().join("empty")).unwrap();
    let full = dir.path().join("full");
    fs::create_dir_all(full.join("notes")).unwrap();
    fs::write(full.join("notes/keep.md"), b"mine").unwrap();
    assert_eq!(sample().write(&full), Err(Error::NotACorpus(full.clone())));
    assert_eq!(fs::read(full.join("notes/keep.md")).unwrap(), b"mine");
    assert!(!full.join("corpus.toml").exists());
}

#[test]
fn committed_members_are_append_only() {
    let dir = tempfile::tempdir().unwrap();
    sample().write(dir.path()).unwrap();
    let mut rewritten = Corpus::new("demo").unwrap();
    rewritten.add("module", "abi-one", "loads and doubles \"red\"", files(&[("m.wat", b"abd"), ("sub/x.bin", b"")])).unwrap();
    rewritten.add("folder", "plugin-dir", "lists one module", files(&[("Plug-ins/One Module.wasm", b"abc")])).unwrap();
    assert_eq!(rewritten.write(dir.path()), Err(Error::AppendOnly(Drift::Rewritten("module".into()))));
    assert_eq!(fs::read(dir.path().join("module/m.wat")).unwrap(), b"abc");
    assert!(rewritten.check(dir.path()).unwrap().contains(&Drift::Rewritten("module".into())));
    let mut dropped = Corpus::new("demo").unwrap();
    dropped.add("module", "abi-one", "loads and doubles \"red\"", files(&[("m.wat", b"abc"), ("sub/x.bin", b"")])).unwrap();
    assert_eq!(dropped.write(dir.path()), Err(Error::AppendOnly(Drift::Removed("folder".into()))));
    let mut grown = sample();
    grown.add("extra", "abi-one", "a new member", files(&[("e.wat", b"e")])).unwrap();
    assert_eq!(grown.check(dir.path()).unwrap(), vec![Drift::Manifest, Drift::Missing("extra/e.wat".into())]);
    grown.write(dir.path()).unwrap();
    assert_eq!(grown.check(dir.path()).unwrap(), vec![]);
    assert_eq!(
        Error::AppendOnly(Drift::Rewritten("module".into())).to_string(),
        "rewritten: member module has other files or digests than corpus.toml lists: committed members are append-only, so keep the member as it is and add a new member for the new content"
    );
}

#[test]
fn copy_member_writes_its_verified_files() {
    let dir = tempfile::tempdir().unwrap();
    sample().write(&dir.path().join("corpus")).unwrap();
    let to = dir.path().join("copy");
    open(&dir.path().join("corpus")).unwrap().copy_member("module", &to).unwrap();
    assert_eq!((fs::read(to.join("m.wat")).unwrap(), fs::read(to.join("sub/x.bin")).unwrap()), (b"abc".to_vec(), Vec::new()));
}

#[test]
fn open_refuses_a_file_that_differs_from_its_listed_digest() {
    let dir = tempfile::tempdir().unwrap();
    sample().write(dir.path()).unwrap();
    fs::write(dir.path().join("module/sub/x.bin"), b"abc").unwrap();
    assert_eq!(open(dir.path()).unwrap_err(), Error::Digest { path: dir.path().join("module/sub/x.bin"), listed: EMPTY.into(), actual: ABC.into() });
    fs::remove_file(dir.path().join("module/sub/x.bin")).unwrap();
    assert_eq!(open(dir.path()).unwrap_err(), Error::Io { path: dir.path().join("module/sub/x.bin"), message: "not found".into() });
}

#[test]
fn coverage_names_kinds_without_members_and_members_of_unknown_kinds() {
    let manifest = sample().manifest();
    assert_eq!(manifest.coverage(&["abi-one", "plugin-dir"]).unwrap(), vec![]);
    assert_eq!(
        manifest.coverage(&["abi-one", "abi-two"]).unwrap(),
        vec![Gap::Uncovered("abi-two".into()), Gap::Unknown { member: "folder".into(), kind: "plugin-dir".into() }]
    );
    assert_eq!(manifest.coverage(&[]), Err(Error::NoKinds));
    assert_eq!(Manifest { member: vec![] }.coverage(&["abi-one"]).unwrap(), vec![Gap::Uncovered("abi-one".into())]);
}

#[test]
fn members_with_bad_names_paths_or_files_are_refused() {
    let mut corpus = sample();
    let one = || files(&[("a", b"")]);
    assert_eq!(Corpus::new("Demo"), Err(Error::Name("Demo".into())));
    assert_eq!(corpus.add("module", "k", "e", one()), Err(Error::Duplicate("module".into())));
    assert_eq!(corpus.add("a_b", "k", "e", one()), Err(Error::Name("a_b".into())));
    assert_eq!(corpus.add("a", "K", "e", one()), Err(Error::Kind("K".into())));
    assert_eq!(corpus.add("a", "k", "two\nlines", one()), Err(Error::Expect("a".into())));
    assert_eq!(corpus.add("a", "k", "", one()), Err(Error::Expect("a".into())));
    assert_eq!(corpus.add("a", "k", &"x".repeat(240), one()), Ok(()));
    assert_eq!(corpus.add("b", "k", &"x".repeat(241), one()), Err(Error::Expect("b".into())));
    assert_eq!(corpus.add("b", "k", "e", vec![]), Err(Error::NoFiles("b".into())));
    for bad in ["../x", ".hidden", "a//b", "a\\b", "/abs", "", "a/./b", " lead", "trail ", "q\"uote"] {
        assert_eq!(corpus.add("b", "k", "e", files(&[(bad, b"")])), Err(Error::Path { member: "b".into(), path: bad.into() }), "{bad}");
    }
    assert_eq!(corpus.add("b", "k", "e", files(&[("x", b""), ("x", b"1")])), Err(Error::DuplicateFile { member: "b".into(), path: "x".into() }));
    let many: Vec<(String, Vec<u8>)> = (0..=MAX_FILES).map(|n| (format!("f{n}"), Vec::new())).collect();
    assert_eq!(corpus.add("b", "k", "e", many), Err(Error::TooManyFiles("b".into())));
}

#[test]
fn a_listing_with_unknown_fields_or_bad_digests_does_not_parse() {
    let extra = SAMPLE_TOML.replacen("app = \"demo\"", "app = \"demo\"\nextra = 1", 1);
    assert!(matches!(Manifest::parse(&extra), Err(Error::Manifest(_))));
    let upper = SAMPLE_TOML.replacen(ABC, &ABC.to_uppercase(), 1);
    assert!(matches!(Manifest::parse(&upper), Err(Error::Manifest(_))));
    let twice = format!("{SAMPLE_TOML}\n{}", SAMPLE_TOML.split("\n\n").next().unwrap());
    assert_eq!(Manifest::parse(&twice), Err(Error::Duplicate("folder".into())));
    let escape = SAMPLE_TOML.replacen("plugin-dir", "../plugin", 1);
    assert_eq!(Manifest::parse(&escape), Err(Error::Kind("../plugin".into())));
    assert_eq!(Manifest::parse("").unwrap(), Manifest { member: vec![] });
}

fn run(args: &[&str], root: &std::path::Path, corpus: Corpus) -> (Result<bool, String>, String, String) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let r = cli::cli(args.iter().map(|a| (*a).to_owned()), root, || Ok::<_, String>(corpus), &mut out, &mut err);
    (r, String::from_utf8(out).unwrap(), String::from_utf8(err).unwrap())
}

#[test]
fn the_cli_writes_checks_and_names_the_append_only_rule() {
    let dir = tempfile::tempdir().unwrap();
    let (r, out, _) = run(&[], dir.path(), sample());
    assert_eq!((r, out), (Ok(true), format!("wrote 2 members to {}\n", dir.path().display())));
    let (r, out, err) = run(&["--check"], dir.path(), sample());
    assert_eq!((r, err), (Ok(true), String::new()));
    assert!(out.starts_with("folder                     plugin-dir               1 file(s)  lists one module\n"), "{out}");
    assert!(out.ends_with(&format!("{} matches the generator\n", dir.path().display())), "{out}");
    let mut rewritten = Corpus::new("demo").unwrap();
    rewritten.add("folder", "plugin-dir", "lists one module", files(&[("Plug-ins/One Module.wasm", b"abd")])).unwrap();
    rewritten.add("module", "abi-one", "loads and doubles \"red\"", files(&[("m.wat", b"abc"), ("sub/x.bin", b"")])).unwrap();
    let (r, _, err) = run(&["--check"], dir.path(), rewritten);
    assert_eq!(r, Ok(false));
    assert!(err.ends_with(&format!("3 drift(s): {APPEND_ONLY}\n")), "{err}");
    let mut grown = sample();
    grown.add("extra", "abi-one", "a new member", files(&[("e.wat", b"e")])).unwrap();
    let (r, _, err) = run(&["--check", "--root", dir.path().to_str().unwrap()], &dir.path().join("elsewhere"), grown);
    assert_eq!(r, Ok(false));
    assert!(err.ends_with("2 drift(s): regenerate to add the new members, then review the diff\n"), "{err}");
    assert_eq!(run(&["--write"], dir.path(), sample()).0, Err(cli::USAGE.to_owned()));
    assert_eq!(run(&["--root"], dir.path(), sample()).0, Err(cli::USAGE.to_owned()));
}

crate::legacy_paths! {
    enum Sample {
        One => "path-one",
        Two => "path-two",
    }
}

#[test]
fn legacy_paths_lists_every_variant_with_its_kind() {
    assert_eq!(Sample::ALL, [Sample::One, Sample::Two]);
    assert_eq!(Sample::ALL.map(Sample::name), ["path-one", "path-two"]);
}

#[test]
fn limits_hold_at_their_exact_boundaries() {
    let one = || files(&[("a", b"")]);
    let mut full = Corpus::new("demo").unwrap();
    for n in 0..MAX_MEMBERS {
        full.add(&format!("m{n}"), "k", "e", one()).unwrap();
    }
    assert_eq!(full.add("over", "k", "e", one()), Err(Error::TooManyMembers));
    let mut corpus = Corpus::new("demo").unwrap();
    let at_limit: Vec<(String, Vec<u8>)> = (0..MAX_FILES).map(|n| (format!("f{n}"), Vec::new())).collect();
    assert_eq!(corpus.add("files", "k", "e", at_limit), Ok(()));
    assert_eq!(corpus.add("big", "k", "e", vec![("a".into(), vec![0; MAX_FILE_BYTES as usize])]), Ok(()));
    assert_eq!(corpus.add("bigger", "k", "e", vec![("a".into(), vec![0; MAX_FILE_BYTES as usize + 1])]), Err(Error::TooLarge("bigger/a".into())));
    let eight = ["p"; 8].join("/");
    let nine = ["p"; 9].join("/");
    let long = "x".repeat(128);
    let longer = "x".repeat(129);
    let total = format!("{}/{}/{}/{}", "y".repeat(128), "y".repeat(128), "y".repeat(128), "y".repeat(125));
    let over_total = format!("{total}y");
    assert_eq!(total.len(), 512);
    for (n, (path, ok)) in
        [(&eight, true), (&nine, false), (&long, true), (&longer, false), (&total, true), (&over_total, false)].into_iter().enumerate()
    {
        let r = corpus.add(&format!("path{n}"), "k", "e", files(&[(path.as_str(), b"")]));
        assert_eq!(r.is_ok(), ok, "{} parts, {} bytes", path.split('/').count(), path.len());
    }
}
