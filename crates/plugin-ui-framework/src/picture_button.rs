use craft_behavior::{Metrics, Sense};
use craft_theme::{Rgba, Stateful, States};
use craft_ui::contract::{Common, Component, Cx, DisabledLook, MeasureCx, NoValue, Outcome, PaintUi, Props as _, Role, Styled as _};
use craft_ui::primitives::{PaintShape, Revision, StrokeWidth, TextureStore, TextureStoreOptions, fit_rect};
use craft_ui::runtime::env::Label;
use craft_ui::style::{ComponentStyle, FocusLook, StyleColor, StyleCx, UiTokens};
use egui::{ColorImage, CornerRadius, Id, Response, Vec2};

#[derive(Clone, Debug, PartialEq, ComponentStyle)]
pub struct PictureButtonStyle {
    #[style(metrics)]
    metrics: Metrics,
    #[style(focus)]
    focus: FocusLook,
    well: StyleColor,
    radius: CornerRadius,
    edge: Stateful<StyleColor>,
}

#[derive(Clone, Debug)]
pub struct PictureButtonOutput {
    response: Response,
    activated: bool,
}

impl PictureButtonOutput {
    pub(crate) fn clicked(&self) -> bool {
        self.activated
    }
}

impl Outcome for PictureButtonOutput {
    type Value = NoValue;

    fn response(&self) -> &Response {
        &self.response
    }
}

#[derive(craft_ui::contract::Component, craft_ui::contract::Props)]
#[component(content_painter)]
pub struct PictureButton<'a> {
    #[common]
    common: Common,
    revision: Revision,
    key: u64,
    image: &'a dyn Fn() -> ColorImage,
    room: Vec2,
}

impl<'a> PictureButton<'a> {
    pub(crate) fn new(label: impl Into<Label>, key: u64, revision: Revision, image: &'a dyn Fn() -> ColorImage, room: Vec2) -> Self {
        let side = |side: f32| if side.is_finite() { side.max(0.0) } else { 0.0 };
        PictureButton { common: Common::new(), revision, key, image, room: Vec2::new(side(room.x), side(room.y)) }.label(label)
    }
}

impl Component for PictureButton<'_> {
    const ROLE: Role = Role::BUTTON;
    const PARTS: bool = false;
    const SHOWS_LABEL: bool = false;
    const OVERLAYS: bool = false;
    const DISABLED: DisabledLook = DisabledLook::Faded;
    type Style = PictureButtonStyle;
    type Output = PictureButtonOutput;

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
        let painter = ui.content_painter(cx);
        let ctx = painter.painter.ctx();
        let store = TextureStore::<(Id, u64)>::in_context(ctx, "craft-plugin-ui-framework.picture", TextureStoreOptions::thumbnails());
        let texture = store.get_or_put(ctx, (cx.resolved.id, self.key), self.revision, self.image);
        painter.picture(texture.id, fit_rect(cx.rect, texture.size_vec2()), texture.uv(), 0.0);
        ui.draw().stroke(&shape, StrokeWidth::Thin, cx.resolve(&cx.style.edge));
    }

    fn finish(self, cx: Cx<'_, Self>) -> PictureButtonOutput {
        let activated = self.activated(&cx);
        PictureButtonOutput { response: cx.response.clone(), activated }
    }
}
