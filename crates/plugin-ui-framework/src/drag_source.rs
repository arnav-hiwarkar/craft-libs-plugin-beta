use craft_behavior::{Metrics, PressPhase, Sense};
use craft_plugin::Dropped;
use craft_ui::contract::{Access, Common, Component, Container, ContainerProps, Cx, NoValue, Outcome, Pad, Pointer as _, Role, Sizing};
use craft_ui::runtime::drag::DragPreview;
use craft_ui::style::{ComponentStyle, StyleCx};
use egui::Response;

#[derive(Clone, Debug, Default, PartialEq, ComponentStyle)]
pub struct DragSourceStyle {
    #[style(metrics)]
    metrics: Metrics,
}

#[derive(Debug)]
pub struct DragSourceOutput {
    response: Response,
}

impl Outcome for DragSourceOutput {
    type Value = NoValue;

    fn response(&self) -> &Response {
        &self.response
    }
}

#[derive(craft_ui::contract::Component, craft_ui::contract::Props)]
#[component(container)]
pub struct DragSource {
    #[common]
    common: Common,
    #[container]
    props: ContainerProps,
    payload: Dropped,
}

impl DragSource {
    pub(crate) fn new(payload: Dropped) -> Self {
        let props = ContainerProps::new().pad(Pad::None).sizing(Sizing::Content);
        DragSource { common: Common::new(), props, payload }
    }
}

impl Component for DragSource {
    const ROLE: Role = Role::GROUP;
    const ALT_ROLES: &'static [Role] = &[Role::None];
    const PARTS: bool = false;
    const STOPS: bool = false;
    type Style = DragSourceStyle;
    type Output = DragSourceOutput;

    fn style(_: &StyleCx<'_>, _: DragSourceStyleKey) -> DragSourceStyle {
        DragSourceStyle::default()
    }

    fn sense(&self) -> Sense {
        Sense::DRAG | Sense::SURFACE
    }

    fn access(&self, access: &mut Access, cx: &Cx<'_, Self>) {
        if cx.resolved.label.is_empty() {
            access.role(Role::None);
        }
    }

    fn update(&mut self, cx: &mut Cx<'_, Self>) {
        if cx.resolved.enabled && matches!(cx.press().phase, PressPhase::DragStart { .. }) {
            cx.start_drag(self.payload.clone(), DragPreview::None);
        }
    }

    fn finish(self, cx: Cx<'_, Self>) -> DragSourceOutput {
        DragSourceOutput { response: cx.response.clone() }
    }
}

impl Container for DragSource {}
