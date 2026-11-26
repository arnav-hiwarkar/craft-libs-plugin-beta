use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use craft_behavior::dock::{Column, DockEdge, Group, Layout, Node, PanelState, Permissions, SplitSize, Workspace};
use craft_plugin::{Call, Contributions, PanelPlace, PanelReply, UiEvent};
use craft_plugin_host::testing::probe_dir;
use craft_plugin_host::{Access, Bundle, Limits, Plugin, Retained};
use craft_plugin_ui_framework::dock::{command_def, panel_key, panel_spec};
use craft_theme::Tokens;
use craft_ui::contract::{Bind, BodyCx};
use craft_ui::docking::{DockConfig, DockPanels, PanelSpec};
use craft_ui::notices::{NoticeCenter, PostedNotice};
use craft_ui::patterns::dock::DockArea;
use craft_ui::runtime::commands::AppCatalog;
use craft_ui::runtime::env::{Env, WindowKey};
use craft_ui::shell::AppShell;
use egui::{Pos2, Rect, Ui, Vec2};
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

    fn panel(&mut self, panel: &str, _instance: Option<&str>, open: bool) -> Result<(), String> {
        self.shown.push((panel.to_string(), open));
        Ok(())
    }
}

pub struct App {
    pub loaded: Option<Loaded>,
    contributions: Contributions,
    pub workspace: Workspace<String>,
    retained: BTreeMap<String, Retained>,
    sizes: BTreeMap<String, [f32; 2]>,
    pub calls: Calls,
    posted: Vec<String>,
    tokens: Option<Tokens>,
    parked: (Vec<String>, Vec<String>),
    generation: u64,
    now: u64,
}

fn default_workspace(panels: &[String]) -> Workspace<String> {
    let end = Node::dock(DockEdge::End, vec![Column::new(vec![Group::new(panels.to_vec(), 0)])]);
    let root = Node::split(craft_behavior::Axis::X, SplitSize::Ratio(0.6), Node::tabs(vec![CANVAS.to_string()], 0), end);
    Workspace::new(Layout::new(Some(root), Vec::new()))
}

fn permissions(panel: &String) -> Permissions {
    if panel == CANVAS { Permissions::PROTECTED } else { Permissions::ALL }
}

impl App {
    pub fn new() -> App {
        let mut app = App {
            loaded: None,
            contributions: Contributions::default(),
            workspace: default_workspace(&[]),
            retained: BTreeMap::new(),
            sizes: BTreeMap::new(),
            calls: Calls::default(),
            posted: Vec::new(),
            tokens: None,
            parked: (Vec::new(), Vec::new()),
            generation: 0,
            now: 0,
        };
        app.load();
        app
    }

    fn docked(&self) -> Vec<String> {
        self.contributions.panels.iter().filter(|panel| panel.place != PanelPlace::Float).map(|panel| panel_key(PROBE, &panel.id)).collect()
    }

    fn registered(&self, panel: &String) -> bool {
        panel == CANVAS || self.contributions.panels.iter().any(|spec| panel_key(PROBE, &spec.id) == *panel)
    }

    fn reconcile(&mut self) {
        let default = default_workspace(&self.docked());
        let registered: BTreeSet<String> = self.contributions.panels.iter().map(|spec| panel_key(PROBE, &spec.id)).collect();
        self.workspace.reconcile(|panel| panel == CANVAS || registered.contains(panel), permissions, &default);
        self.generation += 1;
    }

    pub fn load(&mut self) {
        let loaded = load_probe();
        self.contributions = loaded.plugin.contributions().clone();
        self.loaded = Some(loaded);
        self.retained = self.contributions.panels.iter().map(|spec| (panel_key(PROBE, &spec.id), Retained::new())).collect();
        self.reconcile();
        let (parked, front) = std::mem::take(&mut self.parked);
        for panel in parked.iter().chain(&front) {
            self.workspace.set_visible(panel.clone(), true, None, false, permissions).unwrap();
        }
    }

    pub fn unload(&mut self) {
        let ours = |panel: &&String| panel.starts_with(&format!("plugin.{PROBE}."));
        let parked: Vec<String> = self.workspace.layout.panels().into_iter().filter(ours).cloned().collect();
        let front = parked.iter().filter(|panel| self.workspace.layout.panel_state(panel) == PanelState::Front).cloned().collect();
        self.parked = (parked, front);
        self.loaded = None;
        self.contributions = Contributions::default();
        self.retained.clear();
        self.reconcile();
    }

