//! `KeyArea`: a container that a click anywhere inside gives key focus, and that takes the chords it claims while focused.

use crate::contract::{Access, Common, Component, Container, ContainerProps, Cx, Focus as _, NoValue, Outcome, Pad, Role, Sizing};
use crate::style::{ComponentStyle, FocusLook, StyleCx};
use craft_behavior::{Metrics, Sense};
use craft_keymap::Chord;
use egui::Response;

#[cfg(feature = "testing")]
mod stories;

/// The style of a key area: the focus ring it shows while it holds focus.
#[derive(Clone, Debug, PartialEq, ComponentStyle)]
#[non_exhaustive]
pub struct KeyAreaStyle {
    #[style(metrics)]
    metrics: Metrics,
    #[style(focus)]
    focus: FocusLook,
}

/// What a key area reports.
#[derive(Debug)]
#[non_exhaustive]
pub struct KeyAreaOutput {
    /// The area's response.
    pub response: Response,
    /// The chords of `keys` that arrived this pass while the area held key focus, in the order given.
    pub pressed: Vec<Chord>,
}

impl Outcome for KeyAreaOutput {
    type Value = NoValue;

    fn response(&self) -> &Response {
        &self.response
    }
}

/// A container as wide as the room it is offered that takes key focus from a press inside it that no control inside
/// takes focus for (a button's click, a press on its empty room), as a click anywhere in a panel focuses the panel, and
/// that is a Tab stop before its children. While it holds focus, its `keys` reach it before the shortcuts and arrive
/// as `pressed`. A control inside that takes focus on a press keeps it. A labelled area is a group named by its label;
/// an unlabelled one is no node of its own.
#[derive(crate::contract::Component, crate::contract::Props)]
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
    /// An area that takes `keys` while it holds focus.
    pub fn new(keys: Vec<Chord>) -> Self {
        let props = ContainerProps::new().pad(Pad::None).sizing(Sizing::FillWidth);
        KeyArea { common: Common::new(), props, keys, pressed: Vec::new() }
    }
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
        cx.take_presses();
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
