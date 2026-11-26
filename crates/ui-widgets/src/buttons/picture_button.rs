//! `PictureButton`: a button that shows only a picture, in a rect of the size the caller gives.

use super::button::ButtonOutput;
use crate::contract::{Common, Component, Cx, DisabledLook, MeasureCx, PaintUi, Props as _, Role, Styled as _};
use crate::display::picture::paint_image;
use crate::primitives::{PaintShape, Revision, StrokeWidth};
use crate::runtime::env::Label;
use crate::style::{ComponentStyle, FocusLook, StyleColor, StyleCx, UiTokens};
use craft_behavior::{Metrics, Sense};
use craft_theme::{Rgba, Stateful, States};
use egui::{ColorImage, CornerRadius, Vec2};

#[cfg(feature = "testing")]
mod stories;

/// The style of a picture button.
#[derive(Clone, Debug, PartialEq, ComponentStyle)]
#[non_exhaustive]
pub struct PictureButtonStyle {
    #[style(metrics)]
    metrics: Metrics,
    #[style(focus)]
    focus: FocusLook,
    /// The well behind the picture, which shows while the image is empty or does not fill the rect.
    well: StyleColor,
    /// The corners of the well.
    radius: CornerRadius,
    /// The ring on the outline: clear at rest, the accent while hovered or pressed.
    edge: Stateful<StyleColor>,
}

/// A button that shows only a picture: the image `image` makes, stored under `key`, fitted at its own aspect inside a
/// rect of the size the caller gives, over a sunken well. A click, Space, Enter or an AccessKit click activates it. Its
/// label is its accessible name; a tooltip is the caller's. A new `revision` makes `image` run again.
#[derive(crate::contract::Component, crate::contract::Props)]
#[component(content_painter)]
pub struct PictureButton<'a> {
    #[common]
    common: Common,
    /// The version of the key's picture: a new revision makes `image` run again.
    #[prop]
    revision: Revision,
    key: u64,
    image: &'a dyn Fn() -> ColorImage,
    room: Vec2,
}

impl<'a> PictureButton<'a> {
    /// A button named `label` showing the image `image` makes, stored under `key`, in a rect of `room` points; a
    /// negative or non-finite side is zero.
    pub fn new(label: impl Into<Label>, key: u64, image: &'a dyn Fn() -> ColorImage, room: Vec2) -> Self {
        let side = |side: f32| if side.is_finite() { side.max(0.0) } else { 0.0 };
        PictureButton { common: Common::new(), revision: Revision::default(), key, image, room: Vec2::new(side(room.x), side(room.y)) }.label(label)
    }
}

impl Component for PictureButton<'_> {
    const ROLE: Role = Role::BUTTON;
    const PARTS: bool = false;
    const SHOWS_LABEL: bool = false;
    const OVERLAYS: bool = false;
    const DISABLED: DisabledLook = DisabledLook::Faded;
    type Style = PictureButtonStyle;
    type Output = ButtonOutput;

    fn style(cx: &StyleCx<'_>, _: PictureButtonStyleKey) -> PictureButtonStyle {
        let colors = &cx.tokens.color;
        let (clear, accent) = (cx.color(Rgba::TRANSPARENT), cx.color(colors.accent.solid));
        PictureButtonStyle {
            metrics: Metrics::new(),
            focus: cx.focus_look(),
            well: cx.color(colors.surface.sunken),
            radius: UiTokens::corner(cx.tokens.shape.radius.sm),
            edge: Stateful::new(clear).off(States::new(clear, accent, accent, clear)),
        }
    }

    fn measure(&mut self, _: &MeasureCx<'_, Self>) -> Vec2 {
        self.room
    }

    fn sense(&self) -> Sense {
        Sense::CLICK | Sense::FOCUS
    }

    fn paint(&mut self, ui: &mut PaintUi<'_>, cx: &Cx<'_, Self>) {
        let shape = PaintShape::Rect(cx.rect, cx.style.radius);
        ui.draw().fill(&shape, cx.style.well);
        paint_image(&ui.content_painter(cx), cx.resolved.id, self.key, self.revision, self.image, cx.rect);
        ui.draw().stroke(&shape, StrokeWidth::Thin, cx.state().resolve(&cx.style.edge));
    }

    fn finish(self, cx: Cx<'_, Self>) -> ButtonOutput {
        let activated = self.activated(&cx);
        ButtonOutput::of(&cx, activated)
    }
}
