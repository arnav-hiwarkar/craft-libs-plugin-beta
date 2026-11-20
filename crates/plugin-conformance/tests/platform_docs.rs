use std::path::Path;

#[test]
fn the_platform_docs_have_no_findings() {
    let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/plugin-platform");
    let findings = craft_plugin_conformance::check(&docs).expect("the platform docs are readable");
    assert_eq!(findings, Vec::new());
}
