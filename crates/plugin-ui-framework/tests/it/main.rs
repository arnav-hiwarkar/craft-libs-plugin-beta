use craft_plugin_ui::conformance::{self, Painter};

fn framework() -> Painter {
    let mut installed = false;
    Box::new(move |ui, tree| {
        if !installed {
            installed = true;
            ui.ctx().set_fonts(craft_ui::fonts::deflated_default_definitions());
            let spec = craft_ui::testing::install_spec();
            assert!(craft_ui::install(ui.ctx(), spec).is_ok(), "the runtime installs once per context");
        }
        craft_plugin_ui_framework::draw(ui, tree, "conformance", &mut ())
    })
}

#[test]
fn every_conformance_case_passes_on_the_framework_painter() {
    let failures = conformance::run_all(framework);
    assert!(failures.is_empty(), "{failures:#?}");
}
