#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod canvas;
mod containers;
pub mod dock;
mod icons;
mod list;
mod menu;
mod values;

use craft_plugin::{Dropped, FrameRequest, TextStyle, UiEvent, UiEventKind, Widget};
use craft_theme::TypeRole;
use craft_theme::variant::Tone;
use craft_ui::buttons::{Button, IconButton};
use craft_ui::choice::{Checkbox, Radio, RadioGroup, Switch};
use craft_ui::contract::{Bind, Entry, Outcome as _, Props, Show as _, ShowContainer as _};
use craft_ui::display::{Divider, Progress, Shade, Text};
use craft_ui::form::SliderField;
use craft_ui::layout_components::{Grid, Row, Stack};
use craft_ui::pickers::Select;
use craft_ui::runtime::env::Label;
use craft_ui::sections::Section;
use craft_ui::text::{NumberField, TextField};

pub use values::changed;

pub trait Host {
    fn picture(&self, _key: &str) -> Option<egui::ColorImage> {
        None
    }

    fn version(&self, _key: &str) -> u64 {
        0
    }

    fn frame(&mut self, _ui: &mut egui::Ui, _request: &FrameRequest, _size: egui::Vec2) -> Option<egui::Response> {
        None
    }

    fn triangulate(&self) -> craft_plugin_ui::Triangulate {
        canvas::ear_clipped
    }

    fn drag(&self, _drag: &craft_plugin::Drag) -> Option<Dropped> {
        None
    }

    fn accept_files(&mut self, _paths: &[String]) {}

    fn keys(&self) -> &[String] {
        &[]
    }

    fn pressed(&mut self, _chord: &str) {}
}

impl Host for () {}

pub fn draw(ui: &mut egui::Ui, widgets: &[Widget], scope: &str, host: &mut dyn Host) -> Vec<UiEvent> {
    let keys: Vec<(String, craft_keymap::Chord)> =
        host.keys().iter().filter_map(|key| craft_keymap::Chord::parse(key).ok().map(|chord| (key.clone(), chord))).collect();
    let mut painter = Painter { scope, host, events: Vec::new(), last: None };
    if keys.is_empty() {
        painter.all(ui, widgets);
        return painter.events;
    }
    let area = craft_ui::sections::KeyArea::new(keys.iter().map(|(_, chord)| *chord).collect())
        .id_salt(("craft-plugin-ui-framework.keys", scope))
        .show(ui, |ui, _| painter.all(ui, widgets));
    for chord in area.output.pressed {
        if let Some((key, _)) = keys.iter().find(|(_, claimed)| *claimed == chord) {
            painter.host.pressed(key);
        }
    }
    painter.events
}

pub(crate) struct Painter<'a> {
    pub(crate) scope: &'a str,
    host: &'a mut dyn Host,
    events: Vec<UiEvent>,
    pub(crate) last: Option<egui::Response>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Keys {
    Panel,
    Field,
}

#[derive(Default)]
pub(crate) struct Extras {
    tooltip: Option<String>,
    name: Option<String>,
}

impl Extras {
    fn of(after: &[Widget]) -> Extras {
        let mut extras = Extras::default();
        for widget in after {
            match widget {
                Widget::Tooltip { text } => extras.tooltip = Some(text.clone()),
                Widget::Name { text } => extras.name = Some(text.clone()),
                Widget::Focus { .. } => {}
                _ => break,
            }
        }
        extras
    }

    fn label(&self, shown: &str) -> Label {
        Label::from(self.name.clone().unwrap_or_else(|| shown.to_string()))
    }
}

pub(crate) trait Tip: Props + Sized {
    fn tip(self, extras: &Extras) -> Self {
        match &extras.tooltip {
            Some(text) => self.tooltip(text.clone()),
            None => self,
        }
    }
}

impl<T: Props> Tip for T {}

fn role(style: TextStyle) -> (TypeRole, Shade, Option<Tone>) {
    match style {
        TextStyle::Body => (TypeRole::Body, Shade::Normal, None),
        TextStyle::Heading => (TypeRole::Heading, Shade::Normal, None),
        TextStyle::Weak => (TypeRole::Body, Shade::Muted, None),
        TextStyle::Mono => (TypeRole::Mono, Shade::Normal, None),
        TextStyle::Error => (TypeRole::Body, Shade::Normal, Some(Tone::Bad)),
        TextStyle::Caption => (TypeRole::Caption, Shade::Muted, None),
        TextStyle::Strong => (TypeRole::BodyStrong, Shade::Normal, None),
        TextStyle::Title => (TypeRole::Title, Shade::Normal, None),
    }
}

pub(crate) fn text(text: &str, style: TextStyle) -> Text {
    let (type_role, shade, tone) = role(style);
    let shown = Text::new(text.to_string()).role(type_role).shade(shade);
    match tone {
        Some(tone) => shown.tone(tone),
        None => shown,
    }
}

