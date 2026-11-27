use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use craft_plugin::{Call, Overlay, RenderCx, ToolEvent, ToolEventKind, Value as Wire};
use serde_json::Value;

use super::remote::{Bound, Request};
use super::{Document, Gesture, Key, Surfaces};
use crate::Plugin;

const MAX_RETAINED: usize = 256;

pub(super) fn capped(overlay: Overlay, plugin: &str) -> Overlay {
    let (overlay, note) = overlay.capped();
    if let Some(note) = note {
        log::warn!("plugin `{plugin}`: {note}");
    }
    overlay
}

pub struct Shown {
    pub overlay: Overlay,
    pub plugin: String,
    pub cx: RenderCx,
    pub effect: String,
    pub place: [u64; 2],
    pub(super) bound: Bound,
}

impl Shown {
    pub fn binds(&self, param: &str) -> bool {
        self.overlay.handles.iter().any(|handle| handle.bind.params().contains(&param))
    }

    pub fn continues(&self, grabbed: &Shown, handle: usize) -> bool {
        let bind = |shown: &Shown| shown.overlay.handles.get(handle).map(|handle| handle.bind.clone());
        self.bound.same(&grabbed.bound) && self.cx.instance == grabbed.cx.instance && bind(self).is_some() && bind(self) == bind(grabbed)
    }
}

pub(super) struct OverlaySlot {
    pub(super) bound: Bound,
    pub(super) open: Arc<()>,
    request: Vec<u8>,
    pub(super) shown: Option<Arc<Shown>>,
    pub(super) place: [u64; 2],
}

pub(super) struct Tooling {
    pub(super) key: Key,
    pub(super) plugin: Arc<Plugin>,
    pub(super) open: Arc<()>,
    pub(super) overlay: Overlay,
    gesture: Option<Gesture>,
}

impl Surfaces {
    pub fn overlay(&mut self, doc: &mut dyn Document, plugin: &Arc<Plugin>, effect: &str, cx: RenderCx, place: [u64; 2]) -> Option<Arc<Shown>> {
        let (id, instance) = (plugin.manifest().id.clone(), cx.instance);
        let call = Call::Overlay { effect: effect.to_string(), cx };
        let request = serde_json::to_vec(&call).ok()?;
        let key = (id.clone(), instance);
        if self.overlays.len() >= MAX_RETAINED && !self.overlays.contains_key(&key) {
            self.overlays.clear();
        }
        let project = doc.project();
        let fresh = || {
            let open = Arc::default();
            OverlaySlot { bound: Bound::of(plugin, &project, Arc::downgrade(&open)), open, request: Vec::new(), shown: None, place }
        };
        let slot = self.overlays.entry(key.clone()).or_insert_with(fresh);
        if !slot.bound.from(plugin, &project) {
            *slot = fresh();
        }
        let lane = Key::Effect(id, instance);
        if slot.request != request {
            slot.request = request;
            let bound = slot.bound.clone();
            self.post(Request { key: lane.clone(), bound, call, edit: None, held: None, gesture: None });
        }
        self.wait(doc, |surfaces| !surfaces.lanes.contains_key(&lane));
        self.overlays.get(&key)?.shown.clone()
    }

    pub fn drag(&mut self, shown: &Shown, param: &str, key: &str, to: [f64; 2], edit: Value, held: Value) {
        let value = match shown.cx.params.get(param) {
            Some(Wire::Text(text)) => text.clone(),
            _ => String::new(),
        };
        let call = Call::Drag { effect: shown.effect.clone(), cx: shown.cx.clone(), key: key.to_string(), to, value };
        let lane = Key::Effect(shown.plugin.clone(), shown.cx.instance);
        self.post(Request { key: lane, bound: shown.bound.clone(), call, edit: Some(edit), held: Some(held), gesture: None });
    }

    pub fn tool(&mut self, project: &Arc<AtomicBool>, plugin: &Arc<Plugin>, tool: &str, event: ToolEvent) -> Result<(), String> {
        let contributions = plugin.contributions();
        if event.kind.ui_level() > contributions.event_level() {
            return Ok(());
        }
        let id = &plugin.manifest().id;
        let name = contributions
            .tools
            .iter()
            .find(|spec| spec.id == tool)
            .map(|spec| spec.name.clone())
            .ok_or_else(|| format!("plugin `{id}` has no tool `{tool}`"))?;
        let key = Key::Tool(id.clone(), tool.to_string());
        if self.tool.as_ref().is_some_and(|state| state.key != key || !Arc::ptr_eq(&state.plugin, plugin)) {
            self.tool = None;
        }
        let state = self.tool.get_or_insert_with(|| Tooling {
            key: key.clone(),
            plugin: plugin.clone(),
            open: Arc::default(),
            overlay: Overlay::default(),
            gesture: None,
        });
        if event.kind == ToolEventKind::Down {
            state.gesture = Some(Gesture::new(&name));
        }
        let gesture = match event.kind {
            ToolEventKind::Up => state.gesture.take(),
            _ => state.gesture.clone(),
        };
        let bound = Bound::of(plugin, project, Arc::downgrade(&state.open));
        let call = Call::Tool { tool: tool.to_string(), event };
        self.post(Request { key, bound, call, edit: None, held: None, gesture });
        Ok(())
    }

    pub fn tool_overlay(&mut self, doc: &mut dyn Document) -> Option<&Overlay> {
        let key = self.tool.as_ref()?.key.clone();
        self.wait(doc, |surfaces| !surfaces.lanes.contains_key(&key));
        self.tool.as_ref().map(|state| &state.overlay)
    }

    pub fn put_tool_away(&mut self) {
        self.tool = None;
    }
}
