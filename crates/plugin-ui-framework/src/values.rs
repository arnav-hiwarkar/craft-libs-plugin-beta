use craft_color_ui::gradient::{ColorStop, GradientStops, OpacityStop};
use craft_color_ui::ui::PickedColor;
use craft_imaging::ui::curve::Curve;
use craft_theme::Srgba;
use craft_theme::color::{linear_to_srgb, srgb_decode};
use craft_ui::contract::EditResponse;
use craft_ui::runtime::env::Env;

pub(crate) const CURVE_LEVELS: u32 = 255;
pub(crate) const MAX_COLUMNS: u32 = 64;
const MAX_ROOM: f32 = 16_384.0;

pub fn changed<T: Clone + PartialEq>(out: &EditResponse<T>, given: &T) -> Option<T> {
    out.value().filter(|edited| *edited != given).cloned()
}

pub(crate) fn finite(value: f64, fallback: f64) -> f64 {
    if value.is_finite() { value } else { fallback }
}

pub(crate) fn range(min: f64, max: f64) -> (f64, f64) {
    let (low, high) = (finite(min, 0.0), finite(max, 1.0));
    if low <= high { (low, high) } else { (high, low) }
}

pub(crate) fn extent(points: f32) -> f32 {
    if points.is_finite() { points.clamp(0.0, MAX_ROOM) } else { 0.0 }
}

pub(crate) fn rows_height(ui: &egui::Ui, rows: u32) -> f32 {
    extent(Env::of(ui).tokens.size.row.text * rows as f32)
}

pub(crate) fn key_hash(key: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

fn encoded(linear: f32) -> f32 {
    linear_to_srgb(f64::from(linear).clamp(0.0, 1.0)) as f32
}

fn decoded(encoded: f32) -> f32 {
    srgb_decode(f64::from(encoded).clamp(0.0, 1.0)) as f32
}

pub(crate) fn srgba([r, g, b, a]: [f32; 4]) -> Srgba {
    Srgba::new(encoded(r), encoded(g), encoded(b), a.clamp(0.0, 1.0))
}

pub(crate) fn picked(value: [f32; 4]) -> PickedColor {
    PickedColor::rgb(srgba(value))
}

pub(crate) fn linear(picked: &PickedColor) -> [f32; 4] {
    let [r, g, b, a] = picked.color.to_array();
    [decoded(r), decoded(g), decoded(b), a]
}

pub(crate) fn curve(points: &[[f64; 2]]) -> Curve {
    let scale = CURVE_LEVELS as f32;
    Curve::smooth(points.iter().map(|[x, y]| [*x as f32 * scale, *y as f32 * scale]).collect())
}

pub(crate) fn points(curve: &Curve) -> Vec<[f64; 2]> {
    let scale = f64::from(CURVE_LEVELS);
    curve.points().unwrap_or_default().iter().map(|[x, y]| [f64::from(*x) / scale, f64::from(*y) / scale]).collect()
}

pub(crate) fn stops(gradient: &craft_plugin::Gradient) -> GradientStops {
    let color =
        gradient.colors.iter().map(|(at, [r, g, b, a])| ColorStop::new(*at as f32, srgba([*r as f32, *g as f32, *b as f32, *a as f32]))).collect();
    let opacity = gradient.opacities.iter().map(|(at, opacity)| OpacityStop::new(*at as f32, *opacity as f32)).collect();
    GradientStops::with_opacity(color, opacity)
}

pub(crate) fn gradient(stops: &GradientStops) -> craft_plugin::Gradient {
    let colors = stops
        .color
        .iter()
        .map(|stop| {
            let [r, g, b, a] = stop.color.to_array();
            (f64::from(stop.location), [f64::from(decoded(r)), f64::from(decoded(g)), f64::from(decoded(b)), f64::from(a)])
        })
        .collect();
    let opacities = stops.opacity_stops().iter().map(|stop| (f64::from(stop.location), f64::from(stop.opacity))).collect();
    craft_plugin::Gradient { colors, opacities }
}
