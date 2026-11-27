use craft_behavior::{Metrics, Sense};
use craft_keymap::Chord;
use craft_ui::contract::{Access, Common, Component, Container, ContainerProps, Cx, Focus as _, NoValue, Outcome, Pad, Role, Sizing};
use craft_ui::runtime::focus::FocusTarget;
use craft_ui::style::{ComponentStyle, FocusLook, StyleCx};
use egui::{Context, LayerId, Rect, Response};

const PRESSED: &str = "pressed";

#[derive(Clone, Debug, PartialEq, ComponentStyle)]
pub struct KeyAreaStyle {
    #[style(metrics)]
    metrics: Metrics,
    #[style(focus)]
    focus: FocusLook,
}

#[derive(Debug)]
pub struct KeyAreaOutput {
    response: Response,
    pub(crate) pressed: Vec<Chord>,
}

impl Outcome for KeyAreaOutput {
    type Value = NoValue;

    fn response(&self) -> &Response {
        &self.response
    }
}

#[derive(craft_ui::contract::Component, craft_ui::contract::Props)]
#[component(container)]
pub struct KeyArea {
    #[common]
    common: Common,
    #[container]
    props: ContainerProps,
    keys: Vec<Chord>,
    pressed: Vec<Chord>,
}

impl KeyArea {
    pub(crate) fn new(keys: Vec<Chord>) -> Self {
        let props = ContainerProps::new().pad(Pad::None).sizing(Sizing::FillWidth);
        KeyArea { common: Common::new(), props, keys, pressed: Vec::new() }
    }
}

fn focus_inside(ctx: &Context, rect: Rect, layer: LayerId) -> bool {
    ctx.memory(|memory| memory.focused())
        .and_then(|focused| ctx.read_response(focused))
        .is_some_and(|focused| focused.layer_id == layer && rect.contains_rect(focused.rect))
}

fn pressed_inside(ctx: &Context, rect: Rect, layer: LayerId) -> bool {
    ctx.input(|input| input.pointer.primary_pressed().then(|| input.pointer.press_origin()).flatten())
        .is_some_and(|at| rect.contains(at) && ctx.layer_id_at(at) == Some(layer))
}

impl Component for KeyArea {
    const ROLE: Role = Role::GROUP;
    const ALT_ROLES: &'static [Role] = &[Role::None];
    const PARTS: bool = false;
    type Style = KeyAreaStyle;
    type Output = KeyAreaOutput;

    fn style(cx: &StyleCx<'_>, _: KeyAreaStyleKey) -> KeyAreaStyle {
        KeyAreaStyle { metrics: Metrics::new(), focus: cx.focus_look() }
    }

    fn sense(&self) -> Sense {
        Sense::SURFACE | Sense::FOCUS
    }

    fn access(&self, access: &mut Access, cx: &Cx<'_, Self>) {
        if cx.resolved.label.is_empty() {
            access.role(Role::None);
        }
    }

    fn update(&mut self, cx: &mut Cx<'_, Self>) {
        let ctx = cx.core.ctx.clone();
        let (id, rect, layer) = (cx.id, cx.rect, cx.response.layer_id);
        let pressed = craft_ui::runtime::memory::sub(id, PRESSED);
        if *cx.core.memory.shown::<bool>(pressed) && (focus_inside(&ctx, rect, layer) || craft_ui::runtime::focus::request(&ctx, FocusTarget::Id(id)))
        {
            *cx.core.memory.shown::<bool>(pressed) = false;
        }
        if pressed_inside(&ctx, rect, layer) {
            *cx.core.memory.shown::<bool>(pressed) = true;
            ctx.request_repaint();
        }
        for chord in &self.keys {
            cx.claim(*chord);
            if cx.take_claimed(*chord) {
                self.pressed.push(*chord);
            }
        }
    }

    fn finish(self, cx: Cx<'_, Self>) -> KeyAreaOutput {
        KeyAreaOutput { response: cx.response.clone(), pressed: self.pressed }
    }
}

impl Container for KeyArea {}
