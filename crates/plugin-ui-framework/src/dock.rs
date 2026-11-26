use craft_plugin::{CommandSpec, PanelPlace, PanelSize, PanelSpec};
use craft_theme::Tokens;
use craft_ui::contract::SizeLimits;
use craft_ui::runtime::commands::{Binding, CommandDef, CommandRef, Scope};
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
