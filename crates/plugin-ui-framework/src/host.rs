use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use craft_behavior::dock::{Action, Column, DockEdge, Group, Layout, Node, PanelState, Permissions, SplitSize, Workspace};
use craft_plugin::{CommandSpec, Contributions, MenuItem, PANEL_MENU, PanelPlace, PanelSize, PanelSpec, UiEvent, UiEventKind};
use craft_ui::contract::{Bind, BodyCx, PlainData, Show as _, ShowContainer as _, SizeLimits, Tracked};
use craft_ui::docking::{DockConfig, DockPanels, PanelSpecs};
use craft_ui::menu::PanelRow;
use craft_ui::patterns::dock::DockArea;
use craft_ui::runtime::commands::{AppCatalog, Binding, CommandDef, CommandInput, CommandKind, CommandRef, CommandState, CommandStates, Scope};
use craft_ui::runtime::env::Env;
use egui::{Pos2, Rect, Ui, Vec2};

pub const MENU: &str = "Plug-ins";
pub const HELP: &str = "Help";

const MENU_ITEM: &str = "menu.";
const HELP_ITEM: &str = "help";
const SHARE: f32 = 0.6;

pub fn panel_key(plugin: &str, panel: &str) -> String {
    format!("plugin.{plugin}.{panel}")
}

pub fn command_key(plugin: &str, command: &str) -> String {
    format!("plugin.{plugin}.{command}")
}

#[derive(Clone, Debug, PartialEq)]
pub enum Pick {
    Command { plugin: String, command: String },
    Menu { plugin: String, panel: String, event: UiEvent },
    Help(String),
}

pub trait Bodies {
    fn body(&mut self, panel: &str, ui: &mut Ui);
}

#[derive(Clone, Debug, Default, PartialEq, PlainData)]
struct Specs {
    center: Vec<String>,
    #[plain_data(assume = "a plug-in's panel specs are owned strings and enums")]
    plugins: Vec<(String, String, PanelSpec)>,
    least: [f32; 2],
}

impl Specs {
    fn plugin(&self, key: &str) -> Option<&PanelSpec> {
        self.plugins.iter().find(|(_, held, _)| held == key).map(|(_, _, spec)| spec)
    }

    fn least(&self, size: PanelSize) -> Vec2 {
        let steps = match size {
            PanelSize::Small => 1.0,
            PanelSize::Medium => 1.5,
            PanelSize::Large => 2.0,
        };
        Vec2::new(self.least[0] * steps, self.least[1] * 4.0 * steps)
    }
}

impl PanelSpecs<String> for Specs {
    fn spec(&self, panel: &String) -> craft_ui::docking::PanelSpec {
        let Some(spec) = self.plugin(panel) else { return craft_ui::docking::PanelSpec::new(panel.clone()) };
        let icon = crate::icons::spec(spec.icon.as_deref().unwrap_or_default(), &spec.title);
        let least = self.least(spec.size);
        let shown = craft_ui::docking::PanelSpec::new(spec.title.clone()).icon(icon).limits(SizeLimits::at_least(least));
        match spec.place {
            PanelPlace::Float => shown.float_size(least * 1.5),
            _ => shown,
        }
    }

    fn permissions(&self, panel: &String) -> Permissions {
        if self.center.contains(panel) { Permissions::PROTECTED } else { Permissions::ALL }
    }
}

struct Menu {
    rows: Vec<PanelRow>,
    commands: Vec<(CommandDef, CommandState)>,
}

pub struct Dock {
    workspace: Arc<Workspace<String>>,
    specs: Tracked<Specs>,
    commands: BTreeMap<String, Vec<CommandSpec>>,
    menus: BTreeMap<String, (Vec<MenuItem>, Menu)>,
    states: Tracked<CommandStates>,
    plugin_states: BTreeMap<String, CommandState>,
    parked: BTreeMap<String, Vec<String>>,
    generation: u64,
}

fn command_state(enabled: bool, checked: Option<bool>) -> CommandState {
    let state = CommandState::new().enabled(enabled);
    match checked {
        Some(on) => state.checked(on),
        None => state,
    }
}

fn command_def(plugin: &str, spec: &CommandSpec) -> CommandDef {
    let path: Vec<String> = match &spec.menu {
        Some(menu) => menu.split('/').map(str::trim).filter(|part| !part.is_empty()).map(String::from).collect(),
        None => vec![MENU.to_string()],
    };
    let mut def = CommandDef::new(CommandRef::new(command_key(plugin, &spec.id)), spec.title.clone(), Scope::app());
    def.paths = vec![path];
    def.defaults = spec.shortcut.iter().filter_map(|keys| Binding::parse(keys).ok()).collect();
    match spec.icon.as_deref().and_then(crate::icons::framework_name) {
        Some(icon) => def.with_icon(icon),
        None => def,
    }
}

fn row_command(key: String, label: &str, enabled: bool, checked: Option<bool>) -> (CommandDef, CommandState) {
    let mut def = CommandDef::new(CommandRef::new(key), label.to_string(), Scope::app());
    if checked.is_some() {
        def.kind = CommandKind::Toggle;
    }
    (def, command_state(enabled, checked))
}

