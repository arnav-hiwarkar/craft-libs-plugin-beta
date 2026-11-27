use std::collections::BTreeMap;
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use craft_plugin::{Call, Contributions, PanelReply, UiEvent};
use craft_plugin_host::testing::probe_dir;
use craft_plugin_host::{Access, Bundle, Limits, Plugin, Retained};
use craft_plugin_ui_framework::host::{Bodies, Dock, Pick, panel_key};
use craft_ui::notices::{NoticeCenter, PostedNotice};
use craft_ui::runtime::env::WindowKey;
use craft_ui::shell::AppShell;
use egui::{Ui, Vec2};
use serde_json::{Value, json};

pub const CANVAS: &str = "canvas";
pub const PROBE: &str = "test.probe";

fn build() {
    static BUILT: OnceLock<()> = OnceLock::new();
    BUILT.get_or_init(|| {
        let cargo = std::path::Path::new(env!("CARGO"));
        let status = Command::new(cargo)
            .env("RUSTC", cargo.with_file_name("rustc"))
            .args(["build", "--release", "--quiet", "--target", "wasm32-unknown-unknown", "--manifest-path"])
            .arg(probe_dir().join("Cargo.toml"))
            .status()
            .expect("cargo runs");
        assert!(status.success(), "the probe plugin builds");
    });
}

pub struct Loaded {
    _bundle: tempfile::TempDir,
    _data: tempfile::TempDir,
    pub plugin: Plugin,
}

pub fn load_probe() -> Loaded {
    build();
    let bundle = tempfile::tempdir().unwrap();
    std::fs::copy(probe_dir().join("target/wasm32-unknown-unknown/release/probe.wasm"), bundle.path().join("probe.wasm")).unwrap();
    let manifest = json!({"id": PROBE, "name": "Probe", "version": "1.0.0", "entry": {"wasm": "probe.wasm"}, "permissions": ["document.read"]});
    std::fs::write(bundle.path().join("plugin.json"), manifest.to_string()).unwrap();
    let opened = Bundle::open(bundle.path()).unwrap();
    let data = tempfile::tempdir().unwrap();
    let limits = Limits { interactive: Duration::from_millis(2_000), work: Duration::from_millis(2_000) };
    let plugin = Plugin::load(&opened, opened.manifest.permissions.clone(), data.path().join("data"), limits).unwrap();
    Loaded { _bundle: bundle, _data: data, plugin }
}

#[derive(Default)]
pub struct Calls {
    pub commands: Vec<String>,
    pub shown: Vec<(String, bool)>,
    pub notices: Vec<String>,
    pub states: BTreeMap<String, (bool, Option<bool>)>,
}

impl craft_plugin_host::Host for Calls {
    fn access(&self, _command: &str, _params: &Value) -> Access {
        Access::Needs(vec![craft_plugin::Permission::DocumentRead])
    }

    fn command(&mut self, id: &str, _params: Value) -> Result<Value, String> {
        self.commands.push(id.to_string());
        Ok(json!({}))
    }

    fn notify(&mut self, message: &str) {
        self.notices.push(message.to_string());
    }

    fn command_state(&mut self, command: &str, enabled: bool, checked: Option<bool>) -> Result<(), String> {
        self.states.insert(command.to_string(), (enabled, checked));
        Ok(())
    }

    fn panel(&mut self, panel: &str, _instance: Option<&str>, open: bool) -> Result<(), String> {
        self.shown.push((panel.to_string(), open));
        Ok(())
    }
}

pub struct App {
    pub loaded: Option<Loaded>,
    contributions: Contributions,
    pub dock: Dock,
    panels: Panels,
    pub calls: Calls,
    posted: Vec<String>,
    now: u64,
    pub opened: Vec<String>,
}

#[derive(Default)]
struct Panels {
    contributions: Contributions,
    retained: BTreeMap<String, Retained>,
    sizes: BTreeMap<String, [f32; 2]>,
    frames: Vec<craft_plugin::FrameRequest>,
}

struct PanelHost<'a> {
    keys: &'a [String],
    pressed: Vec<String>,
    frames: &'a mut Vec<craft_plugin::FrameRequest>,
}

impl craft_plugin_ui_framework::Host for PanelHost<'_> {
    fn keys(&self) -> &[String] {
        self.keys
    }

    fn pressed(&mut self, chord: &str) {
        self.pressed.push(chord.to_string());
    }

    fn frame(&mut self, ui: &mut Ui, request: &craft_plugin::FrameRequest, _: Vec2) -> Option<egui::Response> {
        use craft_ui::contract::Show as _;
        self.frames.push(request.clone());
        Some(craft_ui::buttons::Button::new("Frame").show(ui).response)
    }
}

