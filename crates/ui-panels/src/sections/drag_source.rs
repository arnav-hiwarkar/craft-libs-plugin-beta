//! `DragSource`: a container as large as its one child that starts a drag of its payload when the pointer drags it.

use crate::contract::{Access, Common, Component, Container, ContainerProps, Cx, NoValue, Outcome, Pad, Pointer as _, Role, Sizing};
use crate::runtime::drag::DragPreview;
use crate::style::{ComponentStyle, StyleCx};
use craft_behavior::{DropOperation, Metrics, PressPhase, Sense};
use egui::Response;

#[cfg(feature = "testing")]
mod stories;

/// The style of a drag source: none of its own.
#[derive(Clone, Debug, Default, PartialEq, ComponentStyle)]
#[non_exhaustive]
pub struct DragSourceStyle {
    #[style(metrics)]
    metrics: Metrics,
}

/// What a drag source reports.
#[derive(Debug)]
#[non_exhaustive]
pub struct DragSourceOutput {
    /// The source's response.
    pub response: Response,
    /// A drag of the payload started this pass.
    pub started: bool,
    /// A target took the drag that started here, with the operation it took it with.
    pub taken: Option<DropOperation>,
}

impl Outcome for DragSourceOutput {
    type Value = NoValue;

    fn response(&self) -> &Response {
        &self.response
    }
}

/// A container that is as large as its one child and starts a drag of `payload` when a press on it moves past slop; the
/// drop targets of type `P` take it. A press on a child that takes presses of its own stays the child's. A labelled source
/// is a group named by its label; an unlabelled one is no node of its own.
#[derive(crate::contract::Component, crate::contract::Props)]
#[component(container, probe(DragSource<u8>))]
pub struct DragSource<P: Clone + Send + Sync + 'static> {
    #[common]
    common: Common,
    #[container]
    props: ContainerProps,
    payload: P,
    /// What the ghost layer shows beside the pointer.
    #[prop]
    preview: DragPreview,
    started: bool,
}

impl<P: Clone + Send + Sync + 'static> DragSource<P> {
    /// A source of `payload`.
    pub fn new(payload: P) -> Self {
        let props = ContainerProps::new().pad(Pad::None).sizing(Sizing::Content);
        DragSource { common: Common::new(), props, payload, preview: DragPreview::None, started: false }
    }
}

impl<P: Clone + Send + Sync + 'static> Component for DragSource<P> {
    const ROLE: Role = Role::GROUP;
    const ALT_ROLES: &'static [Role] = &[Role::None];
    const PARTS: bool = false;
    const STOPS: bool = false;
    type Style = DragSourceStyle;
    type Output = DragSourceOutput;

    fn style(_: &StyleCx<'_>, _: DragSourceStyleKey<P>) -> DragSourceStyle {
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
            cx.start_drag(self.payload.clone(), self.preview.clone());
            self.started = true;
        }
    }

    fn finish(self, cx: Cx<'_, Self>) -> DragSourceOutput {
        DragSourceOutput { response: cx.response.clone(), started: self.started, taken: cx.drag_ended::<P>() }
    }
}

impl<P: Clone + Send + Sync + 'static> Container for DragSource<P> {}