    pub fn catalog(&self) -> AppCatalog {
        let defs = self.contributions.commands.iter().map(|spec| command_def(PROBE, spec)).collect();
        AppCatalog::new(defs, self.generation).expect("the plugin's commands make one catalog")
    }

    pub fn shows(&self, panel: &str) -> bool {
        self.workspace.layout.contains(&panel_key(PROBE, panel))
    }

    pub fn floats(&self, panel: &str) -> bool {
        let key = panel_key(PROBE, panel);
        self.workspace.layout.floating.iter().any(|window| window.column.groups.iter().any(|group| group.panels.contains(&key)))
    }

    pub fn show(&mut self, ui: &mut Ui) {
        AppShell::embedded(WindowKey::main()).body(|ui, _, app: &mut App| app.dock(ui)).show(ui, self);
    }

    fn dock(&mut self, ui: &mut Ui) {
        self.tokens = Some(Tokens::clone(&Env::of(ui).tokens));
        let mut workspace = std::mem::take(&mut self.workspace);
        DockArea::new("Workspace", Bind::bound(&mut workspace), self, DockConfig::default()).show(ui);
        self.workspace = workspace;
        NoticeCenter::new().show(ui);
        for message in std::mem::take(&mut self.calls.notices) {
            NoticeCenter::new().post(ui.ctx(), &PostedNotice::new(message.clone()), &[]);
            self.posted.push(message);
        }
    }

    pub fn posted(&self) -> &[String] {
        &self.posted
    }

    pub fn run(&mut self, command: &str) {
        let Some(id) = command.strip_prefix(&format!("plugin.{PROBE}.")) else { return };
        if let Some(loaded) = &self.loaded {
            loaded.plugin.call::<()>(&Call::Command { command: id.into() }, None, &mut self.calls).unwrap();
        }
    }

    pub fn pump(&mut self) {
        self.now += 16;
        let Some(loaded) = &self.loaded else { return };
        for spec in &self.contributions.panels {
            let key = panel_key(PROBE, &spec.id);
            let Some(size) = self.sizes.get(&key).copied() else { continue };
            let Some(retained) = self.retained.get_mut(&key) else { continue };
            let Some(input) = retained.start(size, Duration::from_millis(self.now)) else { continue };
            let reply = loaded.plugin.call::<PanelReply>(&Call::Panel { panel: spec.id.clone(), input }, None, &mut self.calls);
            retained.finish(reply.map_err(|error| error.to_string()), Duration::from_millis(self.now));
        }
        self.sizes.clear();
        for (panel, open) in std::mem::take(&mut self.calls.shown) {
            let key = panel_key(PROBE, &panel);
            let action = match (open, self.workspace.layout.contains(&key)) {
                (true, false) => craft_behavior::dock::Action::float(key, Rect::from_min_size(Pos2::new(40.0, 40.0), Vec2::new(260.0, 200.0))),
                (true, true) => craft_behavior::dock::Action::activate(key),
                (false, _) => craft_behavior::dock::Action::close(key),
            };
            self.workspace.apply(action, false, permissions).unwrap();
        }
    }

    pub fn widgets(&self, panel: &str) -> &[craft_plugin::Widget] {
        self.retained.get(&panel_key(PROBE, panel)).map_or(&[], |retained| retained.widgets())
    }
}

impl DockPanels<String> for App {
    fn spec(&self, panel: &String) -> PanelSpec {
        let spec = self.contributions.panels.iter().find(|spec| panel_key(PROBE, &spec.id) == *panel);
        match (spec, &self.tokens) {
            (Some(spec), Some(tokens)) => panel_spec(spec, tokens),
            _ => PanelSpec::new(panel.clone()),
        }
    }

    fn permissions(&self, panel: &String) -> Permissions {
        permissions(panel)
    }

    fn body(&mut self, panel: &String, ui: &mut Ui, _: &mut BodyCx<'_>) {
        if !self.registered(panel) || panel == CANVAS {
            return;
        }
        let room = ui.available_size();
        self.sizes.insert(panel.clone(), [room.x, room.y]);
        let Some(retained) = self.retained.get(panel) else { return };
        let widgets = retained.widgets().to_vec();
        let events: Vec<UiEvent> = craft_plugin_ui_framework::draw(ui, &widgets, panel, &mut ());
        if let Some(retained) = self.retained.get_mut(panel)
            && !events.is_empty()
        {
            retained.push(events, "");
        }
    }
}