impl Painter<'_> {
    pub(crate) fn all(&mut self, ui: &mut egui::Ui, widgets: &[Widget]) {
        for (at, widget) in widgets.iter().enumerate() {
            let extras = Extras::of(&widgets[at + 1..]);
            self.widget(ui, widget, &extras);
        }
    }

    pub(crate) fn seen(&mut self, ui: &egui::Ui, response: &egui::Response, keys: Keys) {
        self.last = Some(response.clone());
        if keys == Keys::Field {
            return;
        }
        let ctx = ui.ctx();
        let pressed: Vec<String> = self
            .host
            .keys()
            .iter()
            .filter(|key| {
                let Ok(chord) = craft_keymap::Chord::parse(key) else { return false };
                craft_ui::runtime::keys::claim(ctx, response.id, chord);
                craft_ui::runtime::keys::take_claimed(ctx, response.id, chord)
            })
            .cloned()
            .collect();
        for key in pressed {
            self.host.pressed(&key);
        }
    }

    pub(crate) fn emit(&mut self, id: u64, kind: UiEventKind) {
        self.events.push(UiEvent { id, kind });
    }

    pub(crate) fn salt(&self, id: u64) -> (String, u64) {
        (self.scope.to_string(), id)
    }

    fn widget(&mut self, ui: &mut egui::Ui, widget: &Widget, extras: &Extras) {
        match widget {
            Widget::Label { text: shown, style } => {
                let out = text(shown, *style).label(extras.label(shown)).tip(extras).show(ui);
                self.seen(ui, &out, Keys::Panel);
            }
            Widget::Text { spans } => {
                Row::new().wrap(true).show(ui, |ui, _| {
                    for span in spans {
                        text(&span.text, span.style).show(ui);
                    }
                });
            }
            Widget::Button { id, text, enabled } => {
                let out = Button::new(extras.label(text)).id_salt(self.salt(*id)).enabled(*enabled).tip(extras).show(ui);
                self.seen(ui, out.response(), Keys::Panel);
                if out.clicked() {
                    self.emit(*id, UiEventKind::Click);
                }
            }
            Widget::IconButton { id, icon, text, enabled } => {
                let out = IconButton::new(icons::spec(icon, text))
                    .label(extras.label(text))
                    .id_salt(self.salt(*id))
                    .enabled(*enabled)
                    .tooltip(extras.tooltip.clone().unwrap_or_else(|| text.clone()))
                    .show(ui);
                self.seen(ui, out.response(), Keys::Panel);
                if out.clicked() {
                    self.emit(*id, UiEventKind::Click);
                }
            }
            Widget::Checkbox { id, text, value } => {
                let out = Checkbox::new(Bind::given(*value)).label(extras.label(text)).id_salt(self.salt(*id)).tip(extras).show(ui);
                self.seen(ui, out.response(), Keys::Panel);
                if let Some(on) = changed(&out, value) {
                    self.emit(*id, UiEventKind::Bool(on));
                }
            }
            Widget::Toggle { id, text, value } => {
                let out = Switch::new(Bind::given(*value)).label(extras.label(text)).id_salt(self.salt(*id)).tip(extras).show(ui);
                self.seen(ui, out.response(), Keys::Panel);
                if let Some(on) = changed(&out, value) {
                    self.emit(*id, UiEventKind::Bool(on));
                }
            }
            Widget::Slider { id, text, value, min, max } => {
                let (low, high) = values::range(*min, *max);
                let out = SliderField::new(extras.label(text), Bind::given(values::finite(*value, low)), low..=high)
                    .id_salt(self.salt(*id))
                    .tip(extras)
                    .show(ui);
                self.seen(ui, out.response(), Keys::Field);
                if let Some(number) = changed(&out.edit, value) {
                    self.emit(*id, UiEventKind::Number(number));
                }
            }
            Widget::Number { id, text, value, speed, range, decimals } => {
                let given = Some(values::finite(*value, 0.0));
                let field = NumberField::new(extras.label(text), Bind::given(given)).id_salt(self.salt(*id)).tip(extras);
                let field = match values::step(*speed) {
                    Some(step) => field.step(step),
                    None => field,
                };
                let field = match decimals {
                    Some(decimals) => field.fixed_decimals(usize::from(*decimals)).commit_decimals(usize::from(*decimals)),
                    None => field,
                };
                let field = match range {
                    Some([low, high]) => {
                        let (low, high) = values::range(*low, *high);
                        field.range(low..=high)
                    }
                    None => field,
                };
                let out = field.show(ui);
                self.seen(ui, out.response(), Keys::Field);
                if let Some(Some(number)) = changed(&out.edit, &given) {
                    self.emit(*id, UiEventKind::Number(number));
                }
            }
            Widget::TextEdit { id, value, hint } => self.text_field(ui, *id, value, hint, 1, extras),
            Widget::TextArea { id, value, hint, rows } => self.text_field(ui, *id, value, hint, (*rows).max(1) as usize, extras),
            Widget::Popup { id, text, options, selected } => {
                let entries: Vec<Entry<u32>> = options.iter().zip(0_u32..).map(|(option, at)| Entry::new(at, option.clone())).collect();
                let out = Select::new(extras.label(text), Bind::given(*selected), &entries).id_salt(self.salt(*id)).tip(extras).show(ui);
                self.seen(ui, out.response(), Keys::Panel);
                if let Some(at) = changed(&out, selected) {
                    self.emit(*id, UiEventKind::Number(f64::from(at)));
                }
            }
            Widget::Radio { id, text, options, selected } => {
                let out = RadioGroup::new(extras.label(text), Bind::given(*selected)).id_salt(self.salt(*id)).show(ui, |ui, _| {
                    for (option, at) in options.iter().zip(0_u32..) {
                        Radio::new(at).label(option.clone()).show(ui);
                    }
                });
                self.seen(ui, out.output.response(), Keys::Panel);
                if let Some(at) = changed(&out.output, selected) {
                    self.emit(*id, UiEventKind::Number(f64::from(at)));
                }
            }
            Widget::Color { id, text, value } => self.color(ui, *id, text, *value, extras),
            Widget::Row { children } => {
                Row::new().show(ui, |ui, _| self.all(ui, children));
            }
            Widget::Column { children } => {
                Stack::new().show(ui, |ui, _| self.all(ui, children));
            }
            Widget::Group { title, children } => {
                Section::new(title.clone()).show(ui, |ui, _| self.all(ui, children));
            }
            Widget::Collapsing { id, title, open, children } => {
                Section::new(title.clone()).id_salt(self.salt(*id)).initially_open(*open).show(ui, |ui, _| self.all(ui, children));
            }
            Widget::Separator => {
                Divider::horizontal().show(ui);
            }
            Widget::Progress { fraction, text } => {
                Progress::new(Progress::clamped(*fraction)).label(text.clone()).tip(extras).show(ui);
            }
            Widget::Scroll { id, height, rows, children } => self.scroll(ui, *id, *height, *rows, children),
            Widget::Tabs { id, options, selected } => self.tabs(ui, *id, options, *selected),
            Widget::List { id, columns, count, first, items, selected, multi, rows, edit } => {
                let shown = list::Rows { columns, count: *count, first: *first, items, selected, multi: *multi, edit };
                match rows {
                    0 => self.list(ui, *id, &shown),
                    rows => {
                        Stack::new().room_height(values::rows_height(ui, *rows)).show(ui, |ui, _| self.list(ui, *id, &shown));
                    }
                }
            }
            Widget::Curve { id, text, points } => self.curve(ui, *id, text, points),
            Widget::Gradient { id, text, value } => self.gradient(ui, *id, text, value),
            Widget::Image { id, key, size } => self.image(ui, *id, key, *size, extras),
            Widget::Grid { id, columns, children } => {
                let widths = vec![craft_behavior::ColumnWidth::Content; (*columns).clamp(1, values::MAX_COLUMNS) as usize];
                Grid::new(widths).id_salt(self.salt(*id)).show(ui, |ui, _| self.all(ui, children));
            }
            Widget::Split { id, axis, first, second } => self.split(ui, *id, *axis, first, second),
            Widget::DropTarget { id, children } => self.drop_target(ui, *id, children),
            Widget::DragSource { id, drag, children } => self.drag_source(ui, *id, drag, children),
            Widget::Canvas { id, size, shapes } => self.canvas(ui, *id, *size, shapes, extras.label("Plug-in drawing")),
            Widget::Menu { id, text, items } => self.menu(ui, *id, text.as_deref(), items),
            Widget::Focus { call } => self.focus(ui, *call),
            Widget::Frame { id, request, size } => self.frame(ui, *id, request, *size),
            Widget::Gap { gap } => {
                ui.add_space(values::gap(ui, *gap));
            }
            Widget::Space { size } => {
                ui.add_space(values::nearest_gap(ui, *size));
            }
            Widget::Tooltip { .. } | Widget::Name { .. } => {}
        }
    }

    fn focus(&mut self, ui: &egui::Ui, call: u64) {
        let Some(last) = &self.last else { return };
        let memory = egui::Id::unique(("craft-plugin-ui-framework.focus", self.scope));
        let asked = craft_ui::runtime::memory::with_session(ui.ctx(), memory, |asked: &mut Option<u64>| asked.replace(call));
        if asked != Some(call) && !craft_ui::runtime::focus::request(ui.ctx(), craft_ui::runtime::focus::FocusTarget::Id(last.id)) {
            craft_ui::runtime::memory::with_session(ui.ctx(), memory, |stored: &mut Option<u64>| *stored = asked);
        }
    }

    fn text_field(&mut self, ui: &mut egui::Ui, id: u64, value: &String, hint: &str, lines: usize, extras: &Extras) {
        let field = TextField::new(extras.label(hint), Bind::given(value.clone())).placeholder(hint.to_string()).id_salt(self.salt(id)).tip(extras);
        let field = if lines > 1 { field.lines(lines) } else { field };
        let out = field.show(ui);
        self.seen(ui, out.response(), Keys::Field);
        if let Some(text) = changed(&out, value) {
            self.emit(id, UiEventKind::Text(text));
        }
    }

    fn scroll(&mut self, ui: &mut egui::Ui, id: u64, height: f32, rows: Option<u32>, children: &[Widget]) {
        let room = match rows {
            Some(rows) => values::rows_height(ui, rows),
            None => values::extent(height),
        };
        Stack::new().id_salt(self.salt(id)).scroll(craft_ui::contract::Scroll::vertical()).room_height(room).show(ui, |ui, _| self.all(ui, children));
    }

    fn tabs(&mut self, ui: &mut egui::Ui, id: u64, options: &[String], selected: u32) {
        let entries: Vec<Entry<u32>> = options.iter().zip(0_u32..).map(|(option, at)| Entry::new(at, option.clone())).collect();
        let out = craft_ui::choice::Segmented::new(Label::from(String::new()), Bind::given(selected), &entries).id_salt(self.salt(id)).show(ui);
        self.seen(ui, out.response(), Keys::Panel);
        if let Some(at) = changed(&out, &selected) {
            self.emit(id, UiEventKind::Number(f64::from(at)));
        }
    }

    fn color(&mut self, ui: &mut egui::Ui, id: u64, text: &str, value: [f32; 4], extras: &Extras) {
        let given = values::picked(value);
        let out = craft_color_ui::ui::ColorPopover::new(extras.label(text), Bind::given(given)).show(ui);
        self.seen(ui, out.response(), Keys::Panel);
        if let Some(picked) = out.events.value()
            && *picked != given
        {
            let color = values::linear(picked);
            if color != value {
                self.emit(id, UiEventKind::Color(color));
            }
        }
    }

    fn curve(&mut self, ui: &mut egui::Ui, id: u64, text: &str, points: &[[f64; 2]]) {
        let given = values::curve(points);
        let out = craft_imaging::ui::curve::CurveEditor::new(text.to_string(), Bind::given(given)).levels(values::CURVE_LEVELS).show(ui);
        self.seen(ui, out.response(), Keys::Panel);
        if let Some(curve) = out.edit.value() {
            let edited = values::points(curve);
            if edited != points {
                self.emit(id, UiEventKind::Points(edited));
            }
        }
    }

    fn gradient(&mut self, ui: &mut egui::Ui, id: u64, text: &str, value: &craft_plugin::Gradient) {
        let given = values::stops(value);
        let out = craft_color_ui::ui::gradient::GradientEditor::new(text.to_string(), Bind::given(given)).show(ui);
        self.seen(ui, out.response(), Keys::Panel);
        if let Some(stops) = out.edit.value() {
            let edited = values::gradient(stops);
            if edited != *value {
                self.emit(id, UiEventKind::Gradient(edited));
            }
        }
    }

    fn frame(&mut self, ui: &mut egui::Ui, id: u64, request: &FrameRequest, size: [f32; 2]) {
        let room = egui::Vec2::new(values::extent(size[0]), values::extent(size[1]));
        match self.host.frame(ui, request, room) {
            Some(response) => {
                if craft_ui::contract::clicked(&response) {
                    self.emit(id, UiEventKind::Click);
                }
                self.seen(ui, &response, Keys::Panel);
            }
            None => {
                let out = text("This app cannot show frames here", TextStyle::Weak).label(Label::from("Frame")).show(ui);
                self.seen(ui, &out, Keys::Panel);
            }
        }
    }

    fn image(&mut self, ui: &mut egui::Ui, id: u64, key: &str, size: [f32; 2], extras: &Extras) {
        let host = &*self.host;
        let make = || host.picture(key).unwrap_or_else(|| egui::ColorImage::new([1, 1], vec![Default::default()]));
        let revision = craft_ui::primitives::Revision::new(host.version(key));
        let room = egui::Vec2::new(values::extent(size[0]), values::extent(size[1]));
        let out = craft_ui::buttons::PictureButton::new(extras.label(key), values::key_hash(key), &make, room)
            .revision(revision)
            .id_salt(self.salt(id))
            .tip(extras)
            .show(ui);
        self.seen(ui, out.response(), Keys::Panel);
        if out.clicked() {
            self.emit(id, UiEventKind::Click);
        }
    }
}
