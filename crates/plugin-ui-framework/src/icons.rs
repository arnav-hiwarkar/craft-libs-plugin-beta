use craft_ui::primitives::IconSpec;
use std::borrow::Cow;

const RENAMED: &[(&str, &str)] = &[("pen", "pencil"), ("chart", "chart_line"), ("rotate", "rotate_cw"), ("zoom", "zoom_in")];

const DRAWN: &[&str] = &[
    "brush", "eraser", "wand", "sparkles", "palette", "droplet", "sun", "image", "text", "square", "circle", "star", "grid", "list", "layers",
    "folder", "file", "search", "settings", "info", "warning", "play", "pause", "plus", "minus", "trash", "copy", "eye", "lock", "link", "clock",
    "crop", "move", "flag", "bookmark", "refresh", "check", "close",
];

pub(crate) fn framework_name(kit: &str) -> Option<&'static str> {
    RENAMED.iter().find(|(from, _)| *from == kit).map(|(_, to)| *to).or_else(|| DRAWN.iter().find(|name| **name == kit).copied())
}

pub(crate) fn spec(kit: &str, title: &str) -> IconSpec<'static> {
    match framework_name(kit) {
        Some(name) => IconSpec::svg(Cow::Borrowed(name).into()),
        None => IconSpec::initial(title),
    }
}
