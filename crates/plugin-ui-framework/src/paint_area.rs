use craft_behavior::{CursorIcon, Metrics, PartKey, PointerButton, PressPhase, PressStart, Sense};
use craft_keymap::{Chord, Mods};
use craft_ui::contract::{Common, Component, Cx, Focus as _, MeasureCx, NoValue, Outcome, PaintUi, PartsCx, Pointer as _, Role};
use craft_ui::primitives::ContentPainterCx;
use craft_ui::runtime::focus::KeyUse;
use craft_ui::style::{ComponentStyle, FocusLook, StyleCx};
use egui::{Rect, Response, Vec2};

const AREA: PartKey = PartKey::new("area");
const SEEN: &str = "seen";
const DEFAULT_ASPECT: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaintPhase {
    Hover,
    Down,
    Drag,
    Up,
    Context,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaintPointer {
    pub(crate) pos: Vec2,
    pub(crate) phase: PaintPhase,
    pub(crate) mods: Mods,
    pub(crate) scroll: Vec2,
    pub(crate) clicks: u32,
}

#[derive(Clone, Debug)]
pub struct PaintAreaOutput {
    pub(crate) response: Response,
    pub(crate) rect: Rect,
    pub(crate) pointers: Vec<PaintPointer>,
    pub(crate) keys: Vec<Chord>,
}

impl Outcome for PaintAreaOutput {
    type Value = NoValue;

    fn response(&self) -> &Response {
        &self.response
    }
}

#[derive(Clone, Debug, PartialEq, ComponentStyle)]
pub struct PaintAreaStyle {
    #[style(metrics)]
    metrics: Metrics,
    #[style(focus)]
    focus: FocusLook,
    least: f32,
}

type Paint<'a> = Box<dyn Fn(&ContentPainterCx, Rect) + 'a>;

#[derive(craft_ui::contract::Component, craft_ui::contract::Props)]
#[component(content_painter)]
pub struct PaintArea<'a> {
    #[common]
    common: Common,
    aspect: f32,
    paint: Paint<'a>,
    pointers: Vec<PaintPointer>,
    keys: Vec<Chord>,
}

impl<'a> PaintArea<'a> {
    pub(crate) fn new(label: impl Into<craft_ui::runtime::env::Label>, aspect: f32, paint: impl Fn(&ContentPainterCx, Rect) + 'a) -> Self {
        use craft_ui::contract::Props as _;
        PaintArea { common: Common::new(), aspect, paint: Box::new(paint), pointers: Vec::new(), keys: Vec::new() }.label(label)
    }

    fn sample(&mut self, cx: &Cx<'_, Self>, pos: egui::Pos2, phase: PaintPhase, scroll: Option<(Vec2, Mods)>) {
        let press = cx.part(AREA).press;
        let clicks = press.phase.start().filter(|_| scroll.is_none() && phase != PaintPhase::Hover).map_or(0, |start| start.clicks);
        let (scroll, mods) = scroll.unwrap_or((Vec2::ZERO, press.mods));
        self.pointers.push(PaintPointer { pos: pos - cx.rect.min, phase, mods, scroll, clicks });
    }

    fn arrived(cx: &mut Cx<'_, Self>, pos: egui::Pos2) -> bool {
        let id = craft_ui::runtime::memory::sub(cx.id, SEEN);
        let seen = cx.core.memory.shown::<Option<egui::Pos2>>(id);
        let arrived = *seen != Some(pos);
        *seen = Some(pos);
        arrived
    }
}

fn aspect_of(aspect: f32) -> f32 {
    if aspect.is_finite() && aspect > 0.0 { aspect } else { DEFAULT_ASPECT }
}

impl Component for PaintArea<'_> {
    const ROLE: Role = Role::CANVAS;
    const OVERLAYS: bool = false;
    const KEYS: KeyUse = KeyUse::RECORDER;
    type Style = PaintAreaStyle;
    type Output = PaintAreaOutput;

    fn style(cx: &StyleCx<'_>, _: PaintAreaStyleKey) -> PaintAreaStyle {
        let least = cx.tokens.size.plot.height;
        PaintAreaStyle { metrics: Metrics::new().min_size(Vec2::new(cx.tokens.size.plot.width, least)), focus: cx.focus_look(), least }
    }

    fn fills(&self) -> Option<craft_behavior::Axis> {
        Some(craft_behavior::Axis::X)
    }

    fn measure(&mut self, cx: &MeasureCx<'_, Self>) -> Vec2 {
        let width = cx.room.unwrap_or(cx.style.least * aspect_of(self.aspect));
        Vec2::new(width, (width / aspect_of(self.aspect)).max(cx.style.least))
    }

    fn sense(&self) -> Sense {
        Sense::FOCUS | Sense::WHEEL_X | Sense::WHEEL_Y
    }

    fn parts(&mut self, rect: Rect, cx: &mut PartsCx<'_, Self>) {
        cx.part(AREA, rect)
            .sense(Sense::CLICK | Sense::CONTEXT_CLICK | Sense::DRAG | Sense::TAKE_FOCUS | Sense::FOLLOWS_IN_SLOP)
            .cursor(CursorIcon::Crosshair)
            .pointer_only("the caller's keys reach the area while it has the focus");
    }

    fn update(&mut self, cx: &mut Cx<'_, Self>) {
        let none = None;
        let phase = cx.part(AREA).press.phase;
        let secondary = phase.start().is_some_and(|start: PressStart| start.button == PointerButton::Secondary);
        match phase {
            PressPhase::Down { start, .. } => {
                if Self::arrived(cx, start.origin) {
                    self.sample(cx, start.origin, PaintPhase::Hover, none);
                }
                self.sample(cx, start.origin, if secondary { PaintPhase::Context } else { PaintPhase::Down }, none);
            }
            _ if secondary => {}
            PressPhase::DragStart { pos, .. } | PressPhase::Drag { pos, .. } => {
                Self::arrived(cx, pos);
                self.sample(cx, pos, PaintPhase::Drag, none);
            }
            PressPhase::Release { pos, .. } => {
                Self::arrived(cx, pos);
                self.sample(cx, pos, PaintPhase::Up, none);
            }
            PressPhase::Cancel { start, .. } => self.sample(cx, start.origin, PaintPhase::Up, none),
            PressPhase::Hover => {
                if let Some(pos) = cx.hover_pos(AREA)
                    && Self::arrived(cx, pos)
                {
                    self.sample(cx, pos, PaintPhase::Hover, none);
                }
            }
            _ => {}
        }
        if let Some(wheel) = cx.take_wheel()
            && let Some(pos) = wheel.at.or_else(|| cx.hover_pos(AREA))
        {
            self.sample(cx, pos, PaintPhase::Hover, Some((wheel.scroll, wheel.mods)));
        }
        self.keys = cx.intents().take_chords();
    }

    fn paint(&mut self, ui: &mut PaintUi<'_>, cx: &Cx<'_, Self>) {
        (self.paint)(&ui.content_painter(cx), cx.rect);
    }

    fn finish(self, cx: Cx<'_, Self>) -> PaintAreaOutput {
        PaintAreaOutput { response: cx.response.clone(), rect: cx.rect, pointers: self.pointers, keys: self.keys }
    }
}
