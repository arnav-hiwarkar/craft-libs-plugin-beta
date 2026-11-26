//! `PaintArea`: an area the caller paints and that reports the pointer and the keys over it. A press, drag and
//! release, the hover position and the wheel come out as samples in the area's own space; while it has the focus every
//! key press comes out as a chord. It edits no value: the caller turns the samples into its own changes.

use crate::contract::{Common, Component, Cx, Focus as _, MeasureCx, NoValue, Outcome, PaintUi, PartsCx, Pointer as _, Role};
use crate::primitives::ContentPainterCx;
use crate::runtime::focus::KeyUse;
use crate::style::{ComponentStyle, FocusLook, StyleCx};
use craft_behavior::{CursorIcon, Metrics, PartKey, PointerSample, PressPhase, Sense};
use craft_keymap::{Chord, Mods};
use egui::{Rect, Response, Vec2};

#[cfg(feature = "testing")]
mod stories;

/// The area's one part: every press, hover and wheel over it is the caller's.
pub const AREA: PartKey = PartKey::new("area");

/// The aspect the area takes when the caller gives none: twice as wide as high.
const DEFAULT_ASPECT: f32 = 2.0;

/// Where a sample of the pointer stands in a gesture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PaintPhase {
    /// The pointer moves over the area with no button held, or the wheel turns over it.
    Hover,
    /// The button went down on the area.
    Down,
    /// The held pointer moved.
    Drag,
    /// The button came up, wherever the pointer is.
    Up,
}

/// One sample of the pointer over the area.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct PaintPointer {
    /// The position from the area's top left corner, in points; outside the area during a drag that left it.
    pub pos: Vec2,
    /// The phase.
    pub phase: PaintPhase,
    /// The modifiers held.
    pub mods: Mods,
    /// The wheel's scroll in points, zero but for a wheel sample.
    pub scroll: Vec2,
    /// The pen's pressure and tilt; `None` for a mouse.
    pub sample: Option<PointerSample>,
}

/// What a pass of the area reports.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct PaintAreaOutput {
    /// The area's response.
    pub response: Response,
    /// The area's rect this pass, which the caller's coordinates map onto.
    pub rect: Rect,
    /// The pointer samples of the pass, in order.
    pub pointers: Vec<PaintPointer>,
    /// The key chords pressed while the area had the focus.
    pub keys: Vec<Chord>,
}

impl Outcome for PaintAreaOutput {
    type Value = NoValue;

    fn response(&self) -> &Response {
        &self.response
    }
}

/// The style of the area: the focus ring around it.
#[derive(Clone, Debug, PartialEq, ComponentStyle)]
#[non_exhaustive]
pub struct PaintAreaStyle {
    #[style(metrics)]
    metrics: Metrics,
    #[style(focus)]
    focus: FocusLook,
    /// The least height, from the plot token.
    least: f32,
}

type Paint<'a> = Box<dyn Fn(&ContentPainterCx, Rect) + 'a>;

/// An area the caller paints that reports the pointer and the keys over it.
///
/// It is as wide as the room and `aspect` times as wide as high, at least a plot high. It is one Tab stop; while focused
/// it takes every key as a chord, so the caller's keys reach it before the app's shortcuts, and Tab leaves it.
#[derive(crate::contract::Component, crate::contract::Props)]
#[component(content_painter)]
pub struct PaintArea<'a> {
    #[common]
    common: Common,
    /// Width over height.
    #[prop(quantity = "ratio")]
    aspect: f32,
    paint: Paint<'a>,
    pointers: Vec<PaintPointer>,
    keys: Vec<Chord>,
}

impl<'a> PaintArea<'a> {
    /// An area named `label` that `paint` paints into its rect.
    pub fn new(label: impl Into<crate::runtime::env::Label>, paint: impl Fn(&ContentPainterCx, Rect) + 'a) -> Self {
        use crate::contract::Props as _;
        PaintArea { common: Common::new(), aspect: DEFAULT_ASPECT, paint: Box::new(paint), pointers: Vec::new(), keys: Vec::new() }.label(label)
    }

    fn sample(&mut self, cx: &Cx<'_, Self>, pos: egui::Pos2, phase: PaintPhase, scroll: Option<(Vec2, Mods)>) {
        let press = cx.part(AREA).press;
        let (scroll, mods) = scroll.unwrap_or((Vec2::ZERO, press.mods));
        self.pointers.push(PaintPointer { pos: pos - cx.rect.min, phase, mods, scroll, sample: press.sample });
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
            .sense(Sense::CLICK | Sense::DRAG | Sense::TAKE_FOCUS | Sense::FOLLOWS_IN_SLOP)
            .cursor(CursorIcon::Crosshair)
            .pointer_only("the caller's keys reach the area while it has the focus");
    }

    fn update(&mut self, cx: &mut Cx<'_, Self>) {
        let none = None;
        match cx.part(AREA).press.phase {
            PressPhase::Down { start, .. } => self.sample(cx, start.origin, PaintPhase::Down, none),
            PressPhase::DragStart { pos, .. } | PressPhase::Drag { pos, .. } => self.sample(cx, pos, PaintPhase::Drag, none),
            PressPhase::Release { pos, .. } => self.sample(cx, pos, PaintPhase::Up, none),
            PressPhase::Cancel { start, .. } => self.sample(cx, start.origin, PaintPhase::Up, none),
            PressPhase::Hover => {
                if let Some(pos) = cx.hover_pos(AREA) {
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
