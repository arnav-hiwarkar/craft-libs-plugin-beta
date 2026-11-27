use craft_plugin::{CommandSpec, MenuItem, PANEL_MENU, PanelPlace, PanelSize, PanelSpec, UiEvent, UiEventKind};
use craft_theme::Tokens;
use craft_ui::contract::SizeLimits;
use craft_ui::menu::PanelRow;
use craft_ui::runtime::commands::{Binding, CommandDef, CommandKind, CommandRef, CommandState, Scope};
use egui::Vec2;

pub const MENU: &str = "Plug-ins";

pub fn panel_key(plugin: &str, panel: &str) -> String {
    format!("plugin.{plugin}.{panel}")
}

pub fn command_key(plugin: &str, command: &str) -> String {
    format!("plugin.{plugin}.{command}")
}

fn steps(size: PanelSize) -> f32 {
    match size {
        PanelSize::Small => 1.0,
        PanelSize::Medium => 1.5,
        PanelSize::Large => 2.0,
    }
}

pub fn least(tokens: &Tokens, size: PanelSize) -> Vec2 {
    Vec2::new(tokens.size.length.field.md * steps(size), tokens.size.control.md * 4.0 * steps(size))
}

pub fn limits(tokens: &Tokens, size: PanelSize) -> SizeLimits {
    SizeLimits::at_least(least(tokens, size))
}

pub fn panel_spec(spec: &PanelSpec, tokens: &Tokens) -> craft_ui::docking::PanelSpec {
    let icon = crate::icons::spec(spec.icon.as_deref().unwrap_or_default(), &spec.title);
    let shown = craft_ui::docking::PanelSpec::new(spec.title.clone()).icon(icon).limits(limits(tokens, spec.size));
    match spec.place {
        PanelPlace::Float => shown.float_size(least(tokens, spec.size) * 1.5),
        _ => shown,
    }
}

pub fn command_def(plugin: &str, spec: &CommandSpec) -> CommandDef {
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

pub const HELP: &str = "Help";

const MENU_ITEM: &str = "menu.";
const HELP_ITEM: &str = "help";

#[derive(Clone, Debug, PartialEq)]
pub enum PanelPick {
    Item(UiEvent),
    Help(String),
}

#[derive(Default)]
pub struct PanelMenu {
    pub rows: Vec<PanelRow>,
    pub commands: Vec<(CommandDef, CommandState)>,
}

pub fn command_state(enabled: bool, checked: Option<bool>) -> CommandState {
    let state = CommandState::new().enabled(enabled);
    match checked {
        Some(on) => state.checked(on),
        None => state,
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

pub fn panel_menu(plugin: &str, spec: &PanelSpec, items: &[MenuItem]) -> PanelMenu {
    let panel = panel_key(plugin, &spec.id);
    let mut menu = PanelMenu::default();
    menu.rows = menu_rows(&panel, items, &mut 0, &mut menu.commands);
    if spec.help.is_some() {
        if !menu.rows.is_empty() {
            menu.rows.push(PanelRow::Separator);
        }
        let (def, state) = row_command(format!("{panel}.{HELP_ITEM}"), HELP, true, None);
        menu.rows.push(PanelRow::Command(def.command.clone()));
        menu.commands.push((def, state));
    }
    menu
}

pub fn panel_pick(plugin: &str, spec: &PanelSpec, command: &str) -> Option<PanelPick> {
    let picked = command.strip_prefix(&panel_key(plugin, &spec.id))?.strip_prefix('.')?;
    if picked == HELP_ITEM {
        return spec.help.clone().map(PanelPick::Help);
    }
    let index: u32 = picked.strip_prefix(MENU_ITEM)?.parse().ok()?;
    Some(PanelPick::Item(UiEvent { id: PANEL_MENU, kind: UiEventKind::Number(f64::from(index)) }))
}
