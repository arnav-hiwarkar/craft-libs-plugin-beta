use std::cell::RefCell;
use std::rc::Rc;

use craft_plugin_ui::conformance::{self, Painter};
use craft_ui::testing::{Fixture, Target};
use egui::{Event, Key, Modifiers, MouseWheelSource, MouseWheelUnit, TouchPhase, Vec2};

mod host;
mod painter;

use host::App;

fn framework() -> Painter {
    let mut installed = false;
    Box::new(move |ui, tree| {
        if !installed {
            installed = true;
            ui.ctx().set_fonts(craft_ui::fonts::deflated_default_definitions());
            assert!(craft_ui::install(ui.ctx(), craft_ui::testing::install_spec()).is_ok(), "the runtime installs once per context");
            craft_ui::testing::windows::set_os(ui.ctx(), craft_host::target::Os::Linux);
        }
        craft_plugin_ui_framework::draw(ui, tree, "conformance", &mut ())
    })
}

#[test]
fn every_conformance_case_passes_on_the_framework_painter() {
    let failures: Vec<String> = conformance::cases()
        .iter()
        .filter_map(|case| match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| conformance::run(case, framework()))) {
            Ok(events) => case.check(&events).err(),
            Err(_) => Some(format!("{}: a step found no node by its accessible name", case.name)),
        })
        .collect();
    assert!(failures.is_empty(), "{} of {} cases fail: {failures:#?}", failures.len(), conformance::cases().len());
}

struct Dock {
    app: Rc<RefCell<App>>,
    fixture: Fixture,
}

impl Dock {
    fn new() -> Dock {
        let app = Rc::new(RefCell::new(App::new()));
        let shown = Rc::clone(&app);
        let mut fixture = Fixture::new(move |ui| shown.borrow_mut().show(ui));
        fixture.room(Vec2::new(900.0, 600.0));
        let mut dock = Dock { app, fixture };
        dock.settle();
        dock
    }

    fn settle(&mut self) {
        for _ in 0..4 {
            self.fixture.settle();
            self.app.borrow_mut().pump();
        }
        self.fixture.settle();
    }

    fn find(&self, label: &str) -> Option<Target> {
        self.fixture.by_label(label)
    }

    fn get(&self, label: &str) -> Target {
        self.find(label).unwrap_or_else(|| panic!("nothing in the dock is labelled `{label}`"))
    }

    fn click(&mut self, label: &str) {
        let target = self.get(label);
        self.fixture.click(&target);
        self.settle();
    }

    fn fire_chord(&mut self, key: Key, modifiers: Modifiers) {
        let catalog = self.app.borrow().catalog();
        self.fixture.catalog(catalog);
        self.fixture.settle();
        craft_ui::runtime::frame::take_output(self.fixture.ctx());
        self.fixture.key(key, modifiers);
        let fired = craft_ui::runtime::frame::take_output(self.fixture.ctx()).fired;
        for fired in fired {
            self.app.borrow_mut().run(&fired.command.id);
        }
        self.settle();
    }
}

#[test]
fn the_probe_counter_docks_beside_the_canvas_and_a_click_on_count_reaches_the_plugin_and_redraws() {
    let mut dock = Dock::new();
    assert!(dock.app.borrow().shows("counter"), "the counter panel docks at its default place");
    assert!(dock.find("0 clicks at speed 0").is_some(), "the counter shows the plugin's first reply");
    dock.click("Count");
    assert!(dock.find("1 clicks at speed 0").is_some(), "the click reached the plugin and its new reply is drawn");
    dock.click("Count");
    assert!(dock.find("2 clicks at speed 0").is_some(), "every click is one plugin event");
}

#[test]
fn the_files_list_scrolls_to_later_rows_and_activating_one_opens_the_floating_inspector() {
    let mut dock = Dock::new();
    dock.click("Files");
    let first = dock.get("file 2");
    dock.fixture.frame(vec![Event::PointerMoved(first.center())]);
    for _ in 0..30 {
        let wheel = Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: Vec2::new(0.0, -2_000.0),
            phase: TouchPhase::Move,
            source: MouseWheelSource::Wheel,
            modifiers: Modifiers::NONE,
        };
        dock.fixture.frame(vec![wheel]);
    }
    dock.settle();
    let later = match dock.app.borrow().widgets("files") {
        [craft_plugin::Widget::List { first, .. }, ..] => *first,
        other => panic!("the files panel shows its list: {other:?}"),
    };
    assert!(later > 100, "the plugin built a later window after the scroll, at row {later}");
    let shown = (later..later + 128).find(|row| dock.find(&format!("file {row}")).is_some()).expect("a row of the new window is in view");
    dock.click(&format!("file {shown}"));
    dock.fixture.key(Key::Enter, Modifiers::NONE);
    dock.settle();
    assert!(dock.app.borrow().floats("inspector"), "activating a row floats the inspector");
    assert!(dock.find(&format!("inspecting file {shown}")).is_some(), "the floating inspector shows the activated row");
}

#[test]
fn a_plugin_command_runs_from_its_chord_through_the_app_catalog() {
    let mut dock = Dock::new();
    dock.fire_chord(Key::J, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(dock.app.borrow().calls.commands, vec!["comp.info".to_string()], "Cmd+Shift+J ran the probe's Read Document");
}

#[test]
fn a_plugin_notice_reaches_the_notice_center() {
    let mut dock = Dock::new();
    dock.app.borrow_mut().run("plugin.test.probe.greet");
    dock.settle();
    assert_eq!(dock.app.borrow().posted(), ["3 layers renamed".to_string()]);
    assert!(dock.find("3 layers renamed").is_some(), "the notice shows");
}

#[test]
fn unloading_the_plugin_hides_its_panel_and_reloading_brings_it_back_where_it_was() {
    let mut dock = Dock::new();
    let places = |dock: &Dock| {
        let layout = &dock.app.borrow().workspace.layout;
        let panels: Vec<String> = layout.panels().into_iter().filter(|panel| panel.starts_with("plugin.")).cloned().collect();
        panels.into_iter().map(|panel| (layout.location(&panel).ok(), layout.panel_state(&panel), panel)).collect::<Vec<_>>()
    };
    let before = places(&dock);
    assert!(before.len() > 2, "the plugin's panels are docked: {before:?}");
    dock.app.borrow_mut().unload();
    dock.settle();
    assert!(!dock.app.borrow().shows("counter"), "the unloaded plugin's panel leaves the layout");
    assert!(dock.find("Count").is_none(), "nothing of the plugin is drawn");
    dock.app.borrow_mut().load();
    dock.settle();
    assert_eq!(places(&dock), before, "every reloaded panel is back at its place, in its group's order, the counter in front");
    assert!(dock.find("Count").is_some(), "the reloaded counter draws again");
}