fn menu_rows(panel: &str, items: &[MenuItem], next: &mut u32, commands: &mut Vec<(CommandDef, CommandState)>) -> Vec<PanelRow> {
    let mut rows = Vec::with_capacity(items.len());
    for item in items {
        let index = *next;
        *next += 1;
        if item.separator {
            rows.push(PanelRow::Separator);
        } else if item.items.is_empty() {
            let (def, state) = row_command(format!("{panel}.{MENU_ITEM}{index}"), &item.text, !item.disabled, item.checked);
            rows.push(PanelRow::Command(def.command.clone()));
            commands.push((def, state));
        } else {
            rows.push(PanelRow::Submenu(item.text.clone().into(), menu_rows(panel, &item.items, next, commands)));
        }
    }
    rows
}

fn panel_menu(key: &str, spec: &PanelSpec, items: &[MenuItem]) -> Menu {
    let mut commands = Vec::new();
    let mut rows = menu_rows(key, items, &mut 0, &mut commands);
    if spec.help.is_some() {
        if !rows.is_empty() {
            rows.push(PanelRow::Separator);
        }
        let (def, state) = row_command(format!("{key}.{HELP_ITEM}"), HELP, true, None);
        rows.push(PanelRow::Command(def.command.clone()));
        commands.push((def, state));
    }
    Menu { rows, commands }
}

fn default_workspace(center: &[String], docked: Vec<String>) -> Workspace<String> {
    let middle = Node::tabs(center.to_vec(), 0);
    let root = match docked.is_empty() {
        true => middle,
        false => Node::split(
            craft_behavior::Axis::X,
            SplitSize::Ratio(SHARE),
            middle,
            Node::dock(DockEdge::End, vec![Column::new(vec![Group::new(docked, 0)])]),
        ),
    };
    Workspace::new(Layout::new(Some(root), Vec::new()))
}

struct Shown<'a> {
    bodies: &'a mut dyn Bodies,
    menus: &'a BTreeMap<String, (Vec<MenuItem>, Menu)>,
}

impl DockPanels<String> for Shown<'_> {
    fn body(&mut self, panel: &String, ui: &mut Ui, _: &mut BodyCx<'_>) {
        self.bodies.body(panel, ui);
    }

    fn panel_rows(&mut self, panel: &String) -> Vec<PanelRow> {
        self.menus.get(panel).map_or_else(Vec::new, |(_, menu)| menu.rows.clone())
    }
}

impl Dock {
    pub fn new(center: Vec<String>) -> Dock {
        let workspace = Arc::new(default_workspace(&center, Vec::new()));
        let specs = Tracked::new(Specs { center, ..Specs::default() });
        Dock {
            workspace,
            specs,
            commands: BTreeMap::new(),
            menus: BTreeMap::new(),
            states: Tracked::default(),
            plugin_states: BTreeMap::new(),
            parked: BTreeMap::new(),
            generation: 0,
        }
    }

    fn reconcile(&mut self) {
        let docked = self.specs.plugins.iter().filter(|(_, _, spec)| spec.place != PanelPlace::Float).map(|(_, key, _)| key.clone()).collect();
        let default = default_workspace(&self.specs.center, docked);
        let registered: BTreeSet<String> = self.specs.center.iter().chain(self.specs.plugins.iter().map(|(_, key, _)| key)).cloned().collect();
        let specs = &*self.specs;
        Arc::make_mut(&mut self.workspace).reconcile(|panel| registered.contains(panel), |panel| specs.permissions(panel), &default);
        self.generation += 1;
        self.refresh_states();
    }

    pub fn register(&mut self, plugin: &str, contributions: &Contributions) {
        self.specs.edit(|specs| {
            specs.plugins.retain(|(owner, _, _)| owner != plugin);
            specs.plugins.extend(contributions.panels.iter().map(|spec| (plugin.to_string(), panel_key(plugin, &spec.id), spec.clone())));
        });
        self.commands.insert(plugin.to_string(), contributions.commands.clone());
        self.reconcile();
        let parked = self.parked.remove(plugin).unwrap_or_default();
        let workspace = Arc::make_mut(&mut self.workspace);
        let specs = &*self.specs;
        for panel in parked {
            let _ = workspace.set_visible(panel, true, None, false, |panel| specs.permissions(panel));
        }
    }

    pub fn unregister(&mut self, plugin: &str) {
        let prefix = panel_key(plugin, "");
        let layout = self.workspace.layout();
        let mut parked: Vec<String> = layout.panels().into_iter().filter(|panel| panel.starts_with(&prefix)).cloned().collect();
        let front: Vec<String> = parked.iter().filter(|panel| layout.panel_state(panel) == PanelState::Front).cloned().collect();
        parked.extend(front);
        self.parked.insert(plugin.to_string(), parked);
        self.specs.edit(|specs| specs.plugins.retain(|(owner, _, _)| owner != plugin));
        self.commands.remove(plugin);
        self.menus.retain(|key, _| !key.starts_with(&prefix));
        self.plugin_states.retain(|key, _| !key.starts_with(&prefix));
        self.reconcile();
    }

