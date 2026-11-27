use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use crate::gradient::Gradient;
use crate::kernels::{BlendMode, BlurShape, Dab, Encoding, Filter, HostKernel, Measure, Negatives, Taps};
use crate::viewer::{ToolEvent, ToolSpec};

pub const ABI: u32 = 1;
pub const PIXEL_BYTES: usize = 16;

pub type Pixel = [f32; 4];

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "call", rename_all = "snake_case")]
pub enum Call {
    Describe,
    Layout { effect: String, cx: RenderCx },
    Render { effect: String, cx: RenderCx, frame: FrameDesc },
    Input { effect: String, cx: RenderCx, out: Rect },
    Pass { effect: String, cx: RenderCx, kernel: String, f: Vec<f32>, u: Vec<u32>, src: Window, aux: Option<Window>, out: Window, spread: bool },
    Panel { panel: String, input: UiInput },
    Command { command: String },
    Task { task: String, args: serde_json::Value },
    Controls { effect: String, param: String, instance: u64, value: String, input: UiInput },
    Overlay { effect: String, cx: RenderCx },
    Drag { effect: String, cx: RenderCx, key: String, to: [f64; 2], value: String },
    Tool { tool: String, event: ToolEvent },
    Button { effect: String, param: String, instance: u64, layer: u64 },
    Migrate { effect: String, from: u32, values: BTreeMap<String, Value> },
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ControlsReply {
    pub widgets: Vec<Widget>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub topics: Vec<Topic>,
    #[serde(default)]
    pub timers: Vec<Timer>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Outcome<T> {
    Ok(T),
    Err(String),
}

impl<T> From<Result<T, String>> for Outcome<T> {
    fn from(result: Result<T, String>) -> Self {
        match result {
            Ok(value) => Outcome::Ok(value),
            Err(message) => Outcome::Err(message),
        }
    }
}

impl<T> From<Outcome<T>> for Result<T, String> {
    fn from(outcome: Outcome<T>) -> Self {
        match outcome {
            Outcome::Ok(value) => Ok(value),
            Outcome::Err(message) => Err(message),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Contributions {
    #[serde(default)]
    pub effects: Vec<EffectSpec>,
    #[serde(default)]
    pub panels: Vec<PanelSpec>,
    #[serde(default)]
    pub commands: Vec<CommandSpec>,
    #[serde(default)]
    pub tasks: Vec<TaskSpec>,
    #[serde(default)]
    pub tools: Vec<ToolSpec>,
    #[serde(default)]
    pub separate_input: bool,
    #[serde(default)]
    pub bounded: bool,
    #[serde(default)]
    pub ui_level: u32,
}

pub const UI_LEVEL: u32 = 2;
pub const PANEL_MENU: u64 = u64::MAX;

impl Contributions {
    pub fn event_level(&self) -> u32 {
        self.ui_level.max(1)
    }

    pub fn check(&self) -> Result<(), String> {
        self.effects.iter().try_for_each(|effect| effect.check_bounded(self.bounded))?;
        self.tools.iter().try_for_each(|tool| self.check_options(&tool.id, tool.options.as_deref()))?;
        self.panels.iter().try_for_each(PanelSpec::check)
    }

    fn check_options(&self, tool: &str, options: Option<&str>) -> Result<(), String> {
        let Some(options) = options else { return Ok(()) };
        match self.panels.iter().find(|panel| panel.id == options) {
            None => Err(format!(
                "tool `{tool}` names options panel `{options}`, but the plugin has no panel `{options}`; declare it with .panel(\"{options}\", ..).place(PanelPlace::Bar)"
            )),
            Some(panel) if panel.place != PanelPlace::Bar => Err(format!(
                "tool `{tool}` names options panel `{options}`, which is not an option bar; add .place(PanelPlace::Bar) after .panel(\"{options}\", ..)"
            )),
            Some(_) => Ok(()),
        }
    }

    pub fn warnings(&self) -> Vec<String> {
        let panels = self.panels.iter().map(|panel| ("panel", &panel.id, &panel.icon));
        let commands = self.commands.iter().map(|command| ("command", &command.id, &command.icon));
        let tools = self.tools.iter().map(|tool| ("tool", &tool.id, &tool.named_icon));
        panels
            .chain(commands)
            .chain(tools)
            .filter_map(|(kind, id, icon)| icon.as_ref().filter(|icon| !ICONS.contains(&icon.as_str())).map(|icon| (kind, id, icon)))
            .map(|(kind, id, icon)| {
                format!("{kind} `{id}` names icon `{icon}`, which is not in craft_plugin::ICONS, so the app draws a letter; pick a name from ICONS")
            })
            .collect()
    }
}

const KEY_NAMES: &[&str] = &[
    "Up",
    "Down",
    "Left",
    "Right",
    "ArrowUp",
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
    "Escape",
    "Tab",
    "Backspace",
    "Enter",
    "Insert",
    "Delete",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "Space",
    ";",
    "'",
    ",",
    ".",
    "/",
    "\\",
    "=",
    "-",
    "`",
    "[",
    "]",
    "+",
];

pub fn is_chord(text: &str) -> bool {
    let (head, key) = match text.strip_suffix("++") {
        Some(head) => (head, "+"),
        None if text == "+" => ("", "+"),
        None => text.rsplit_once('+').unwrap_or(("", text)),
    };
    let function = key.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()).is_some_and(|n| (1..=35).contains(&n));
    let single = key.len() == 1 && key.bytes().all(|b| b.is_ascii_alphanumeric());
    let modifiers = head.is_empty() || head.split('+').all(|modifier| matches!(modifier, "Cmd" | "Ctrl" | "Shift" | "Alt"));
    modifiers && (function || single || KEY_NAMES.contains(&key))
}

impl PanelSpec {
    fn check(&self) -> Result<(), String> {
        if let Some(key) = self.keys.iter().find(|key| !is_chord(key)) {
            return Err(format!(
                "panel `{}` takes key `{key}`, which the app cannot read; write the modifiers Cmd, Ctrl, Shift or Alt and one key, joined by +, like \"Delete\" or \"Cmd+Shift+D\"",
                self.id
            ));
        }
        match &self.help {
            Some(help) if !help.starts_with("https://") => {
                Err(format!("panel `{}` has help `{help}`; .help(..) takes only an https:// address", self.id))
            }
            _ => Ok(()),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct EffectSpec {
    pub id: String,
    pub name: String,
    pub category: String,
    #[serde(default)]
    pub params: Vec<Param>,
    #[serde(default)]
    pub time_dependent: bool,
    #[serde(default)]
    pub reads_other_frames: bool,
    #[serde(default)]
    pub reads_scene: bool,
    #[serde(default)]
    pub shader: Option<String>,
    #[serde(default)]
    pub designer: Option<String>,
    #[serde(default)]
    pub reads_param_history: bool,
    #[serde(default)]
    pub presets: Vec<Preset>,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub reads_aux: Vec<Channel>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Depth,
    Normal,
    Uv,
    ObjectId,
    MaterialId,
    Coverage,
    Background,
    Named(String),
}

pub const MAX_AUX_BYTES: usize = 256 << 20;
pub const AUX_BACKGROUND_DEPTH: f32 = 1.0e10;

impl Channel {
    pub fn components(&self) -> usize {
        match self {
            Channel::Normal | Channel::Background => 3,
            Channel::Uv => 2,
            _ => 1,
        }
    }

    pub fn unique(channels: &[Channel]) -> Vec<Channel> {
        let mut unique: Vec<Channel> = Vec::with_capacity(channels.len());
        for channel in channels {
            if !unique.contains(channel) {
                unique.push(channel.clone());
            }
        }
        unique
    }

    pub fn read_bytes(width: u32, rows: u32, channels: &[Channel]) -> Result<usize, String> {
        let components: usize = Channel::unique(channels).iter().map(Channel::components).sum();
        let bytes =
            (width as usize).checked_mul(rows as usize).and_then(|pixels| pixels.checked_mul(components)).and_then(|floats| floats.checked_mul(4));
        match bytes {
            Some(bytes) if bytes <= MAX_AUX_BYTES => Ok(bytes),
            _ => Err(format!(
                "reading {components} channel values for {rows} rows of {width} pixels is over the {} MiB a read may take; read fewer channels at a time, or read them in render, where each band reads only its own rows",
                MAX_AUX_BYTES >> 20
            )),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AuxDesc {
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub first_row: u32,
    pub channels: Vec<Channel>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Preset {
    pub name: String,
    pub values: BTreeMap<String, Value>,
}

impl EffectSpec {
    pub fn new(id: &str, name: &str, category: &str) -> Self {
        EffectSpec { id: id.into(), name: name.into(), category: category.into(), ..Default::default() }
    }

    pub fn version(mut self, version: u32) -> Self {
        self.version = version;
        self
    }

    pub fn param(mut self, param: Param) -> Self {
        self.params.push(param);
        self
    }

    pub fn params(mut self, params: impl IntoIterator<Item = Param>) -> Self {
        self.params.extend(params);
        self
    }

    pub fn group(mut self, group: &str, params: impl IntoIterator<Item = Param>) -> Self {
        self.params.extend(params.into_iter().map(|mut p| {
            p.id = format!("{group}/{}", p.id);
            p
        }));
        self
    }

    pub fn time_dependent(mut self) -> Self {
        self.time_dependent = true;
        self
    }

    pub fn reads_other_frames(mut self) -> Self {
        self.reads_other_frames = true;
        self
    }

    pub fn reads_scene(mut self) -> Self {
        self.reads_scene = true;
        self
    }

    pub fn reads_aux(mut self, channels: impl IntoIterator<Item = Channel>) -> Self {
        self.reads_aux.extend(channels);
        self
    }

    pub fn shader(mut self, wgsl: &str) -> Self {
        self.shader = Some(wgsl.into());
        self
    }

    pub fn designer(mut self, panel: &str) -> Self {
        self.designer = Some(panel.into());
        self
    }

    pub fn reads_param_history(mut self) -> Self {
        self.reads_param_history = true;
        self
    }

    pub fn preset<'a>(mut self, name: &str, values: impl IntoIterator<Item = (&'a str, Value)>) -> Self {
        let values = values.into_iter().map(|(id, value)| (id.to_string(), value)).collect();
        self.presets.push(Preset { name: name.into(), values });
        self
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Param {
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub kind: ParamKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_when: Option<When>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled_when: Option<When>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct When {
    pub param: String,
    pub is: Is,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Is {
    Equals(f64),
    NotEquals(f64),
    On,
}

impl When {
    pub fn holds(&self, value: Option<&Value>) -> bool {
        match (self.is, value) {
            (Is::Equals(wanted), Some(Value::Number(n))) => *n == wanted,
            (Is::NotEquals(wanted), Some(Value::Number(n))) => *n != wanted,
            (Is::On, Some(Value::Bool(on))) => *on,
            _ => false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParamKind {
    Slider {
        default: f64,
        min: f64,
        max: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        soft: Option<[f64; 2]>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        log: bool,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        percent: bool,
    },
    Percent {
        default: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        soft: Option<[f64; 2]>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        log: bool,
    },
    Angle {
        default: f64,
    },
    Checkbox {
        default: bool,
    },
    Popup {
        options: Vec<String>,
        default: u32,
    },
    Point {
        default: [f64; 2],
    },
    Point3 {
        default: [f64; 3],
    },
    Color {
        default: [f64; 4],
    },
    Text {
        default: String,
    },
    Custom {
        default: String,
    },
    Layer,
    Mask,
    Gradient {
        default: Gradient,
    },
    Curve {
        default: Vec<[f64; 2]>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        channels: bool,
    },
    Button,
}

impl Param {
    fn new(id: &str, name: &str, kind: ParamKind) -> Self {
        Param { id: id.into(), name: name.into(), kind, visible_when: None, enabled_when: None }
    }

    pub fn button(id: &str, name: &str) -> Self {
        Param::new(id, name, ParamKind::Button)
    }

    pub fn visible_when(mut self, param: &str, is: Is) -> Self {
        self.visible_when = Some(When { param: param.into(), is });
        self
    }

    pub fn enabled_when(mut self, param: &str, is: Is) -> Self {
        self.enabled_when = Some(When { param: param.into(), is });
        self
    }

    pub(crate) fn check_conditions(&self, noun: &str, siblings: &[Param]) -> Result<(), String> {
        for when in self.visible_when.iter().chain(&self.enabled_when) {
            let fits = |kind: &ParamKind| match when.is {
                Is::On => matches!(kind, ParamKind::Checkbox { .. }),
                Is::Equals(_) | Is::NotEquals(_) => {
                    matches!(kind, ParamKind::Slider { .. } | ParamKind::Percent { .. } | ParamKind::Angle { .. } | ParamKind::Popup { .. })
                }
            };
            if !siblings.iter().any(|other| other.id == when.param && other.id != self.id && fits(&other.kind)) {
                return Err(format!(
                    "{noun} `{}` depends on `{}`, which is not another of its {noun}s of a kind that fits {:?}; name a checkbox for `Is::On` or a number or popup for `Is::Equals`",
                    self.id, when.param, when.is
                ));
            }
        }
        Ok(())
    }

    pub fn visible(&self, value: impl Fn(&str) -> Option<Value>) -> bool {
        self.visible_when.as_ref().is_none_or(|when| when.holds(value(&when.param).as_ref()))
    }

    pub fn enabled(&self, value: impl Fn(&str) -> Option<Value>) -> bool {
        self.enabled_when.as_ref().is_none_or(|when| when.holds(value(&when.param).as_ref()))
    }

    pub fn slider(id: &str, name: &str, default: f64, range: RangeInclusive<f64>) -> Self {
        Param::new(id, name, ParamKind::Slider { default, min: *range.start(), max: *range.end(), soft: None, log: false, percent: false })
    }

    pub fn percent(id: &str, name: &str, default: f64) -> Self {
        Param::new(id, name, ParamKind::Percent { default, soft: None, log: false })
    }

    pub fn range(mut self, range: RangeInclusive<f64>) -> Self {
        let (min, max) = (*range.start(), *range.end());
        self.kind = match self.kind {
            ParamKind::Percent { default, soft, log } => ParamKind::Slider { default, min, max, soft, log, percent: true },
            ParamKind::Slider { default, soft, log, percent, .. } => ParamKind::Slider { default, min, max, soft, log, percent },
            other => other,
        };
        self
    }

    pub fn soft(mut self, range: RangeInclusive<f64>) -> Self {
        if let ParamKind::Slider { soft, .. } | ParamKind::Percent { soft, .. } = &mut self.kind {
            *soft = Some([*range.start(), *range.end()]);
        }
        self
    }

    pub fn log(mut self) -> Self {
        if let ParamKind::Slider { log, .. } | ParamKind::Percent { log, .. } = &mut self.kind {
            *log = true;
        }
        self
    }

    pub fn angle(id: &str, name: &str, default: f64) -> Self {
        Param::new(id, name, ParamKind::Angle { default })
    }

    pub fn checkbox(id: &str, name: &str, default: bool) -> Self {
        Param::new(id, name, ParamKind::Checkbox { default })
    }

    pub fn popup(id: &str, name: &str, options: &[&str], default: u32) -> Self {
        let options = options.iter().map(|o| o.to_string()).collect();
        Param::new(id, name, ParamKind::Popup { options, default })
    }

    pub fn point(id: &str, name: &str, default: [f64; 2]) -> Self {
        Param::new(id, name, ParamKind::Point { default })
    }

    pub fn point3(id: &str, name: &str, default: [f64; 3]) -> Self {
        Param::new(id, name, ParamKind::Point3 { default })
    }

    pub fn color(id: &str, name: &str, default: [f64; 4]) -> Self {
        Param::new(id, name, ParamKind::Color { default })
    }

    pub fn text(id: &str, name: &str, default: &str) -> Self {
        Param::new(id, name, ParamKind::Text { default: default.into() })
    }

    pub fn custom(id: &str, name: &str, default: &str) -> Self {
        Param::new(id, name, ParamKind::Custom { default: default.into() })
    }

    pub fn layer(id: &str, name: &str) -> Self {
        Param::new(id, name, ParamKind::Layer)
    }

    pub fn mask(id: &str, name: &str) -> Self {
        Param::new(id, name, ParamKind::Mask)
    }

    pub fn gradient(id: &str, name: &str, colors: &[(f64, [f64; 4])]) -> Self {
        Param::new(id, name, ParamKind::Gradient { default: Gradient::new(colors) })
    }

    pub fn curve(id: &str, name: &str, default: &[[f64; 2]]) -> Self {
        Param::new(id, name, ParamKind::Curve { default: default.to_vec(), channels: false })
    }

    pub fn channels(mut self) -> Self {
        if let ParamKind::Curve { channels, .. } = &mut self.kind {
            *channels = true;
        }
        self
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Number(f64),
    List(Vec<f64>),
    Text(String),
    Gradient(Gradient),
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct RenderCx {
    pub params: BTreeMap<String, Value>,
    pub time: f64,
    pub comp_time: f64,
    pub frame_rate: f64,
    pub scale: f64,
    pub layer_size: [f64; 2],
    pub offset: [f64; 2],
    pub size: [u32; 2],
    pub instance: u64,
    #[serde(default)]
    pub float: bool,
    #[serde(default)]
    pub linear: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameDesc {
    pub width: u32,
    pub height: u32,
    pub origin: [u32; 2],
    #[serde(default)]
    pub input: Option<Window>,
    #[serde(default)]
    pub spread: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn of_size(size: [u32; 2]) -> Rect {
        let side = |n: u32| i32::try_from(n).unwrap_or(i32::MAX);
        Rect { left: 0, top: 0, right: side(size[0]), bottom: side(size[1]) }
    }

    pub fn width(&self) -> u32 {
        span(self.left, self.right)
    }

    pub fn height(&self) -> u32 {
        span(self.top, self.bottom)
    }

    pub fn is_empty(&self) -> bool {
        self.width() == 0 || self.height() == 0
    }

    pub fn grown(self, by: u32) -> Rect {
        let by = i32::try_from(by).unwrap_or(i32::MAX);
        Rect {
            left: self.left.saturating_sub(by),
            top: self.top.saturating_sub(by),
            right: self.right.saturating_add(by),
            bottom: self.bottom.saturating_add(by),
        }
    }

    pub fn clipped(self, size: [u32; 2]) -> Rect {
        let frame = Rect::of_size(size);
        let left = self.left.clamp(0, frame.right);
        let top = self.top.clamp(0, frame.bottom);
        Rect { left, top, right: self.right.clamp(left, frame.right), bottom: self.bottom.clamp(top, frame.bottom) }
    }

    pub fn union(self, other: Rect) -> Rect {
        Rect {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }

    pub fn intersection(self, other: Rect) -> Rect {
        let (left, top) = (self.left.max(other.left), self.top.max(other.top));
        Rect { left, top, right: self.right.min(other.right).max(left), bottom: self.bottom.min(other.bottom).max(top) }
    }

    pub fn contains(&self, other: Rect) -> bool {
        other.left >= self.left && other.top >= self.top && other.right <= self.right && other.bottom <= self.bottom
    }
}

fn span(from: i32, to: i32) -> u32 {
    u32::try_from((i64::from(to) - i64::from(from)).max(0)).unwrap_or(u32::MAX)
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rendered {
    pub drawn: Option<Rect>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Window {
    pub size: [u32; 2],
    pub first_row: u32,
    pub rows: u32,
}

impl Window {
    pub fn pixels(&self) -> usize {
        (self.size[0] as usize).saturating_mul(self.rows as usize)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Layout {
    #[serde(default, deserialize_with = "four_sides")]
    pub padding: [u32; 4],
    #[serde(default)]
    pub reach: Option<u32>,
    #[serde(default)]
    pub gpu: Option<Plan>,
    #[serde(default)]
    pub cpu: bool,
    #[serde(default)]
    pub maps_input: bool,
    #[serde(default)]
    pub ignores_input: bool,
}

fn four_sides<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<[u32; 4], D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Sides {
        All(u32),
        Each([u32; 4]),
    }
    Ok(match Sides::deserialize(deserializer)? {
        Sides::All(n) => [n; 4],
        Sides::Each(sides) => sides,
    })
}

impl Layout {
    pub fn whole_frame() -> Self {
        Layout::default()
    }

    pub fn pointwise() -> Self {
        Layout { reach: Some(0), ..Default::default() }
    }

    pub fn neighbourhood(reach: u32) -> Self {
        Layout { reach: Some(reach), ..Default::default() }
    }

    pub fn padded(mut self, padding: u32) -> Self {
        self.padding = [padding; 4];
        self
    }

    pub fn padded_sides(mut self, left: u32, top: u32, right: u32, bottom: u32) -> Self {
        self.padding = [left, top, right, bottom];
        self
    }

    pub fn maps_input(mut self) -> Self {
        self.maps_input = true;
        self
    }

    pub fn ignores_input(mut self) -> Self {
        self.ignores_input = true;
        self
    }

    pub fn gpu(mut self, plan: Plan) -> Self {
        self.gpu = Some(plan);
        self
    }

    pub fn passes(mut self, plan: Plan) -> Self {
        self.gpu = Some(plan);
        self.cpu = true;
        self
    }
}

pub const MAX_PADDING: u32 = 4096;
const MAX_PADDED_SIDE: f64 = 16384.0;
const MIN_PIXEL_BUDGET: f64 = 16_777_216.0;

pub fn applied_padding(requested: [u32; 4], size: [u32; 2]) -> [u32; 4] {
    let sides = requested.map(|side| f64::from(side.min(MAX_PADDING)));
    let [left, top, right, bottom] = sides;
    let (w, h) = (f64::from(size[0]), f64::from(size[1]));
    let (across, down) = (left + right, top + bottom);
    let mut f = 1.0_f64;
    if across > 0.0 {
        f = f.min((MAX_PADDED_SIDE - w).max(0.0) / across);
    }
    if down > 0.0 {
        f = f.min((MAX_PADDED_SIDE - h).max(0.0) / down);
    }
    let budget = MIN_PIXEL_BUDGET.max(4.0 * w * h);
    if (w + across * f) * (h + down * f) > budget {
        let (a, b, c) = (across * down, w * down + h * across, w * h - budget);
        f = f.min(if a > 0.0 { (-b + (b * b - 4.0 * a * c).max(0.0).sqrt()) / (2.0 * a) } else { -c / b });
    }
    let f = if f.is_finite() { f.clamp(0.0, 1.0) } else { 0.0 };
    sides.map(|side| (side * f).floor() as u32)
}

pub const MAX_PASSES: usize = 128;
pub const MAX_SCRATCH_IMAGES: usize = 32;
pub const MAX_SCRATCH_FRAMES: u64 = 8;
const DRAW_PIXEL_BYTES: u64 = 8;
const DEPTH_PIXEL_BYTES: u64 = 4;
pub const MAX_PASS_FLOATS: usize = 96;
pub const MAX_PASS_UINTS: usize = 16;
pub const MAX_BUFFERS: usize = 64;
pub const MAX_TEXTURES: usize = 16;
pub const MAX_TEXELS: u64 = 1 << 24;
pub const MAX_SOURCE_FRAMES: usize = 4;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Plan {
    #[serde(default)]
    pub images: Vec<[u32; 2]>,
    #[serde(default)]
    pub buffers: Vec<PlanData>,
    #[serde(default)]
    pub textures: Vec<GpuTexture>,
    #[serde(default)]
    pub layers: Vec<String>,
    #[serde(default)]
    pub sources: Vec<f64>,
    pub passes: Vec<PlanPass>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PlanData {
    Cache(String),
    Floats(Vec<f32>),
    Indices(Vec<u32>),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GpuTexture {
    pub key: String,
    pub size: [u32; 2],
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlanPass {
    pub kernel: String,
    pub src: PlanImage,
    #[serde(default)]
    pub aux: Option<PlanImage>,
    pub out: PlanImage,
    #[serde(default)]
    pub f: Vec<f32>,
    #[serde(default)]
    pub u: Vec<u32>,
    #[serde(default)]
    pub draw: Option<Draw>,
    #[serde(default)]
    pub reach: Option<u32>,
    #[serde(default)]
    pub aux_reach: Option<u32>,
    #[serde(default)]
    pub host: Option<HostKernel>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Draw {
    pub vertices: u32,
    pub layout: Vec<u32>,
    #[serde(default)]
    pub indices: Option<u32>,
    #[serde(default)]
    pub instances: Option<u32>,
    #[serde(default)]
    pub instance_layout: Vec<u32>,
    #[serde(default)]
    pub count: Option<u32>,
    #[serde(default)]
    pub depth: bool,
    #[serde(default)]
    pub blend: Blend,
    #[serde(default)]
    pub cull: Cull,
    #[serde(default)]
    pub clear: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Blend {
    #[default]
    Replace,
    Over,
    Add,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Cull {
    #[default]
    None,
    Back,
    Front,
}

impl Plan {
    pub fn new() -> Self {
        Plan::default()
    }

    pub fn scratch(&mut self, size: [u32; 2]) -> PlanImage {
        self.images.push(size);
        PlanImage::Scratch(self.images.len().saturating_sub(1) as u32)
    }

    pub fn buffer(&mut self, data: PlanData) -> u32 {
        self.buffers.push(data);
        self.buffers.len().saturating_sub(1) as u32
    }

    pub fn texture(&mut self, key: &str, size: [u32; 2]) -> PlanImage {
        self.textures.push(GpuTexture { key: key.to_string(), size });
        PlanImage::Texture(self.textures.len().saturating_sub(1) as u32)
    }

    pub fn layer(&mut self, param: &str) -> PlanImage {
        self.layers.push(param.to_string());
        PlanImage::Layer(self.layers.len().saturating_sub(1) as u32)
    }

    pub fn source_at(&mut self, layer_time: f64) -> PlanImage {
        self.sources.push(layer_time);
        PlanImage::SourceAt(self.sources.len().saturating_sub(1) as u32)
    }

    pub fn pass(mut self, pass: PlanPass) -> Self {
        self.passes.push(pass);
        self
    }

    pub fn size_of(&self, image: PlanImage, frame: [u32; 2]) -> Option<[u32; 2]> {
        match image {
            PlanImage::Source | PlanImage::Output => Some(frame),
            PlanImage::Scratch(index) => self.images.get(index as usize).copied(),
            PlanImage::Texture(index) => self.textures.get(index as usize).map(|texture| texture.size),
            PlanImage::Layer(index) => ((index as usize) < self.layers.len()).then_some(frame),
            PlanImage::SourceAt(index) => ((index as usize) < self.sources.len()).then_some(frame),
        }
    }

    pub fn runs_on_cpu(&self) -> bool {
        self.buffers.is_empty() && self.textures.is_empty() && self.layers.is_empty() && self.passes.iter().all(|pass| pass.draw.is_none())
    }

    pub fn check(&self, frame: [u32; 2]) -> Result<(), String> {
        if !self.runs_on_cpu() {
            return Err("a plan with draw passes, buffers, textures or layers runs only on the GPU".into());
        }
        self.validate(frame)
    }

    fn scratch_bytes(&self, frame: [u32; 2]) -> u64 {
        let bytes = |[w, h]: [u32; 2], per_pixel: u64| (u64::from(w) * u64::from(h)).saturating_mul(per_pixel);
        let mut kept: Vec<([u32; 2], u64)> = self.images.iter().map(|size| (*size, PIXEL_BYTES as u64)).collect();
        let mut working: Vec<([u32; 2], u64, usize)> = Vec::new();
        let mut largest_intermediate = 0;
        for pass in &self.passes {
            let (Some(src), Some(out)) = (self.size_of(pass.src, frame), self.size_of(pass.out, frame)) else { continue };
            match (&pass.host, &pass.draw) {
                (Some(kernel), _) if kernel.needs_temporary() => working.push((out, PIXEL_BYTES as u64, 0)),
                (Some(HostKernel::Flow { levels, .. }), _) => working.extend(
                    crate::kernels::flow_levels(out, *levels).into_iter().flat_map(|level| [0, 1, 2].map(|nth| (level, PIXEL_BYTES as u64, nth))),
                ),
                (Some(HostKernel::Resample { .. }), _) => {
                    largest_intermediate = largest_intermediate.max(bytes([out[0], src[1]], PIXEL_BYTES as u64))
                }
                (None, Some(draw)) => {
                    working.extend(std::iter::once((out, DRAW_PIXEL_BYTES, 0)).chain(draw.depth.then_some((out, DEPTH_PIXEL_BYTES, 0))))
                }
                _ => {}
            }
        }
        working.sort_unstable();
        working.dedup();
        kept.extend(working.into_iter().map(|(size, per_pixel, _)| (size, per_pixel)));
        kept.iter().map(|&(size, per_pixel)| bytes(size, per_pixel)).fold(largest_intermediate, u64::saturating_add)
    }

    pub fn validate(&self, frame: [u32; 2]) -> Result<(), String> {
        let passes = self.passes.len();
        if !(1..=MAX_PASSES).contains(&passes) {
            return Err(format!("a plan has 1 to {MAX_PASSES} passes, this one has {passes}"));
        }
        if self.images.len() > MAX_SCRATCH_IMAGES {
            return Err(format!("a plan has at most {MAX_SCRATCH_IMAGES} scratch images, this one has {}", self.images.len()));
        }
        let [fw, fh] = frame;
        for (i, &[w, h]) in self.images.iter().enumerate() {
            if w == 0 || h == 0 || w > fw.saturating_mul(2) || h > fh.saturating_mul(2) {
                return Err(format!("scratch image {i} is {w}x{h}; it must be non-empty and at most twice the {fw}x{fh} padded frame"));
            }
        }
        let area = |[w, h]: [u32; 2]| u64::from(w) * u64::from(h);
        if self.scratch_bytes(frame) > MAX_SCRATCH_FRAMES.saturating_mul(area(frame)).saturating_mul(PIXEL_BYTES as u64) {
            return Err(format!(
                "the scratch images and the images the passes work in (blur temporaries, resample intermediates, flow levels, draw targets) together exceed {MAX_SCRATCH_FRAMES} times the {fw}x{fh} padded frame"
            ));
        }
        if self.buffers.len() > MAX_BUFFERS {
            return Err(format!("a plan has at most {MAX_BUFFERS} buffers, this one has {}", self.buffers.len()));
        }
        let pictures = self.textures.len() + self.layers.len();
        if pictures > MAX_TEXTURES {
            return Err(format!("a plan has at most {MAX_TEXTURES} textures and layers together, this one has {pictures}"));
        }
        if self.sources.len() > MAX_SOURCE_FRAMES {
            return Err(format!("a plan reads at most {MAX_SOURCE_FRAMES} source frames at other times, this one reads {}", self.sources.len()));
        }
        if let Some(time) = self.sources.iter().find(|time| !time.is_finite()) {
            return Err(format!("a source frame is asked at layer time {time}; give a finite time in seconds"));
        }
        if let Some((i, texture)) = self.textures.iter().enumerate().find(|(_, texture)| area(texture.size) == 0) {
            return Err(format!("texture {i} ({:?}) is {}x{}; it must be non-empty", texture.key, texture.size[0], texture.size[1]));
        }
        let texels =
            self.textures.iter().map(|texture| area(texture.size)).fold((self.layers.len() as u64).saturating_mul(area(frame)), u64::saturating_add);
        if texels > MAX_TEXELS {
            return Err(format!("the textures and layers hold {texels} pixels; the limit is {MAX_TEXELS}"));
        }
        let mut written = vec![PlanImage::Source];
        let mut depth_size = None;
        for (n, pass) in self.passes.iter().enumerate() {
            if pass.f.len() > MAX_PASS_FLOATS || pass.u.len() > MAX_PASS_UINTS {
                let (floats, uints) = (pass.f.len(), pass.u.len());
                return Err(format!("pass {n} has {floats} floats and {uints} uints; the limits are {MAX_PASS_FLOATS} and {MAX_PASS_UINTS}"));
            }
            if matches!(pass.out, PlanImage::Source | PlanImage::Texture(_) | PlanImage::Layer(_) | PlanImage::SourceAt(_)) {
                return Err(format!("pass {n} writes {:?}, which passes only read", pass.out));
            }
            for image in std::iter::once(pass.src).chain(pass.aux) {
                if self.size_of(image, frame).is_none() {
                    return Err(format!("pass {n} reads {image:?}, which the plan does not declare"));
                }
                if matches!(image, PlanImage::Output | PlanImage::Scratch(_)) && !written.contains(&image) {
                    return Err(format!("pass {n} reads {image:?} before a pass writes it"));
                }
                if image == pass.out && !pass.host.as_ref().is_some_and(HostKernel::pointwise) {
                    return Err(format!("pass {n} reads the image it writes"));
                }
            }
            let Some(out_size) = self.size_of(pass.out, frame) else {
                return Err(format!("pass {n} writes {:?}, which the plan does not declare", pass.out));
            };
            if let Some(kernel) = &pass.host {
                let size = |image| self.size_of(image, frame).unwrap_or_default();
                let refused = match pass.draw {
                    Some(_) => Err("a host kernel pass cannot also draw".to_string()),
                    None => kernel.check(size(pass.src), pass.aux.map(size), out_size),
                };
                refused.map_err(|why| format!("pass {n} ({}): {why}", kernel.name()))?;
            }
            if let Some(draw) = &pass.draw {
                let named = [Some(draw.vertices), draw.indices, draw.instances];
                if let Some(missing) = named.into_iter().flatten().find(|&buffer| buffer as usize >= self.buffers.len()) {
                    return Err(format!("pass {n} names buffer {missing}, which the plan does not declare"));
                }
                if draw.depth && *depth_size.get_or_insert(out_size) != out_size {
                    return Err(format!("pass {n} uses the depth buffer on an image of another size than an earlier pass"));
                }
            }
            written.push(pass.out);
        }
        if !written.contains(&PlanImage::Output) {
            return Err("no pass writes the output image".into());
        }
        Ok(())
    }
}

impl PlanPass {
    pub fn new(kernel: &str, src: PlanImage, out: PlanImage) -> Self {
        PlanPass {
            kernel: kernel.to_string(),
            src,
            aux: None,
            out,
            f: Vec::new(),
            u: Vec::new(),
            draw: None,
            reach: None,
            aux_reach: None,
            host: None,
        }
    }

    pub fn host(kernel: HostKernel, src: PlanImage, out: PlanImage) -> Self {
        let name = kernel.name();
        PlanPass { host: Some(kernel), ..PlanPass::new(name, src, out) }
    }

    pub fn blur(src: PlanImage, out: PlanImage, radius: [f32; 2]) -> Self {
        PlanPass::host(HostKernel::Blur { shape: BlurShape::Gaussian, radius }, src, out)
    }

    pub fn box_blur(src: PlanImage, out: PlanImage, radius: [f32; 2]) -> Self {
        PlanPass::host(HostKernel::Blur { shape: BlurShape::Box, radius }, src, out)
    }

    pub fn resample(src: PlanImage, out: PlanImage, filter: Filter) -> Self {
        PlanPass::host(HostKernel::Resample { filter, scale: None }, src, out)
    }

    pub fn blend_images(base: PlanImage, top: PlanImage, out: PlanImage, mode: BlendMode, amount: f32) -> Self {
        PlanPass::host(HostKernel::Blend { mode, amount }, base, out).aux(top)
    }

    pub fn curves(src: PlanImage, out: PlanImage, table: &[Pixel]) -> Self {
        PlanPass::host(HostKernel::Curves { table: table.to_vec() }, src, out)
    }

    pub fn convolve(src: PlanImage, out: PlanImage, taps: Taps) -> Self {
        PlanPass::host(HostKernel::Convolve { taps }, src, out)
    }

    pub fn warp(src: PlanImage, displacement: PlanImage, out: PlanImage, amount: [f32; 2]) -> Self {
        PlanPass::host(HostKernel::Warp { amount }, src, out).aux(displacement)
    }

    pub fn matrix(src: PlanImage, out: PlanImage, rows: [[f32; 4]; 4]) -> Self {
        PlanPass::host(HostKernel::Matrix { rows }, src, out)
    }

    pub fn to_linear(src: PlanImage, out: PlanImage) -> Self {
        PlanPass::host(HostKernel::ToLinear, src, out)
    }

    pub fn to_srgb(src: PlanImage, out: PlanImage) -> Self {
        PlanPass::host(HostKernel::ToSrgb, src, out)
    }

    pub fn transfer(src: PlanImage, out: PlanImage, to: Encoding, negatives: Negatives) -> Self {
        PlanPass::host(HostKernel::Transfer { to, negatives }, src, out)
    }

    pub fn brightness_curve(src: PlanImage, out: PlanImage, measure: Measure, curve: Vec<f32>, range: f32) -> Self {
        PlanPass::host(HostKernel::BrightnessCurve { measure, curve, range }, src, out)
    }

    pub fn dither(src: PlanImage, out: PlanImage, seed: u32, origin: [i64; 2]) -> Self {
        PlanPass::host(HostKernel::Dither { seed, origin }, src, out)
    }

    pub fn light_alpha(src: PlanImage, out: PlanImage) -> Self {
        PlanPass::host(HostKernel::LightAlpha, src, out)
    }

    pub fn dilate(src: PlanImage, out: PlanImage, radius: [u32; 2]) -> Self {
        PlanPass::host(HostKernel::Dilate { radius }, src, out)
    }

    pub fn erode(src: PlanImage, out: PlanImage, radius: [u32; 2]) -> Self {
        PlanPass::host(HostKernel::Erode { radius }, src, out)
    }

    pub fn directional_blur(src: PlanImage, out: PlanImage, angle: f32, length: f32) -> Self {
        PlanPass::host(HostKernel::DirectionalBlur { angle, length }, src, out)
    }

    pub fn bilateral(src: PlanImage, out: PlanImage, radius: u32, tolerance: f32) -> Self {
        PlanPass::host(HostKernel::Bilateral { radius, tolerance }, src, out)
    }

    pub fn noise(out: PlanImage, scale: f32, octaves: u32, seed: u32, origin: [i64; 2]) -> Self {
        PlanPass::host(HostKernel::Noise { scale, octaves, seed, origin }, PlanImage::Source, out)
    }

    pub fn hsl_range(src: PlanImage, out: PlanImage, hue: [f32; 2], saturation: [f32; 2], lightness: [f32; 2], softness: f32) -> Self {
        PlanPass::host(HostKernel::HslRange { hue, saturation, lightness, softness }, src, out)
    }

    pub fn splat(src: PlanImage, out: PlanImage, dabs: Vec<Dab>) -> Self {
        PlanPass::host(HostKernel::Splat { dabs }, src, out)
    }

    pub fn flow(from: PlanImage, to: PlanImage, out: PlanImage) -> Self {
        PlanPass::host(HostKernel::Flow { levels: 6, radius: 3, iterations: 2 }, from, out).aux(to)
    }

    pub fn draw(kernel: &str, out: PlanImage) -> Self {
        PlanPass { draw: Some(Draw::default()), ..PlanPass::new(kernel, PlanImage::Source, out) }
    }

    pub fn src(mut self, image: PlanImage) -> Self {
        self.src = image;
        self
    }

    pub fn aux(mut self, image: PlanImage) -> Self {
        self.aux = Some(image);
        self
    }

    pub fn floats(mut self, values: &[f32]) -> Self {
        self.f = values.to_vec();
        self
    }

    pub fn uints(mut self, values: &[u32]) -> Self {
        self.u = values.to_vec();
        self
    }

    fn drawing(mut self, set: impl FnOnce(&mut Draw)) -> Self {
        set(self.draw.get_or_insert_with(Draw::default));
        self
    }

    pub fn vertices(self, buffer: u32, layout: &[u32]) -> Self {
        self.drawing(|draw| (draw.vertices, draw.layout) = (buffer, layout.to_vec()))
    }

    pub fn indices(self, buffer: u32) -> Self {
        self.drawing(|draw| draw.indices = Some(buffer))
    }

    pub fn instances(self, buffer: u32, layout: &[u32]) -> Self {
        self.drawing(|draw| (draw.instances, draw.instance_layout) = (Some(buffer), layout.to_vec()))
    }

    pub fn count(self, count: u32) -> Self {
        self.drawing(|draw| draw.count = Some(count))
    }

    pub fn depth(self) -> Self {
        self.drawing(|draw| draw.depth = true)
    }

    pub fn blend(self, blend: Blend) -> Self {
        self.drawing(|draw| draw.blend = blend)
    }

    pub fn cull(self, cull: Cull) -> Self {
        self.drawing(|draw| draw.cull = cull)
    }

    pub fn clear(self) -> Self {
        self.drawing(|draw| draw.clear = true)
    }

    pub fn reach(mut self, rows: u32) -> Self {
        self.reach = Some(rows);
        self
    }

    pub fn aux_reach(mut self, rows: u32) -> Self {
        self.aux_reach = Some(rows);
        self
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanImage {
    Source,
    Output,
    Scratch(u32),
    Texture(u32),
    Layer(u32),
    SourceAt(u32),
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PanelSpec {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub place: PanelPlace,
    #[serde(default)]
    pub size: PanelSize,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PanelPlace {
    Bottom,
    Float,
    Status,
    Bar,
    #[default]
    #[serde(other)]
    Side,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PanelSize {
    Small,
    Large,
    #[default]
    #[serde(other)]
    Medium,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CommandSpec {
    pub id: String,
    pub title: String,
    pub shortcut: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<Context>,
    #[serde(default)]
    pub toolbar: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Context {
    Layers,
    Timeline,
    Viewer,
    #[serde(other)]
    Unknown,
}

pub const ICONS: &[&str] = &[
    "brush", "pen", "eraser", "wand", "sparkles", "sliders", "palette", "droplet", "sun", "image", "film", "music", "text", "shapes", "square",
    "circle", "star", "heart", "grid", "list", "layers", "folder", "file", "search", "settings", "info", "warning", "help", "play", "pause", "stop",
    "plus", "minus", "trash", "copy", "eye", "lock", "link", "clock", "chart", "camera", "crop", "move", "rotate", "zoom", "magnet", "flag", "tag",
    "bookmark", "bell", "terminal", "code", "puzzle", "refresh", "download", "upload", "check", "close",
];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TaskSpec {
    pub id: String,
    pub title: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    #[default]
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct TaskState {
    pub id: u64,
    pub task: String,
    pub status: TaskStatus,
    pub progress: f64,
    pub message: String,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct FrameRequest {
    #[serde(default)]
    pub comp: Option<String>,
    #[serde(default)]
    pub layer: Option<String>,
    pub time: f64,
    pub scale: f64,
    #[serde(default)]
    pub ids: bool,
    #[serde(default)]
    pub source: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AudioRequest {
    #[serde(default)]
    pub layer: Option<String>,
    pub start: f64,
    pub duration: f64,
    pub rate: u32,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct AudioDesc {
    pub channels: u32,
    pub rate: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PickFile {
    #[serde(default)]
    pub save: bool,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub extensions: Vec<String>,
}

pub const MAX_FORM_FIELDS: usize = 64;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Form {
    pub title: String,
    pub fields: Vec<Param>,
}

impl Form {
    pub fn new(title: &str) -> Self {
        Form { title: title.into(), fields: Vec::new() }
    }

    pub fn field(mut self, field: Param) -> Self {
        self.fields.push(field);
        self
    }

    pub fn check(&self) -> Result<(), String> {
        if self.fields.len() > MAX_FORM_FIELDS {
            return Err(format!("a form has at most {MAX_FORM_FIELDS} fields, not {}", self.fields.len()));
        }
        if let Some(field) = self.fields.iter().find(|field| {
            matches!(field.kind, ParamKind::Layer | ParamKind::Mask | ParamKind::Gradient { .. } | ParamKind::Curve { .. } | ParamKind::Button)
        }) {
            return Err(format!(
                "form field `{}` is a layer, mask, gradient, curve or button; a form takes numbers, checkboxes, popups, points, colours and text",
                field.id
            ));
        }
        if let Some((_, field)) = self.fields.iter().enumerate().find(|(at, field)| self.fields[..*at].iter().any(|before| before.id == field.id)) {
            return Err(format!("form field `{}` appears twice; give each field its own id", field.id));
        }
        self.fields
            .iter()
            .try_for_each(|field| field.check_conditions("field", &self.fields))
            .map_err(|error| format!("form `{}`: {error}", self.title))
    }

    pub fn defaults(&self) -> BTreeMap<String, Value> {
        self.fields.iter().map(|field| (field.id.clone(), field.default_value([1.0, 1.0]))).collect()
    }

    pub fn answer(&self, mut values: BTreeMap<String, Value>) -> Result<BTreeMap<String, Value>, String> {
        let defaults = self.defaults();
        self.fields
            .iter()
            .map(|field| match values.remove(&field.id) {
                Some(value) => field.fit(value).map(|value| (field.id.clone(), value)),
                None => Ok((field.id.clone(), defaults[&field.id].clone())),
            })
            .collect()
    }

    pub fn answer_over(&self, prior: &BTreeMap<String, Value>, proposed: BTreeMap<String, Value>) -> Result<BTreeMap<String, Value>, String> {
        let mut merged = prior.clone();
        merged.extend(proposed);
        let mut answered = self.answer(merged)?;
        let turned_off: Vec<&Param> = self.fields.iter().filter(|field| !field.enabled(|id| answered.get(id).cloned())).collect();
        for field in turned_off {
            if let Some(kept) = prior.get(&field.id) {
                answered.insert(field.id.clone(), kept.clone());
            }
        }
        Ok(answered)
    }
}

pub const MAX_DATA_KEYS: usize = 64;
pub const MAX_DATA_KEY: usize = 256;
pub const MAX_DATA_VALUE: usize = 256 << 10;
pub const MAX_DATA_PER_PLUGIN: usize = 4 << 20;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Place {
    Project,
    Layer(u64),
    OwnLayer,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "call", rename_all = "snake_case")]
pub enum HostCall {
    Log { level: LogLevel, message: String },
    Notify { message: String },
    Command { id: String, params: serde_json::Value },
    Layer { param: String, time: Option<f64> },
    Source { time: f64 },
    Masks,
    Scene,
    ReadFile { root: Root, path: String, error_kind: Option<bool> },
    WriteFile { root: Root, path: String },
    ListFiles { root: Root, path: String },
    CacheGet { key: String },
    CachePut { key: String },
    CacheRemove { key: String },
    Start { task: String, args: serde_json::Value },
    Task { id: u64 },
    Cancel { id: u64 },
    Progress { fraction: f64, message: String },
    Frame { request: FrameRequest },
    Audio { request: AudioRequest },
    PickFile { request: PickFile },
    UploadImage { key: String, width: u32, height: u32 },
    ParamsAt { time: f64 },
    CacheHas { key: String },
    RemoveImage { key: String },
    ClipboardRead,
    ClipboardWrite { text: String },
    Placement { param: String, time: Option<f64> },
    SceneAt { time: f64 },
    DataGet { place: Place, key: String },
    DataSet { place: Place, key: String, remove: bool },
    Aux { channels: Vec<Channel>, rows: Option<[u32; 2]>, size: Option<[u32; 2]> },
    Form { form: Form },
    ShowPanel { panel: String, instance: Option<String> },
    CommandState { command: String, enabled: bool, checked: Option<bool> },
    ClosePanel { panel: String, instance: Option<String> },
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ParamsAt {
    pub params: BTreeMap<String, Value>,
    #[serde(default)]
    pub history: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileErrorKind {
    NotFound,
    PermissionDenied,
    Other,
}

const NOT_FOUND: &str = "not found: ";
const DENIED: &str = "permission denied: ";

fn file_failure(path: &str, error: &std::io::Error) -> String {
    let prefix = match error.kind() {
        std::io::ErrorKind::NotFound => NOT_FOUND,
        std::io::ErrorKind::PermissionDenied => DENIED,
        _ => "",
    };
    format!("{prefix}{path}: {error}")
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FileError {
    pub kind: FileErrorKind,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ReadFailed {
    pub failed: FileError,
}

impl From<FileError> for String {
    fn from(error: FileError) -> String {
        error.message
    }
}

impl FileError {
    pub fn of(path: &str, error: &std::io::Error) -> Self {
        let kind = match error.kind() {
            std::io::ErrorKind::NotFound => FileErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied => FileErrorKind::PermissionDenied,
            _ => FileErrorKind::Other,
        };
        FileError { kind, message: file_failure(path, error) }
    }

    pub fn other(message: impl Into<String>) -> Self {
        FileError { kind: FileErrorKind::Other, message: message.into() }
    }

    pub fn from_reply(message: String) -> Self {
        let kind = if message.starts_with(NOT_FOUND) {
            FileErrorKind::NotFound
        } else if message.starts_with(DENIED) {
            FileErrorKind::PermissionDenied
        } else {
            FileErrorKind::Other
        };
        FileError { kind, message }
    }
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for FileError {}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Root {
    Bundle,
    Data,
    User,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct ImageDesc {
    pub width: u32,
    pub height: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct MaskPath {
    pub name: String,
    pub points: Vec<[f64; 2]>,
    pub closed: bool,
    pub inverted: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Scene {
    #[serde(default)]
    pub projection: Option<[[f64; 4]; 3]>,
    #[serde(default)]
    pub camera_layer: bool,
    #[serde(default)]
    pub camera: Option<Camera>,
    #[serde(default)]
    pub lights: Vec<Light>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Camera {
    pub view: [[f64; 4]; 4],
    pub position: [f64; 3],
    pub zoom: f64,
    #[serde(default)]
    pub focus: Option<Focus>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Focus {
    pub distance: f64,
    pub aperture: f64,
    pub blur: f64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct LayerPlacement {
    pub matrix: [[f64; 4]; 4],
    pub opacity: f64,
    pub size: [f64; 2],
}

impl LayerPlacement {
    pub fn apply(&self, point: [f64; 3]) -> [f64; 3] {
        let row = |r: [f64; 4]| r[0] * point[0] + r[1] * point[1] + r[2] * point[2] + r[3];
        let w = row(self.matrix[3]);
        let w = if w.abs() > 1e-12 { w } else { 1.0 };
        [row(self.matrix[0]) / w, row(self.matrix[1]) / w, row(self.matrix[2]) / w]
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Light {
    pub kind: LightKind,
    pub position: [f64; 3],
    pub direction: [f64; 3],
    pub colour: [f32; 3],
    pub intensity: f32,
    pub cone_angle: f32,
    pub feather: f32,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LightKind {
    Ambient,
    #[default]
    Point,
    Spot,
    Parallel,
}

impl Scene {
    pub const NEAR: f64 = 1.0;

    pub fn centred(layer_size: [f64; 2]) -> Scene {
        let zoom = (layer_size[0] * 50.0 / 36.0).max(1.0);
        let centre = [layer_size[0] / 2.0, layer_size[1] / 2.0];
        let projection = [[1.0, 0.0, centre[0] / zoom, 0.0], [0.0, 1.0, centre[1] / zoom, 0.0], [0.0, 0.0, 1.0 / zoom, 1.0]];
        let view = [[1.0, 0.0, 0.0, -centre[0]], [0.0, 1.0, 0.0, -centre[1]], [0.0, 0.0, 1.0, zoom], [0.0, 0.0, 0.0, 1.0]];
        let camera = Camera { view, position: [centre[0], centre[1], -zoom], zoom, focus: None };
        Scene { projection: Some(projection), camera_layer: false, camera: Some(camera), lights: Vec::new() }
    }

    pub fn project(&self, point: [f64; 3]) -> Option<[f64; 2]> {
        let m = self.projection?;
        let row = |r: [f64; 4]| r[0] * point[0] + r[1] * point[1] + r[2] * point[2] + r[3];
        let w = row(m[2]);
        (w > 1e-9).then(|| [row(m[0]) / w, row(m[1]) / w])
    }

    pub fn view_projection(&self, cx: &RenderCx) -> Option<[f32; 16]> {
        let [x, y, w] = self.projection?;
        let [width, height] = [f64::from(cx.size[0].max(1)), f64::from(cx.size[1].max(1))];
        let steepness = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
        let depth = match self.camera {
            Some(camera) if steepness <= w[3].abs() * 1e-12 => {
                std::array::from_fn(|i| w[3] * (if i == 3 { 0.5 } else { 0.0 } - camera.view[2][i] * 1e-6))
            }
            _ => [0.0, 0.0, 0.0, Self::NEAR * steepness],
        };
        let rows: [[f64; 4]; 4] = [
            std::array::from_fn(|i| 2.0 * (x[i] * cx.scale + cx.offset[0] * w[i]) / width - w[i]),
            std::array::from_fn(|i| w[i] - 2.0 * (y[i] * cx.scale + cx.offset[1] * w[i]) / height),
            depth,
            w,
        ];
        Some(std::array::from_fn(|i| rows[i % 4][i / 4] as f32))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct UiInput {
    #[serde(default)]
    pub events: Vec<UiEvent>,
    pub size: [f32; 2],
    #[serde(default)]
    pub keys: String,
    #[serde(default)]
    pub app: Vec<AppEvent>,
    #[serde(default)]
    pub timers: Vec<String>,
    #[serde(default)]
    pub views: Vec<View>,
    #[serde(default)]
    pub instance: Option<String>,
    #[serde(default)]
    pub call: u64,
    #[serde(default)]
    pub pressed: Vec<String>,
    #[serde(default)]
    pub closed: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct View {
    pub id: u64,
    pub first: u32,
    pub count: u32,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Topic {
    Selection,
    Time,
    Comp,
    Project,
    Edit,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "topic", rename_all = "snake_case")]
pub enum AppEvent {
    Selection { layers: Vec<u64>, properties: Vec<[u64; 2]>, keyframes: u32 },
    Time { comp: u64, time: f64 },
    Comp { comp: Option<u64> },
    Project { path: Option<String> },
    Edit { layers: Vec<u64> },
}

impl AppEvent {
    pub fn topic(&self) -> Topic {
        match self {
            AppEvent::Selection { .. } => Topic::Selection,
            AppEvent::Time { .. } => Topic::Time,
            AppEvent::Comp { .. } => Topic::Comp,
            AppEvent::Project { .. } => Topic::Project,
            AppEvent::Edit { .. } => Topic::Edit,
        }
    }
}

pub const MIN_TIMER_MS: u32 = 16;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Timer {
    pub name: String,
    pub ms: u32,
    #[serde(default)]
    pub repeat: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(from = "PanelWire")]
pub struct PanelReply {
    pub widgets: Vec<Widget>,
    #[serde(default)]
    pub topics: Vec<Topic>,
    #[serde(default)]
    pub timers: Vec<Timer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub menu: Vec<MenuItem>,
    #[serde(skip)]
    pub legacy: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PanelWire {
    Legacy(Vec<Widget>),
    Reply {
        widgets: Vec<Widget>,
        #[serde(default)]
        topics: Vec<Topic>,
        #[serde(default)]
        timers: Vec<Timer>,
        #[serde(default)]
        menu: Vec<MenuItem>,
    },
}

impl From<PanelWire> for PanelReply {
    fn from(wire: PanelWire) -> Self {
        match wire {
            PanelWire::Legacy(widgets) => PanelReply { widgets, legacy: true, ..Default::default() },
            PanelWire::Reply { widgets, topics, timers, menu } => PanelReply { widgets, topics, timers, menu, legacy: false },
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UiEvent {
    pub id: u64,
    pub kind: UiEventKind,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UiEventKind {
    Click,
    Bool(bool),
    Number(f64),
    Text(String),
    Color([f32; 4]),
    Pointer(Pointer),
    Key(Key),
    Drop(Dropped),
    List(ListEvent),
    Points(Vec<[f64; 2]>),
    Gradient(Gradient),
    Context(Pointer),
    Open(Pointer),
}

impl UiEventKind {
    pub fn ui_level(&self) -> u32 {
        match self {
            UiEventKind::Context(_) | UiEventKind::Open(_) => 2,
            _ => 1,
        }
    }

    pub fn known_to_every_sdk(&self) -> bool {
        matches!(
            self,
            UiEventKind::Click
                | UiEventKind::Bool(_)
                | UiEventKind::Number(_)
                | UiEventKind::Text(_)
                | UiEventKind::Color(_)
                | UiEventKind::Pointer(_)
        )
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ListEvent {
    Select(Vec<u32>),
    Activate(u32),
    Toggle { row: u32, open: bool },
    Show { first: u32, count: u32 },
    Rename { row: u32, text: String },
    Move { rows: Vec<u32>, to: u32 },
    Sort { column: u32, descending: bool },
}

pub const MAX_LIST_ITEMS: u32 = 1024;
pub const LIST_WINDOW: u32 = 128;
pub const MAX_SELECTED: u32 = 1 << 20;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Key {
    pub name: String,
    #[serde(default)]
    pub modifiers: Modifiers,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Dropped {
    Files { paths: Vec<String> },
    Items { ids: Vec<u64> },
    Data { plugin: String, data: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Drag {
    File { root: Root, path: String },
    Data { data: String },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Pointer {
    pub pos: [f32; 2],
    pub phase: PointerPhase,
    #[serde(default)]
    pub modifiers: Modifiers,
    #[serde(default)]
    pub scroll: [f32; 2],
}

impl Pointer {
    pub fn new(pos: [f32; 2], phase: PointerPhase) -> Self {
        Pointer { pos, phase, modifiers: Modifiers::default(), scroll: [0.0; 2] }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub command: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PointerPhase {
    Hover,
    Down,
    Drag,
    Up,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "w", rename_all = "snake_case")]
pub enum Widget {
    Label {
        text: String,
        style: TextStyle,
    },
    Button {
        id: u64,
        text: String,
        enabled: bool,
    },
    Checkbox {
        id: u64,
        text: String,
        value: bool,
    },
    Slider {
        id: u64,
        text: String,
        value: f64,
        min: f64,
        max: f64,
    },
    Number {
        id: u64,
        text: String,
        value: f64,
        speed: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        range: Option<[f64; 2]>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        decimals: Option<u8>,
    },
    TextEdit {
        id: u64,
        value: String,
        hint: String,
    },
    Popup {
        id: u64,
        text: String,
        options: Vec<String>,
        selected: u32,
    },
    Color {
        id: u64,
        text: String,
        value: [f32; 4],
    },
    Row {
        children: Vec<Widget>,
    },
    Column {
        children: Vec<Widget>,
    },
    Group {
        title: String,
        children: Vec<Widget>,
    },
    Separator,
    Space {
        size: f32,
    },
    Canvas {
        id: u64,
        size: [f32; 2],
        shapes: Vec<Shape>,
    },
    Image {
        id: u64,
        key: String,
        size: [f32; 2],
    },
    Scroll {
        id: u64,
        height: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rows: Option<u32>,
        children: Vec<Widget>,
    },
    Collapsing {
        id: u64,
        title: String,
        open: bool,
        children: Vec<Widget>,
    },
    Tabs {
        id: u64,
        options: Vec<String>,
        selected: u32,
    },
    Progress {
        fraction: f32,
        text: String,
    },
    Tooltip {
        text: String,
    },
    Grid {
        id: u64,
        columns: u32,
        children: Vec<Widget>,
    },
    DropTarget {
        id: u64,
        children: Vec<Widget>,
    },
    DragSource {
        id: u64,
        drag: Drag,
        children: Vec<Widget>,
    },
    List {
        id: u64,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        columns: Vec<Column>,
        count: u32,
        first: u32,
        items: Vec<Item>,
        selected: Vec<u32>,
        multi: bool,
        rows: u32,
        #[serde(default, skip_serializing_if = "ListEdit::is_off")]
        edit: ListEdit,
    },
    IconButton {
        id: u64,
        icon: String,
        text: String,
        enabled: bool,
    },
    Toggle {
        id: u64,
        text: String,
        value: bool,
    },
    Radio {
        id: u64,
        text: String,
        options: Vec<String>,
        selected: u32,
    },
    TextArea {
        id: u64,
        value: String,
        hint: String,
        rows: u32,
    },
    Text {
        spans: Vec<Span>,
    },
    Menu {
        id: u64,
        text: Option<String>,
        items: Vec<MenuItem>,
    },
    Split {
        id: u64,
        axis: Axis,
        first: Vec<Widget>,
        second: Vec<Widget>,
    },
    Curve {
        id: u64,
        text: String,
        points: Vec<[f64; 2]>,
    },
    Gradient {
        id: u64,
        text: String,
        value: Gradient,
    },
    Gap {
        gap: Gap,
    },
    Name {
        text: String,
    },
    Focus {
        call: u64,
    },
    Frame {
        id: u64,
        request: FrameRequest,
        size: [f32; 2],
    },
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListEdit {
    #[serde(default)]
    pub rename: bool,
    #[serde(default)]
    pub reorder: bool,
    #[serde(default)]
    pub sortable: bool,
    #[serde(default)]
    pub sorted: Option<(u32, bool)>,
}

impl ListEdit {
    pub fn is_off(&self) -> bool {
        *self == ListEdit::default()
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TextStyle {
    Heading,
    Weak,
    Mono,
    Error,
    Caption,
    Strong,
    Title,
    #[default]
    #[serde(other)]
    Body,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: TextStyle,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Column {
    pub title: String,
    #[serde(default)]
    pub width: ColumnWidth,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColumnWidth {
    Narrow,
    Wide,
    Fill,
    #[default]
    #[serde(other)]
    Auto,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Item {
    pub cells: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub depth: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<bool>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub weak: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

fn is_zero(depth: &u8) -> bool {
    *depth == 0
}

impl Item {
    pub fn new(text: &str) -> Item {
        Item { cells: vec![text.into()], ..Default::default() }
    }

    pub fn cells(cells: &[&str]) -> Item {
        Item { cells: cells.iter().map(|cell| cell.to_string()).collect(), ..Default::default() }
    }

    pub fn icon(mut self, icon: &str) -> Item {
        self.icon = Some(icon.into());
        self
    }

    pub fn depth(mut self, depth: u8) -> Item {
        self.depth = depth;
        self
    }

    pub fn open(mut self, open: bool) -> Item {
        self.open = Some(open);
        self
    }

    pub fn weak(mut self) -> Item {
        self.weak = true;
        self
    }

    pub fn key(mut self, key: &str) -> Item {
        self.key = Some(key.into());
        self
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct MenuItem {
    pub text: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub separator: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<MenuItem>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    Vertical,
    #[default]
    #[serde(other)]
    Horizontal,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Gap {
    Small,
    Large,
    #[default]
    #[serde(other)]
    Medium,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "s", rename_all = "snake_case")]
pub enum Shape {
    Line {
        points: Vec<[f32; 2]>,
        color: Paint,
        width: f32,
    },
    Bezier {
        points: [[f32; 2]; 4],
        color: Paint,
        width: f32,
    },
    Rect {
        min: [f32; 2],
        max: [f32; 2],
        fill: Option<Paint>,
        stroke: Option<(Paint, f32)>,
        radius: f32,
    },
    Circle {
        center: [f32; 2],
        radius: f32,
        fill: Option<Paint>,
        stroke: Option<(Paint, f32)>,
    },
    Text {
        pos: [f32; 2],
        text: String,
        color: Paint,
        size: f32,
    },
    Polygon {
        points: Vec<[f32; 2]>,
        fill: Paint,
    },
    Image {
        key: String,
        min: [f32; 2],
        max: [f32; 2],
    },
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Paint {
    Text,
    Weak,
    Accent,
    Background,
    Surface,
    Border,
    Error,
    Rgba([u8; 4]),
    Good,
    Warn,
    Faint,
    Selected,
    Series(u8),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contributions_and_a_reply_from_a_newer_plugin_read_their_unknown_names_as_the_defaults_and_keep_everything_else() {
        let contributions: Contributions = serde_json::from_str(
            r#"{"panels":[{"id":"clips","title":"Clips","place":"sidebar_left","size":"huge","keys":["Delete"]}],
                "commands":[{"id":"grade","title":"Grade","shortcut":null,"context":["node","layers"],"toolbar":true}],"ui_level":9}"#,
        )
        .unwrap();
        let panel = PanelSpec { id: "clips".into(), title: "Clips".into(), keys: vec!["Delete".into()], ..PanelSpec::default() };
        assert_eq!((contributions.panels, contributions.ui_level), (vec![panel], 9));
        let command = CommandSpec {
            id: "grade".into(),
            title: "Grade".into(),
            context: vec![Context::Unknown, Context::Layers],
            toolbar: true,
            ..CommandSpec::default()
        };
        assert_eq!(contributions.commands, vec![command]);
        let reply: PanelReply = serde_json::from_str(
            r#"{"widgets":[
                {"w":"label","text":"Takes","style":"glow"},
                {"w":"sparkline","id":3,"values":[1,2,3],"children":[{"w":"button","id":4,"text":"Lost","enabled":true}]},
                {"w":"row","children":[{"w":"gap","gap":"vast"},{"w":"button","id":5,"text":"Apply","enabled":true}]},
                {"w":"split","id":6,"axis":"diagonal","first":[],"second":[]},
                {"w":"list","id":7,"columns":[{"title":"Name","width":"golden"}],"count":0,"first":0,"items":[],"selected":[],"multi":false,"rows":4}
            ],"topics":["playhead","time"],"badge":"new"}"#,
        )
        .unwrap();
        assert_eq!(
            reply.widgets,
            vec![
                Widget::Label { text: "Takes".into(), style: TextStyle::Body },
                Widget::Unknown,
                Widget::Row { children: vec![Widget::Gap { gap: Gap::Medium }, Widget::Button { id: 5, text: "Apply".into(), enabled: true }] },
                Widget::Split { id: 6, axis: Axis::Horizontal, first: vec![], second: vec![] },
                Widget::List {
                    id: 7,
                    columns: vec![Column { title: "Name".into(), width: ColumnWidth::Auto }],
                    count: 0,
                    first: 0,
                    items: vec![],
                    selected: vec![],
                    multi: false,
                    rows: 4,
                    edit: ListEdit::default(),
                },
            ]
        );
        assert_eq!(reply.topics, vec![Topic::Unknown, Topic::Time]);
        let shapes: Vec<Shape> =
            serde_json::from_str(r#"[{"s":"arc","radius":3},{"s":"circle","center":[1,2],"radius":3,"fill":"accent","stroke":null}]"#).unwrap();
        assert_eq!(shapes, vec![Shape::Unknown, Shape::Circle { center: [1.0, 2.0], radius: 3.0, fill: Some(Paint::Accent), stroke: None }]);
    }

    #[test]
    fn a_form_field_condition_is_checked_like_an_effect_parameters() {
        let refused = |form: Form| form.check().unwrap_err();
        let missing = Form::new("Settings").field(Param::slider("amount", "Amount", 1.0, 0.0..=10.0).visible_when("missing", Is::On));
        assert_eq!(
            refused(missing),
            "form `Settings`: field `amount` depends on `missing`, which is not another of its fields of a kind that fits On; name a checkbox for `Is::On` or a number or popup for `Is::Equals`"
        );
        let wrong_kind = Form::new("Settings")
            .field(Param::slider("mode", "Mode", 1.0, 0.0..=10.0))
            .field(Param::slider("amount", "Amount", 1.0, 0.0..=10.0).enabled_when("mode", Is::On));
        assert_eq!(
            refused(wrong_kind),
            "form `Settings`: field `amount` depends on `mode`, which is not another of its fields of a kind that fits On; name a checkbox for `Is::On` or a number or popup for `Is::Equals`"
        );
        let itself = Form::new("Settings").field(Param::checkbox("manual", "Manual", false).visible_when("manual", Is::On));
        assert_eq!(
            refused(itself),
            "form `Settings`: field `manual` depends on `manual`, which is not another of its fields of a kind that fits On; name a checkbox for `Is::On` or a number or popup for `Is::Equals`"
        );
        let fine = Form::new("Settings")
            .field(Param::checkbox("manual", "Manual", false))
            .field(Param::slider("amount", "Amount", 1.0, 0.0..=10.0).visible_when("manual", Is::On));
        assert_eq!(fine.check(), Ok(()));
    }

    #[test]
    fn an_answer_keeps_the_prior_value_of_a_field_its_own_answer_turns_off() {
        let form = Form::new("Settings")
            .field(Param::checkbox("manual", "Manual", false))
            .field(Param::slider("amount", "Amount", 1.0, 0.0..=10.0).enabled_when("manual", Is::On));
        let prior = form.defaults();
        let number = |n| BTreeMap::from([("amount".to_string(), Value::Number(n))]);
        assert_eq!(form.answer_over(&prior, number(7.0)).unwrap()["amount"], Value::Number(1.0));
        let mut switched = number(7.0);
        switched.insert("manual".into(), Value::Bool(true));
        assert_eq!(form.answer_over(&prior, switched).unwrap()["amount"], Value::Number(7.0));
    }

    #[test]
    fn an_aux_read_counts_each_channel_once_and_refuses_more_than_its_budget() {
        let one = Channel::read_bytes(3840, 2160, &[Channel::Depth]).unwrap();
        assert_eq!(Channel::read_bytes(3840, 2160, &[Channel::Depth, Channel::Depth]), Ok(one));
        assert!(Channel::read_bytes(3840, 2160, &[Channel::Normal, Channel::Depth]).unwrap() > one);
        let refused = Channel::read_bytes(16384, 16384, &[Channel::Depth, Channel::Normal]).unwrap_err();
        assert!(refused.contains("read fewer channels"), "{refused}");
        assert!(Channel::read_bytes(u32::MAX, u32::MAX, &[Channel::Background]).is_err());
    }

    const FRAME: [u32; 2] = [53, 37];

    fn finish() -> PlanPass {
        PlanPass::new("tint", PlanImage::Source, PlanImage::Output)
    }

    fn plan(images: Vec<[u32; 2]>, passes: Vec<PlanPass>) -> Plan {
        Plan { images, passes, ..Default::default() }
    }

    #[test]
    fn source_frames_have_their_own_budget_so_two_dci_4k_frames_fit() {
        let dci = [4096, 2160];
        let mut two = plan(vec![dci], vec![finish()]);
        let (before, after) = (two.source_at(0.0), two.source_at(1.0));
        two.passes =
            vec![PlanPass::flow(before, after, PlanImage::Scratch(0)), PlanPass::warp(before, PlanImage::Scratch(0), PlanImage::Output, [0.5; 2])];
        assert_eq!(two.validate(dci), Ok(()));
        let mut with_layers = plan(vec![], vec![finish()]);
        for i in 0..MAX_TEXTURES {
            with_layers.layer(&format!("layer{i}"));
        }
        with_layers.source_at(0.0);
        assert!(with_layers.validate([16, 16]).is_ok(), "source frames do not count against textures and layers");
        let mut too_many = plan(vec![], vec![finish()]);
        for i in 0..=MAX_SOURCE_FRAMES {
            too_many.source_at(i as f64);
        }
        assert!(too_many.validate(dci).unwrap_err().contains("at most 4 source frames"));
    }

    #[test]
    fn a_read_asks_for_its_error_kind_as_a_field_and_a_read_from_an_older_sdk_does_not() {
        let asked = HostCall::ReadFile { root: Root::Data, path: "a".into(), error_kind: Some(true) };
        assert_eq!(serde_json::to_string(&asked).unwrap(), r#"{"call":"read_file","root":"data","path":"a","error_kind":true}"#);
        let older: HostCall = serde_json::from_str(r#"{"call":"read_file","root":"data","path":"a"}"#).unwrap();
        assert!(matches!(older, HostCall::ReadFile { error_kind: None, .. }));
        let failed = ReadFailed { failed: FileError { kind: FileErrorKind::NotFound, message: "a: gone".into() } };
        assert_eq!(serde_json::to_string(&failed).unwrap(), r#"{"failed":{"kind":"not_found","message":"a: gone"}}"#);
    }

    #[test]
    fn a_scene_from_an_older_host_has_no_focus_and_a_placement_moves_points() {
        let older = r#"{"camera":{"view":[[1,0,0,0],[0,1,0,0],[0,0,1,0],[0,0,0,1]],"position":[0,0,-50],"zoom":50},"lights":[{"kind":"spot","position":[1,2,3],"direction":[0,0,1],"colour":[1,1,1],"intensity":1,"cone_angle":90,"feather":0}]}"#;
        let scene: Scene = serde_json::from_str(older).unwrap();
        assert_eq!(scene.camera.unwrap().focus, None);
        let placed = LayerPlacement {
            matrix: [[0.0, -1.0, 0.0, 47.0], [1.0, 0.0, 0.0, 8.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]],
            opacity: 1.0,
            size: [20.0, 10.0],
        };
        assert_eq!(placed.apply([20.0, 10.0, 3.0]), [37.0, 28.0, 3.0]);
    }

    #[test]
    fn one_check_refuses_every_plan_beyond_the_limits() {
        let tint = |src, out| PlanPass::new("tint", src, out);
        let scratch = PlanImage::Scratch(0);
        let mut draw_without_buffer = plan(vec![], vec![PlanPass::draw("mesh", PlanImage::Output)]);
        draw_without_buffer.passes[0] = draw_without_buffer.passes[0].clone().vertices(0, &[3]);
        let mut depth_on_two_sizes = plan(vec![[8, 8]], vec![]);
        let mesh = depth_on_two_sizes.buffer(PlanData::Floats(vec![0.0; 9]));
        depth_on_two_sizes.passes = vec![
            PlanPass::draw("mesh", scratch).vertices(mesh, &[3]).depth(),
            PlanPass::draw("mesh", PlanImage::Output).vertices(mesh, &[3]).depth(),
        ];
        let mut too_many_pictures = plan(vec![], vec![finish()]);
        for i in 0..17 {
            too_many_pictures.layer(&format!("layer{i}"));
        }
        let mut too_many_texels = plan(vec![], vec![finish()]);
        too_many_texels.texture("big", [4096, 4097]);
        let mut wrapping_texels = plan(vec![], vec![finish()]);
        for size in [[u32::MAX, u32::MAX], [u32::MAX, 2], [1, 1]] {
            wrapping_texels.texture("huge", size);
        }
        let (source, output) = (PlanImage::Source, PlanImage::Output);
        let host = |kernel| plan(vec![], vec![PlanPass::host(kernel, source, output)]);
        let small_top = plan(vec![[8, 8]], vec![tint(source, scratch), PlanPass::blend_images(source, scratch, output, BlendMode::Add, 1.0)]);
        let drawing_blur = plan(vec![], vec![PlanPass { draw: Some(Draw::default()), ..PlanPass::blur(source, output, [1.0; 2]) }]);
        let separable = |x: usize, y: usize| Taps::Separable { x: vec![0.5; x], y: vec![0.5; y] };
        let (tall, wide) = (PlanImage::Scratch(0), PlanImage::Scratch(1));
        let resampled_through_tall_and_wide = plan(
            vec![[53, 74], [106, 37], [106, 74]],
            vec![
                PlanPass::resample(source, tall, Filter::Bilinear),
                PlanPass::resample(tall, wide, Filter::Bilinear),
                PlanPass::resample(wide, output, Filter::Bilinear),
            ],
        );
        let mut scratch_and_draw = plan(vec![[53, 37]; 8], vec![]);
        let mesh = scratch_and_draw.buffer(PlanData::Floats(vec![0.0; 9]));
        scratch_and_draw.passes = vec![PlanPass::draw("mesh", output).vertices(mesh, &[3])];
        let cases = [
            ("no passes", plan(vec![], vec![]), "1 to 128 passes"),
            ("129 passes", plan(vec![], vec![finish(); 129]), "1 to 128 passes"),
            ("33 images", plan(vec![[8, 8]; 33], vec![finish()]), "at most 32 scratch images"),
            ("empty image", plan(vec![[0, 8]], vec![finish()]), "scratch image 0 is 0x8"),
            ("huge image", plan(vec![[2 * 53 + 1, 8]], vec![finish()]), "scratch image 0 is 107x8"),
            ("unwritten read", plan(vec![[8, 8]], vec![tint(scratch, PlanImage::Output)]), "before a pass writes it"),
            ("undeclared image", plan(vec![], vec![tint(PlanImage::Source, scratch), finish()]), "does not declare"),
            ("no output", plan(vec![[8, 8]], vec![tint(PlanImage::Source, scratch)]), "no pass writes the output"),
            (
                "reads own output",
                plan(vec![[8, 8]], vec![tint(PlanImage::Source, scratch), tint(scratch, scratch), finish()]),
                "reads the image it writes",
            ),
            ("writes source", plan(vec![], vec![tint(PlanImage::Source, PlanImage::Source), finish()]), "which passes only read"),
            ("writes a layer", plan(vec![], vec![tint(PlanImage::Source, PlanImage::Layer(0)), finish()]), "which passes only read"),
            ("undeclared layer", plan(vec![], vec![tint(PlanImage::Layer(0), PlanImage::Output)]), "does not declare"),
            ("too much scratch", plan(vec![[2 * 53, 2 * 37]; 3], vec![finish()]), "exceed 8 times"),
            (
                "scratch at the limit and a blur temporary",
                plan(vec![[53, 37]; 8], vec![PlanPass::blur(source, scratch, [1.0; 2]), finish()]),
                "the scratch images and the images the passes work in (blur temporaries, resample intermediates, flow levels, draw targets) together exceed 8 times",
            ),
            ("scratch at the limit and a resample's intermediate", resampled_through_tall_and_wide, "together exceed 8 times"),
            ("scratch at the limit and a draw target", scratch_and_draw, "together exceed 8 times"),
            ("97 floats", plan(vec![], vec![finish().floats(&[0.0; 97])]), "97 floats"),
            ("17 uints", plan(vec![], vec![finish().uints(&[0; 17])]), "17 uints"),
            ("17 pictures", too_many_pictures, "at most 16 textures and layers"),
            (
                "a source frame at no time",
                {
                    let mut p = plan(vec![], vec![finish()]);
                    p.source_at(f64::NAN);
                    p
                },
                "give a finite time",
            ),
            (
                "writes a source frame",
                {
                    let mut p = plan(vec![], vec![finish()]);
                    let frame = p.source_at(0.5);
                    p.passes.push(tint(PlanImage::Source, frame));
                    p
                },
                "which passes only read",
            ),
            (
                "scratch and a flow's levels",
                plan(vec![[53, 37]; 5], vec![PlanPass::flow(source, source, scratch), finish()]),
                "flow levels, draw targets) together exceed 8 times",
            ),
            ("too many texels", too_many_texels, "the limit is 16777216"),
            ("texels that wrap", wrapping_texels, "hold 18446744073709551615 pixels; the limit is 16777216"),
            ("draw without its buffer", draw_without_buffer, "names buffer 0"),
            ("depth on two sizes", depth_on_two_sizes, "depth buffer on an image of another size"),
            (
                "wide blur",
                plan(vec![], vec![PlanPass::blur(source, output, [600.0, 1.0])]),
                "pass 0 (blur): `radius` is [600.0, 1.0]; each axis must be 0 to 512",
            ),
            ("blur of nan", plan(vec![], vec![PlanPass::box_blur(source, output, [f32::NAN, 1.0])]), "`radius` is [NaN, 1.0]"),
            ("blend without top", host(HostKernel::Blend { mode: BlendMode::Over, amount: 1.0 }), "pass 0 (blend): `aux` is missing"),
            ("blend of sizes", small_top, "pass 1 (blend): `aux` is 8x8 but `out` is 53x37"),
            (
                "even taps",
                plan(vec![], vec![PlanPass::convolve(source, output, separable(3, 2))]),
                "`y` has 2 taps; give an odd number from 1 to 1025",
            ),
            ("too many taps", plan(vec![], vec![PlanPass::convolve(source, output, separable(1027, 1))]), "`x` has 1027 taps"),
            (
                "ragged full taps",
                host(HostKernel::Convolve { taps: Taps::Full { width: 3, weights: vec![1.0; 6] } }),
                "`weights` has 6 values for `width` 3",
            ),
            ("one point curve", plan(vec![], vec![PlanPass::curves(source, output, &[[0.0; 4]])]), "`table` has 1 entries; it needs 2 to 4096"),
            (
                "infinite matrix",
                plan(vec![], vec![PlanPass::matrix(source, output, [[f32::INFINITY; 4]; 4])]),
                "`rows` holds a value that is not a finite",
            ),
            ("far warp", plan(vec![], vec![PlanPass::warp(source, source, output, [5000.0, 0.0])]), "`amount` is [5000.0, 0.0]"),
            ("zero scale", host(HostKernel::Resample { filter: Filter::Bilinear, scale: Some([0.0, 1.0]) }), "`scale` is [0.0, 1.0]"),
            ("drawing blur", drawing_blur, "pass 0 (blur): a host kernel pass cannot also draw"),
            (
                "flat brightness range",
                plan(vec![], vec![PlanPass::brightness_curve(source, output, Measure::Luma, vec![0.0, 1.0], 0.0)]),
                "pass 0 (brightness_curve): `range` is 0; it must be a finite number above 0",
            ),
            (
                "one point brightness curve",
                plan(vec![], vec![PlanPass::brightness_curve(source, output, Measure::MaxChannel, vec![1.0], 1.0)]),
                "`curve` has 1 entries",
            ),
        ];
        for (name, plan, expected) in cases {
            let why = plan.validate(FRAME).expect_err(name);
            assert!(why.contains(expected), "{name}: {why}");
        }
    }

    #[test]
    fn plans_at_the_limits_pass_and_only_cpu_plans_pass_the_cpu_check() {
        let mut images = vec![[2 * 53, 2 * 37], [53, 37], [53, 37], [53, 37]];
        images.extend([[1, 1]; 12]);
        let at_the_limits = plan(images, std::iter::repeat_n(finish().floats(&[0.0; 96]).uints(&[0; 16]), 64).collect());
        assert_eq!(at_the_limits.validate(FRAME), Ok(()));
        assert_eq!(at_the_limits.check(FRAME), Ok(()));
        let mut with_layer = plan(vec![], vec![]);
        let matte = with_layer.layer("matte");
        let with_layer = with_layer.pass(finish().aux(matte));
        assert_eq!(with_layer.validate(FRAME), Ok(()));
        assert_eq!(with_layer.check(FRAME), Err("a plan with draw passes, buffers, textures or layers runs only on the GPU".into()));
    }

    #[test]
    fn host_kernel_plans_pass_both_checks_and_plans_without_kernels_still_parse() {
        let mut plan = Plan::new();
        let (half, blurred, glow) = (plan.scratch([27, 19]), plan.scratch([27, 19]), plan.scratch(FRAME));
        let plan = plan
            .pass(PlanPass::resample(PlanImage::Source, half, Filter::Bicubic))
            .pass(PlanPass::blur(half, blurred, [512.0, 0.0]))
            .pass(PlanPass::resample(blurred, glow, Filter::Bilinear))
            .pass(PlanPass::blend_images(PlanImage::Source, glow, PlanImage::Output, BlendMode::Screen, 0.5));
        assert_eq!(plan.check(FRAME), Ok(()));
        let sent: Plan = serde_json::from_str(&serde_json::to_string(&plan).unwrap()).unwrap();
        assert_eq!(sent, plan);
        let before_kernels = r#"{"passes":[{"kernel":"tint","src":"source","out":"output","f":[1.0],"reach":2}]}"#;
        let old: Plan = serde_json::from_str(before_kernels).unwrap();
        assert_eq!(old.passes, vec![finish().floats(&[1.0]).reach(2)]);
    }

    #[test]
    fn padding_from_a_plugin_built_before_four_sides_applies_to_every_side() {
        let old: Layout = serde_json::from_str(r#"{"padding":6,"reach":0}"#).unwrap();
        assert_eq!(old.padding, [6; 4]);
        let sides = Layout::pointwise().padded_sides(5, 0, 2, 1);
        let again: Layout = serde_json::from_str(&serde_json::to_string(&sides).unwrap()).unwrap();
        assert_eq!(again.padding, [5, 0, 2, 1]);
    }

    #[test]
    fn rectangle_and_padded_sizes_at_the_ends_of_their_range_neither_wrap_nor_panic() {
        let widest = Rect { left: i32::MIN, top: -1, right: i32::MAX, bottom: i32::MAX };
        assert_eq!([widest.width(), widest.height()], [u32::MAX, 1 << 31]);
        let cx = crate::Cx::new(RenderCx { size: [u32::MAX, 10], ..Default::default() });
        assert_eq!(cx.padded(1).size, [u32::MAX, 10]);
        assert_eq!(
            Window { size: [u32::MAX, u32::MAX], first_row: 0, rows: u32::MAX }.pixels(),
            (u32::MAX as usize).saturating_mul(u32::MAX as usize)
        );
    }

    #[test]
    fn a_slider_or_percent_from_an_older_sdk_has_no_soft_range_and_stays_linear() {
        let slider: Param = serde_json::from_str(r#"{"id":"a","name":"A","kind":"slider","default":1.0,"min":0.0,"max":2.0}"#).unwrap();
        assert_eq!(slider, Param::slider("a", "A", 1.0, 0.0..=2.0));
        let percent: Param = serde_json::from_str(r#"{"id":"b","name":"B","kind":"percent","default":50.0}"#).unwrap();
        assert_eq!(percent, Param::percent("b", "B", 50.0));
        assert_eq!(
            serde_json::to_string(&Param::slider("a", "A", 1.0, 0.0..=2.0)).unwrap(),
            r#"{"id":"a","name":"A","kind":"slider","default":1.0,"min":0.0,"max":2.0}"#
        );
    }

    #[test]
    fn a_slider_takes_a_soft_range_and_a_log_scale_and_a_percent_takes_a_range() {
        let gain = Param::slider("g", "Gain", 1.0, 0.0..=1000.0).soft(0.01..=10.0).log();
        assert_eq!(gain.kind, ParamKind::Slider { default: 1.0, min: 0.0, max: 1000.0, soft: Some([0.01, 10.0]), log: true, percent: false });
        let mix = Param::percent("m", "Mix", 50.0).range(-100.0..=300.0);
        assert_eq!(mix.kind, ParamKind::Slider { default: 50.0, min: -100.0, max: 300.0, soft: None, log: false, percent: true });
        let sent: Param = serde_json::from_str(&serde_json::to_string(&gain).unwrap()).unwrap();
        assert_eq!(sent, gain);
    }

    #[test]
    fn a_percent_keeps_its_soft_range_and_log_scale_whichever_order_the_modifiers_come_in() {
        let expected = ParamKind::Slider { default: 50.0, min: -100.0, max: 300.0, soft: Some([1.0, 200.0]), log: true, percent: true };
        assert_eq!(Param::percent("m", "Mix", 50.0).soft(1.0..=200.0).log().range(-100.0..=300.0).kind, expected);
        assert_eq!(Param::percent("m", "Mix", 50.0).range(-100.0..=300.0).log().soft(1.0..=200.0).kind, expected);
        assert_eq!(serde_json::to_string(&Param::percent("b", "B", 50.0)).unwrap(), r#"{"id":"b","name":"B","kind":"percent","default":50.0}"#);
    }
}
