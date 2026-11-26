use craft_plugin::{Drag, Dropped, Paint};
use egui::{Color32, FontFamily};

#[cfg(any(test, feature = "conformance"))]
pub mod conformance;
mod form;
mod list;
mod marks;
mod panel;
#[cfg(test)]
mod tests;

pub use form::{Answer, draw_form, param_button};
pub use marks::{draw_marks, filled_polygon};
pub use panel::draw;

pub type Triangulate = fn(&[[f64; 2]]) -> Vec<[usize; 3]>;

#[derive(Clone)]
pub struct Theme {
    pub text: Color32,
    pub weak: Color32,
    pub accent: Color32,
    pub background: Color32,
    pub surface: Color32,
    pub border: Color32,
    pub error: Color32,
    pub good: Color32,
    pub warn: Color32,
    pub faint: Color32,
    pub selected: Color32,
    pub series: [Color32; 8],
    pub heading: FontFamily,
}

impl Theme {
    pub fn paint(&self, paint: Paint) -> Color32 {
        match paint {
            Paint::Text => self.text,
            Paint::Weak => self.weak,
            Paint::Accent => self.accent,
            Paint::Background => self.background,
            Paint::Surface => self.surface,
            Paint::Border => self.border,
            Paint::Error => self.error,
            Paint::Rgba([r, g, b, a]) => Color32::from_rgba_unmultiplied(r, g, b, a),
            Paint::Good => self.good,
            Paint::Warn => self.warn,
            Paint::Faint => self.faint,
            Paint::Selected => self.selected,
            Paint::Series(n) => self.series[usize::from(n) % self.series.len()],
        }
    }
}

pub trait Host {
    type Payload: std::any::Any + Send + Sync;

    fn tr(&self, english: &str) -> String;
    fn register(&mut self, id: &str, response: &egui::Response, label: &str);
    fn texture(&mut self, ctx: &egui::Context, key: &str) -> Option<egui::TextureId>;
    fn triangulate(&self) -> Triangulate;
    fn drag(&self, drag: &Drag) -> Option<Self::Payload>;
    fn dropped(&self, payload: &Self::Payload) -> Option<Dropped>;
    fn accept_files(&mut self, paths: &[String]);

    fn icon(&mut self, _painter: &egui::Painter, _rect: egui::Rect, _name: &str, _color: Color32) -> bool {
        false
    }

    #[expect(clippy::disallowed_types)]
    fn combo_box(&self, ui: &mut egui::Ui, id: egui::IdSalt, label: &str, selected: String, menu: &mut dyn FnMut(&mut egui::Ui)) -> egui::Response {
        egui::ComboBox::new(id, label).selected_text(selected).show_ui(ui, menu).response
    }
}
