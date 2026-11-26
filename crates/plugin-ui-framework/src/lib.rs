#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod dock;
mod icons;
mod list;
mod values;

use craft_plugin::{TextStyle, UiEvent, UiEventKind, Widget};
use craft_theme::TypeRole;
use craft_theme::variant::Tone;
use craft_ui::buttons::{Button, IconButton};
use craft_ui::choice::{Checkbox, Radio, RadioGroup, Switch};
use craft_ui::contract::{Bind, Entry, Props, Show as _, ShowContainer as _};
use craft_ui::display::{Divider, Progress, Shade, Text};
use craft_ui::form::SliderField;
use craft_ui::layout_components::{Row, Stack};
use craft_ui::pickers::Select;
use craft_ui::runtime::env::Label;
use craft_ui::sections::Section;
use craft_ui::text::{NumberField, TextField};

pub use values::changed;

pub trait Host {
    fn picture(&mut self, _key: &str) -> Option<egui::ColorImage> {
        None
    }
}

impl Host for () {}

pub fn draw(ui: &mut egui::Ui, widgets: &[Widget], scope: &str, host: &mut dyn Host) -> Vec<UiEvent> {
    let mut painter = Painter { scope, host, events: Vec::new() };
    painter.all(ui, widgets);
    painter.events
}

pub(crate) struct Painter<'a> {
    pub(crate) scope: &'a str,
    host: &'a mut dyn Host,
    events: Vec<UiEvent>,
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

    pub(crate) fn emit(&mut self, id: u64, kind: UiEventKind) {
        self.events.push(UiEvent { id, kind });
    }

    pub(crate) fn salt(&self, id: u64) -> (String, u64) {
        (self.scope.to_string(), id)
    }

    fn widget(&mut self, ui: &mut egui::Ui, widget: &Widget, extras: &Extras) {
        match widget {
            Widget::Label { text: shown, style } => {
                text(shown, *style).label(extras.label(shown)).tip(extras).show(ui);
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
                if out.clicked() {
                    self.emit(*id, UiEventKind::Click);
                }
            }
            Widget::Checkbox { id, text, value } => {
                let out = Checkbox::new(Bind::given(*value)).label(extras.label(text)).id_salt(self.salt(*id)).tip(extras).show(ui);
                if let Some(on) = changed(&out, value) {
                    self.emit(*id, UiEventKind::Bool(on));
                }
            }
            Widget::Toggle { id, text, value } => {
                let out = Switch::new(Bind::given(*value)).label(extras.label(text)).id_salt(self.salt(*id)).tip(extras).show(ui);
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
                if let Some(number) = changed(&out.edit, value) {
                    self.emit(*id, UiEventKind::Number(number));
                }
            }
            Widget::Number { id, text, value, range, .. } => {
                let given = Some(values::finite(*value, 0.0));
                let field = NumberField::new(extras.label(text), Bind::given(given)).id_salt(self.salt(*id)).tip(extras);
                let field = match range {
                    Some([low, high]) => {
                        let (low, high) = values::range(*low, *high);
                        field.range(low..=high)
                    }
                    None => field,
                };
                let out = field.show(ui);
                if let Some(Some(number)) = changed(&out.edit, &given) {
                    self.emit(*id, UiEventKind::Number(number));
                }
            }
            Widget::TextEdit { id, value, hint } => self.text_field(ui, *id, value, hint, 1, extras),
            Widget::TextArea { id, value, hint, rows } => self.text_field(ui, *id, value, hint, (*rows).max(1) as usize, extras),
            Widget::Popup { id, text, options, selected } => {
                let entries: Vec<Entry<u32>> = options.iter().zip(0_u32..).map(|(option, at)| Entry::new(at, option.clone())).collect();
                let out = Select::new(extras.label(text), Bind::given(*selected), &entries).id_salt(self.salt(*id)).tip(extras).show(ui);
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
            Widget::List { id, columns, count, first, items, selected, multi, rows } => {
                let shown = list::Rows { columns, count: *count, first: *first, items, selected, multi: *multi };
                match rows {
                    0 => self.list(ui, *id, &shown),
                    rows => {
                        Stack::new().room_height(values::rows_height(ui, *rows)).show(ui, |ui, _| self.list(ui, *id, &shown));
                    }
                }
            }
            Widget::Curve { id, text, points } => self.curve(ui, *id, text, points),
            Widget::Gradient { id, text, value } => self.gradient(ui, *id, text, value),
            Widget::Image { id, key, .. } => self.image(ui, *id, key),
            Widget::Grid { children, .. } => {
                Stack::new().show(ui, |ui, _| self.all(ui, children));
            }
            Widget::Split { first, second, axis, .. } => match axis {
                craft_plugin::Axis::Horizontal => {
                    Row::new().show(ui, |ui, _| {
                        Stack::new().show(ui, |ui, _| self.all(ui, first));
                        Stack::new().show(ui, |ui, _| self.all(ui, second));
                    });
                }
                craft_plugin::Axis::Vertical => {
                    Stack::new().show(ui, |ui, _| {
                        self.all(ui, first);
                        Divider::horizontal().show(ui);
                        self.all(ui, second);
                    });
                }
            },
            Widget::DropTarget { children, .. } | Widget::DragSource { children, .. } => {
                Stack::new().show(ui, |ui, _| self.all(ui, children));
            }
            Widget::Canvas { .. } | Widget::Menu { .. } => {
                text("This app does not draw this plug-in widget yet", TextStyle::Weak).show(ui);
            }
            Widget::Space { .. } | Widget::Gap { .. } | Widget::Tooltip { .. } | Widget::Name { .. } | Widget::Focus { .. } => {}
        }
    }

    fn text_field(&mut self, ui: &mut egui::Ui, id: u64, value: &String, hint: &str, lines: usize, extras: &Extras) {
        let field = TextField::new(extras.label(hint), Bind::given(value.clone())).placeholder(hint.to_string()).id_salt(self.salt(id)).tip(extras);
        let field = if lines > 1 { field.lines(lines) } else { field };
        let out = field.show(ui);
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
        if let Some(at) = changed(&out, &selected) {
            self.emit(id, UiEventKind::Number(f64::from(at)));
        }
    }

    fn color(&mut self, ui: &mut egui::Ui, id: u64, text: &str, value: [f32; 4], extras: &Extras) {
        let given = values::picked(value);
        let out = craft_color_ui::ui::ColorPopover::new(extras.label(text), Bind::given(given)).show(ui);
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
        if let Some(stops) = out.edit.value() {
            let edited = values::gradient(stops);
            if edited != *value {
                self.emit(id, UiEventKind::Gradient(edited));
            }
        }
    }

    fn image(&mut self, ui: &mut egui::Ui, id: u64, key: &str) {
        let picture = self.host.picture(key);
        let make = || picture.clone().unwrap_or_else(|| egui::ColorImage::new([1, 1], vec![Default::default()]));
        let out = craft_ui::display::Picture::new(key.to_string(), values::key_hash(key), &make).id_salt(self.salt(id)).show(ui);
        if craft_ui::contract::clicked(&out) {
            self.emit(id, UiEventKind::Click);
        }
    }
}