impl Bodies for Panels {
    fn body(&mut self, panel: &str, ui: &mut Ui) {
        let Some(spec) = self.contributions.panels.iter().find(|spec| panel_key(PROBE, &spec.id) == panel) else { return };
        let room = ui.available_size();
        self.sizes.insert(panel.to_string(), [room.x, room.y]);
        let Some(retained) = self.retained.get(panel) else { return };
        let widgets = retained.widgets().to_vec();
        let mut host = PanelHost { keys: &spec.keys, pressed: Vec::new(), frames: &mut self.frames };
        let events: Vec<UiEvent> = craft_plugin_ui_framework::draw(ui, &widgets, panel, &mut host);
        let pressed = host.pressed;
        if let Some(retained) = self.retained.get_mut(panel) {
            for chord in &pressed {
                retained.press(chord);
            }
            if !events.is_empty() {
                retained.push(events, "");
            }
        }
    }
}

impl App {
    pub fn new() -> App {
        let mut app = App {
            loaded: None,
            contributions: Contributions::default(),
            dock: Dock::new(vec![CANVAS.to_string()]),
            panels: Panels::default(),
            calls: Calls::default(),
            posted: Vec::new(),
            now: 0,
            opened: Vec::new(),
        };
        app.load();
        app
    }

    pub fn load(&mut self) {
        let loaded = load_probe();
        self.contributions = loaded.plugin.contributions().clone();
        self.loaded = Some(loaded);
        self.panels.contributions = self.contributions.clone();
        self.panels.retained = self.contributions.panels.iter().map(|spec| (panel_key(PROBE, &spec.id), Retained::new())).collect();
        self.dock.register(PROBE, &self.contributions);
    }

    pub fn unload(&mut self) {
        self.loaded = None;
        self.contributions = Contributions::default();
        self.panels.contributions = Contributions::default();
        self.panels.retained.clear();
        self.dock.unregister(PROBE);
    }

    pub fn shows(&self, panel: &str) -> bool {
        self.dock.layout().contains(&panel_key(PROBE, panel))
    }

    pub fn floats(&self, panel: &str) -> bool {
        let key = panel_key(PROBE, panel);
        self.dock.layout().floating.iter().any(|window| window.column.groups.iter().any(|group| group.panels.contains(&key)))
    }

    pub fn show(&mut self, ui: &mut Ui) {
        AppShell::embedded(WindowKey::main()).body(|ui, _, app: &mut App| app.draw(ui)).show(ui, self);
    }

    fn draw(&mut self, ui: &mut Ui) {
        self.dock.toolbar(ui);
        self.dock.show(ui, &mut self.panels);
        NoticeCenter::new().show(ui);
        for message in std::mem::take(&mut self.calls.notices) {
            NoticeCenter::new().post(ui.ctx(), &PostedNotice::new(message.clone()), &[]);
            self.posted.push(message);
        }
    }

    pub fn posted(&self) -> &[String] {
        &self.posted
    }

    pub fn frames(&self) -> &[craft_plugin::FrameRequest] {
        &self.panels.frames
    }

    pub fn run(&mut self, command: &str) {
        match self.dock.pick(command) {
            Some(Pick::Menu { panel, event, .. }) => {
                if let Some(retained) = self.panels.retained.get_mut(&panel_key(PROBE, &panel)) {
                    retained.push(vec![event], "");
                }
            }
            Some(Pick::Help(url)) => self.opened.push(url),
            Some(Pick::Command { command, .. }) => {
                if let Some(loaded) = &self.loaded {
                    loaded.plugin.call::<()>(&Call::Command { command }, None, &mut self.calls).unwrap();
                }
            }
            None => {}
        }
    }

    pub fn pump(&mut self) {
        self.now += 16;
        let Some(loaded) = &self.loaded else { return };
        for spec in &self.contributions.panels {
            let key = panel_key(PROBE, &spec.id);
            let Some(size) = self.panels.sizes.get(&key).copied() else { continue };
            let Some(retained) = self.panels.retained.get_mut(&key) else { continue };
            let Some(input) = retained.start(size, Duration::from_millis(self.now)) else { continue };
            let reply = loaded.plugin.call::<PanelReply>(&Call::Panel { panel: spec.id.clone(), input }, None, &mut self.calls);
            retained.finish(reply.map_err(|error| error.to_string()), Duration::from_millis(self.now));
            self.dock.set_menu(PROBE, &spec.id, retained.menu());
        }
        self.panels.sizes.clear();
        for (command, (enabled, checked)) in &self.calls.states {
            self.dock.set_state(PROBE, command, *enabled, *checked);
        }
        for (panel, open) in std::mem::take(&mut self.calls.shown) {
            self.dock.show_panel(PROBE, &panel, open);
        }
    }

    pub fn bring(&mut self, panel: &str) {
        self.dock.bring(PROBE, panel);
    }

    pub fn widgets(&self, panel: &str) -> &[craft_plugin::Widget] {
        self.panels.retained.get(&panel_key(PROBE, panel)).map_or(&[], |retained| retained.widgets())
    }
}