    pub fn set_state(&mut self, plugin: &str, command: &str, enabled: bool, checked: Option<bool>) {
        let state = command_state(enabled, checked);
        let key = command_key(plugin, command);
        if self.plugin_states.get(&key) != Some(&state) {
            self.plugin_states.insert(key, state);
            self.refresh_states();
        }
    }

    pub fn set_menu(&mut self, plugin: &str, panel: &str, items: &[MenuItem]) {
        let key = panel_key(plugin, panel);
        if self.menus.get(&key).is_some_and(|(held, _)| held == items) {
            return;
        }
        let Some(spec) = self.specs.plugin(&key) else { return };
        let menu = panel_menu(&key, spec, items);
        self.menus.insert(key, (items.to_vec(), menu));
        self.generation += 1;
        self.refresh_states();
    }

    fn refresh_states(&mut self) {
        let mut states = CommandStates::new();
        for (key, state) in &self.plugin_states {
            states.set(CommandRef::new(key.clone()), state.clone());
        }
        for (_, menu) in self.menus.values() {
            for (def, state) in &menu.commands {
                states.set(def.command.clone(), state.clone());
            }
        }
        if *self.states != states {
            *self.states = states;
        }
    }

    pub fn commands(&self) -> CommandInput {
        let mut defs: Vec<CommandDef> = self.commands.iter().flat_map(|(plugin, specs)| specs.iter().map(|spec| command_def(plugin, spec))).collect();
        defs.extend(self.menus.values().flat_map(|(_, menu)| menu.commands.iter().map(|(def, _)| def.clone())));
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (self.generation, defs.iter().map(|def| (&def.command.id, &def.label)).collect::<Vec<_>>()).hash(&mut hasher);
        let input = CommandInput::new().states(&self.states);
        match AppCatalog::new(defs, hasher.finish()) {
            Ok(catalog) => input.catalog(catalog),
            Err(_) => input,
        }
    }

    pub fn pick(&self, command: &str) -> Option<Pick> {
        for (owner, key, spec) in &self.specs.plugins {
            let Some(picked) = command.strip_prefix(key.as_str()).and_then(|rest| rest.strip_prefix('.')) else { continue };
            if picked == HELP_ITEM {
                return spec.help.clone().map(Pick::Help);
            }
            if let Some(index) = picked.strip_prefix(MENU_ITEM).and_then(|index| index.parse::<u32>().ok()) {
                let event = UiEvent { id: PANEL_MENU, kind: UiEventKind::Number(f64::from(index)) };
                return Some(Pick::Menu { plugin: owner.clone(), panel: spec.id.clone(), event });
            }
        }
        self.commands.iter().find_map(|(plugin, specs)| {
            let id = command.strip_prefix(&command_key(plugin, ""))?;
            specs.iter().any(|spec| spec.id == id).then(|| Pick::Command { plugin: plugin.clone(), command: id.to_string() })
        })
    }

    pub fn show_panel(&mut self, plugin: &str, panel: &str, open: bool) {
        let key = panel_key(plugin, panel);
        let action = match (open, self.workspace.layout().contains(&key)) {
            (true, false) => {
                let least = self.specs.plugin(&key).map_or(Vec2::ZERO, |spec| self.specs.least(spec.size) * 1.5);
                Action::float(key, Rect::from_min_size(Pos2::new(least.x / 4.0, least.y / 4.0), least))
            }
            (true, true) => Action::activate(key),
            (false, _) => Action::close(key),
        };
        let specs = &*self.specs;
        let _ = Arc::make_mut(&mut self.workspace).apply(action, false, |panel| specs.permissions(panel));
    }

    pub fn bring(&mut self, plugin: &str, panel: &str) {
        let specs = &*self.specs;
        let _ = Arc::make_mut(&mut self.workspace).apply(Action::activate(panel_key(plugin, panel)), false, |panel| specs.permissions(panel));
    }

    pub fn layout(&self) -> &Layout<String> {
        self.workspace.layout()
    }

    pub fn toolbar(&self, ui: &mut Ui) {
        craft_ui::layout_components::Row::new().show(ui, |ui, _| {
            for (plugin, specs) in &self.commands {
                for spec in specs.iter().filter(|spec| spec.toolbar) {
                    craft_ui::buttons::Button::command(CommandRef::new(command_key(plugin, &spec.id))).show(ui);
                }
            }
        });
    }

    pub fn show(&mut self, ui: &mut Ui, bodies: &mut dyn Bodies) {
        let least = {
            let env = Env::of(ui);
            [env.tokens.size.length.field.md, env.tokens.size.control.md]
        };
        if self.specs.least != least {
            self.specs.edit(|specs| specs.least = least);
        }
        let mut shown = Shown { bodies, menus: &self.menus };
        DockArea::new("Workspace", Bind::bound(&mut self.workspace), &self.specs, &mut shown, DockConfig::default()).show(ui);
    }
}
