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
fn write_removes_strays_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = sample();
    corpus.write(dir.path()).unwrap();
    fs::create_dir_all(dir.path().join("retired/sub")).unwrap();
    fs::write(dir.path().join("retired/sub/a.wasm"), b"x").unwrap();
    fs::write(dir.path().join("module/m.wat"), b"changed").unwrap();
    corpus.write(dir.path()).unwrap();
    corpus.write(dir.path()).unwrap();
    assert_eq!(corpus.check(dir.path()).unwrap(), vec![]);
    assert!(!dir.path().join("retired").exists());
    assert_eq!(fs::read(dir.path().join("module/m.wat")).unwrap(), b"abc");
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
