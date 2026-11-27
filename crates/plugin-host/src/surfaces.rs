mod forms;
mod remote;
#[cfg(all(test, not(target_family = "wasm")))]
mod tests;
pub(crate) mod viewer;

use std::any::Any;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use craft_plugin::{AppEvent, Call, MenuItem, Overlay, ToolEventKind, UiEvent, UiInput, Widget};
use serde_json::Value;

pub use forms::{AskedForm, Values, answer_form, asked_form};
pub use remote::run_now;
use remote::{Bound, Done, Lane, Reply, Request, Waker, Worker, lock, serve_one, wait_for_work, worker};
use viewer::{OverlaySlot, Tooling, capped};

use crate::{Host, MAX_BACKLOG, Plugin, Retained};

const WAIT: Duration = Duration::from_millis(4);

pub trait Document {
    fn project(&self) -> Arc<AtomicBool>;

    fn app_events(&mut self) -> Vec<AppEvent>;

    fn admit(&mut self, _gesture: &Gesture) -> Result<(), String> {
        Ok(())
    }

    fn serve(&mut self, plugin: &Arc<Plugin>, gesture: Option<&Gesture>, call: &mut dyn FnMut(&mut dyn Host)) -> Result<(), String>;

    fn holds(&self, edit: &Value) -> Value;

    fn write(&mut self, edit: &Value, value: String) -> Result<(), String>;
}

#[derive(Clone)]
pub struct Gesture {
    label: String,
    keeps_own: bool,
    state: Arc<Mutex<Option<Box<dyn Any + Send>>>>,
}

impl Gesture {
    pub fn new(label: &str) -> Gesture {
        Gesture { label: label.to_string(), keeps_own: false, state: Arc::default() }
    }

    pub fn command(plugin: &Plugin, command: &str) -> Gesture {
        let commands = &plugin.contributions().commands;
        Gesture::new(commands.iter().find(|spec| spec.id == command).map_or(command, |spec| spec.title.as_str())).keeping_own()
    }

    pub fn keeping_own(mut self) -> Gesture {
        self.keeps_own = true;
        self
    }

    pub fn with<T: Send + 'static>(self, state: T) -> Gesture {
        *lock(&self.state) = Some(Box::new(state));
        self
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn keeps_own(&self) -> bool {
        self.keeps_own
    }

    pub fn state<T: Default + Send + 'static, R>(&self, look: impl FnOnce(&mut T) -> R) -> R {
        let mut state = lock(&self.state);
        let mut held = state.take().and_then(|held| held.downcast::<T>().ok()).unwrap_or_default();
        let answer = look(&mut held);
        *state = Some(held);
        answer
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Key {
    Slot(Owner),
    Effect(String, u64),
    Tool(String, String),
    Command(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Owner {
    Panel { plugin: String, panel: String, instance: Option<String> },
    Control(u64),
}

impl Owner {
    pub fn panel(plugin: &str, panel: &str, instance: Option<&str>) -> Owner {
        Owner::Panel { plugin: plugin.into(), panel: panel.into(), instance: instance.map(str::to_string) }
    }
}

#[derive(Clone, Debug)]
pub enum Surface<'a> {
    Panel { panel: &'a str, instance: Option<&'a str> },
    Control { effect: &'a str, param: &'a str, instance: u64, value: &'a str, edit: Option<Value> },
}

impl Surface<'_> {
    fn owner(&self, plugin: &str) -> Owner {
        match *self {
            Surface::Panel { panel, instance } => Owner::panel(plugin, panel, instance),
            Surface::Control { instance, .. } => Owner::Control(instance),
        }
    }

    fn call(&self) -> Call {
        match *self {
            Surface::Panel { panel, instance } => {
                Call::Panel { panel: panel.to_string(), input: UiInput { instance: instance.map(str::to_string), ..UiInput::default() } }
            }
            Surface::Control { effect, param, instance, value, .. } => {
                Call::Controls { effect: effect.to_string(), param: param.to_string(), instance, value: value.to_string(), input: UiInput::default() }
            }
        }
    }
}

pub struct Shown<'a> {
    pub plugin: &'a str,
    pub widgets: &'a [Widget],
    pub error: Option<&'a str>,
}

struct Slot {
    retained: Retained,
    plugin: Arc<Plugin>,
    project: Arc<AtomicBool>,
    open: Arc<()>,
    shown: bool,
    hidden: bool,
    value: Option<String>,
    held: bool,
    gestures: Vec<(u64, Gesture)>,
}

impl Slot {
    fn new(plugin: &Arc<Plugin>, project: &Arc<AtomicBool>) -> Slot {
        let retained = Retained::with_level(plugin.contributions().event_level());
        Slot {
            retained,
            plugin: plugin.clone(),
            project: project.clone(),
            open: Arc::default(),
            shown: false,
            hidden: false,
            value: None,
            held: false,
            gestures: Vec::new(),
        }
    }

    fn redraw_for(&mut self, project: &Arc<AtomicBool>) {
        self.retained.reset();
        (self.project, self.open, self.value, self.held) = (project.clone(), Arc::default(), None, false);
        self.gestures.clear();
    }

    fn hold(&mut self, held: bool) {
        if held && !self.held {
            self.retained.split();
            self.gestures.push((self.retained.gesture(), Gesture::new(&self.plugin.manifest().name)));
        }
        self.held = held;
    }

    fn started_gesture(&self) -> Gesture {
        let started = self.retained.started_gesture();
        let held = self.gestures.iter().find(|(gesture, _)| *gesture == started);
        held.map_or_else(|| Gesture::new(&self.plugin.manifest().name), |(_, gesture)| gesture.clone())
    }

    fn forget_ended_gestures(&mut self) {
        let (current, held, retained) = (self.retained.gesture(), self.held, &self.retained);
        self.gestures.retain(|(gesture, _)| (held && *gesture == current) || retained.pending_in(*gesture));
    }
}

struct Inbox(Sender<Done>, Receiver<Done>);

impl Default for Inbox {
    fn default() -> Self {
        let (sender, receiver) = channel();
        Inbox(sender, receiver)
    }
}

#[derive(Default)]
pub struct Surfaces {
    slots: HashMap<Owner, Slot>,
    overlays: HashMap<(String, u64), OverlaySlot>,
    tool: Option<Tooling>,
    lanes: HashMap<Key, VecDeque<Request>>,
    workers: HashMap<(String, Lane), Worker>,
    done: Inbox,
    wake: Option<Waker>,
    now: Duration,
    deadline: Option<Instant>,
    scheduled: Option<Duration>,
    notice: Option<String>,
    open: Arc<()>,
}

impl Surfaces {
    pub fn has_waker(&self) -> bool {
        self.wake.is_some()
    }

    pub fn set_waker(&mut self, wake: impl Fn() + Send + Sync + 'static) {
        self.wake = Some(Arc::new(wake));
    }

    fn waker(&self) -> Waker {
        self.wake.clone().unwrap_or_else(|| Arc::new(|| {}))
    }

    pub fn frame(&mut self, doc: &mut dyn Document, now: Duration) -> Option<Duration> {
        self.now = now;
        self.deadline = Some(Instant::now() + WAIT);
        self.work(doc);
        let current = |plugin: &Arc<Plugin>| !plugin.stopped();
        self.slots.retain(|_, slot| current(&slot.plugin));
        self.overlays.retain(|_, overlaid| current(&overlaid.bound.plugin));
        self.workers.retain(|_, worker| current(&worker.plugin));
        if self.tool.as_ref().is_some_and(|tool| !current(&tool.plugin)) {
            self.tool = None;
        }
        for slot in self.slots.values_mut() {
            if !std::mem::take(&mut slot.shown) {
                slot.retained.hide();
                slot.hidden = true;
            }
        }
        for event in doc.app_events() {
            self.slots.values_mut().for_each(|slot| slot.retained.notify(&event));
        }
        self.scheduled = self.slots.values().filter_map(|slot| slot.retained.next_due()).min();
        self.scheduled.map(|due| due.saturating_sub(now))
    }

    pub fn submit(&mut self, key: &Owner, events: Vec<UiEvent>, keys: &str) {
        if let Some(slot) = self.slots.get_mut(key) {
            slot.retained.push(events, keys);
        }
    }

    pub fn hold(&mut self, key: &Owner, held: bool) {
        if let Some(slot) = self.slots.get_mut(key) {
            slot.hold(held);
        }
    }

    pub fn press_key(&mut self, key: &Owner, chord: &str) {
        if let Some(slot) = self.slots.get_mut(key) {
            slot.retained.press(chord);
        }
    }

    pub fn menu(&self, key: &Owner) -> &[MenuItem] {
        self.slots.get(key).map_or(&[], |slot| slot.retained.menu())
    }

    pub fn busy(&self, key: &Owner) -> bool {
        self.slots.get(key).is_some_and(|slot| slot.retained.in_flight())
    }

    pub fn calls(&self, key: &Owner) -> u64 {
        self.slots.get(key).map_or(0, |slot| slot.retained.calls())
    }

    pub fn idle(&self) -> bool {
        self.lanes.is_empty() && !self.slots.values().any(|slot| slot.retained.in_flight())
    }

    pub fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }

    pub fn command(&mut self, project: &Arc<AtomicBool>, plugin: &Arc<Plugin>, command: &str) {
        let call = Call::Command { command: command.to_string() };
        self.press(project, plugin, call, Gesture::command(plugin, command));
    }

    pub fn press(&mut self, project: &Arc<AtomicBool>, plugin: &Arc<Plugin>, call: Call, gesture: Gesture) {
        let bound = Bound::of(plugin, project, Arc::downgrade(&self.open));
        self.post(Request { key: Key::Command(bound.id().to_string()), bound, call, edit: None, held: None, gesture: Some(gesture) });
    }

    pub fn show(&mut self, doc: &mut dyn Document, plugin: &Arc<Plugin>, surface: Surface, size: [f32; 2]) -> Result<Shown<'_>, String> {
        let (owner, call, project) = (surface.owner(&plugin.manifest().id), surface.call(), doc.project());
        let key = &owner;
        let slot = self.slots.entry(owner.clone()).or_insert_with(|| Slot::new(plugin, &project));
        if !Arc::ptr_eq(&slot.plugin, plugin) {
            *slot = Slot::new(plugin, &project);
        } else if !Arc::ptr_eq(&slot.project, &project) {
            slot.redraw_for(&project);
        }
        slot.shown = true;
        let mut edit = None;
        if let Surface::Control { value, edit: target, .. } = surface {
            edit = target;
            if slot.value.as_deref() != Some(value) {
                slot.value = Some(value.to_string());
                slot.retained.invalidate();
            }
        }
        let panel = matches!(call, Call::Panel { .. });
        let bound = Bound::of(plugin, &project, Arc::downgrade(&slot.open));
        while let Some((input, gesture)) = self.slots.get_mut(key).and_then(|slot| {
            let input = slot.retained.start(size, self.now)?;
            slot.hidden = false;
            Some((input, panel.then(|| slot.started_gesture())))
        }) {
            let mut call = call.clone();
            if let Call::Panel { input: given, .. } | Call::Controls { input: given, .. } = &mut call {
                *given = UiInput { instance: given.instance.take(), ..input };
            }
            self.send(Request { key: Key::Slot(owner.clone()), bound: bound.clone(), call, edit: edit.clone(), held: None, gesture });
            self.wait(doc, |surfaces| !surfaces.busy(key));
            if self.busy(key) || edit.is_some() || self.spent() {
                break;
            }
        }
        self.schedule(key);
        let slot = self.slots.get_mut(key).ok_or("the panel closed")?;
        slot.forget_ended_gestures();
        Ok(Shown { plugin: &slot.plugin.manifest().id, widgets: slot.retained.widgets(), error: slot.retained.error() })
    }

    pub fn close(&mut self, project: &Arc<AtomicBool>, owner: &Owner, plugin: Option<&Arc<Plugin>>) {
        self.slots.remove(owner);
        if let (Some(plugin), Owner::Panel { panel, instance, .. }) = (plugin, owner) {
            let input = UiInput { instance: instance.clone(), closed: true, ..UiInput::default() };
            let bound = Bound::of(plugin, project, Arc::downgrade(&self.open));
            let call = Call::Panel { panel: panel.clone(), input };
            self.send(Request { key: Key::Slot(owner.clone()), bound, call, edit: None, held: None, gesture: None });
        }
    }

    fn schedule(&mut self, key: &Owner) {
        let due = self.slots.get(key).and_then(|slot| slot.retained.next_due());
        if let Some(due) = due
            && self.scheduled.is_none_or(|scheduled| due < scheduled)
        {
            self.scheduled = Some(due);
            self.waker()();
        }
    }

    fn send(&mut self, request: Request) {
        let (done, wake) = (self.done.0.clone(), self.waker());
        let fail = |request, error: String| {
            let _ = done.send(Done { request, reply: Err(error) });
        };
        if let Err(error) = request.bound.check() {
            return fail(request, error);
        }
        let lane = (request.bound.id().to_string(), Lane::of(&request.call));
        let worker = match self.workers.entry(lane.clone()) {
            Entry::Occupied(found) if Arc::ptr_eq(&found.get().plugin, &request.bound.plugin) => found.into_mut(),
            entry => match worker(&request.bound.plugin, lane.1, done.clone(), wake) {
                Ok(started) => entry.insert_entry(started).into_mut(),
                Err(error) => return fail(request, error.to_string()),
            },
        };
        if let Err(unsent) = worker.requests.send(request) {
            self.workers.remove(&lane);
            fail(unsent.0, "the plugin's worker stopped".into());
        }
    }

    fn post(&mut self, request: Request) {
        let Some(waiting) = self.lanes.get_mut(&request.key) else {
            self.lanes.insert(request.key.clone(), VecDeque::new());
            return self.send(request);
        };
        let merges = |queued: &Request| match (&queued.call, &request.call) {
            (Call::Overlay { .. }, Call::Overlay { .. }) => true,
            (Call::Drag { key: handle, .. }, Call::Drag { key, .. }) => handle == key && queued.edit == request.edit,
            (Call::Tool { event: last, .. }, Call::Tool { event, .. }) => last.kind == ToolEventKind::Move && event.kind == ToolEventKind::Move,
            _ => false,
        };
        let kind = std::mem::discriminant(&request.call);
        let latest = waiting.iter().rposition(|queued| std::mem::discriminant(&queued.call) == kind);
        match latest.filter(|at| merges(&waiting[*at])) {
            Some(at) => waiting[at] = request,
            None => waiting.push_back(request),
        }
        let motion = |queued: &Request| match &queued.call {
            Call::Tool { event, .. } => event.kind == ToolEventKind::Move,
            Call::Drag { .. } => true,
            _ => false,
        };
        while waiting.len() > MAX_BACKLOG
            && let Some(oldest) = waiting.iter().position(motion)
        {
            waiting.remove(oldest);
        }
    }

    fn advance(&mut self, doc: &dyn Document, key: &Key) {
        match self.lanes.get_mut(key).and_then(VecDeque::pop_front) {
            Some(mut next) => {
                if let (Some(edit), Some(held)) = (&next.edit, &mut next.held) {
                    *held = doc.holds(edit);
                }
                self.send(next)
            }
            None => {
                self.lanes.remove(key);
            }
        }
    }

    fn spent(&mut self) -> bool {
        Instant::now() >= *self.deadline.get_or_insert_with(|| Instant::now() + WAIT)
    }

    fn work(&mut self, doc: &mut dyn Document) -> bool {
        let mut any = false;
        while serve_one(doc) || self.done.1.try_recv().map(|done| self.apply(doc, done)).is_ok() {
            any = true;
            if self.spent() {
                self.waker()();
                break;
            }
        }
        any
    }

    fn apply(&mut self, doc: &mut dyn Document, Done { request, reply }: Done) {
        let stale = request.bound.check().err();
        if let Some(why) = &stale {
            log::info!("plugin `{}`: a reply for {:?} was dropped: {why}", request.bound.id(), request.key);
        }
        let Request { key, bound, call, edit, held, .. } = request;
        if let Key::Slot(slot_key) = &key {
            let Some(slot) = self.slots.get_mut(slot_key).filter(|slot| bound.asked_by(&slot.open)) else { return };
            match (stale, reply) {
                (None, Ok(Reply::Panel(reply, edited))) => {
                    slot.retained.finish(Ok(reply), self.now);
                    if let (false, Some(value), Some(edit)) = (slot.hidden, edited, &edit)
                        && let Err(error) = doc.write(edit, value)
                    {
                        self.notice = Some(error);
                    }
                }
                (None, Err(error)) => slot.retained.finish(Err(error), self.now),
                _ => slot.retained.abandon(),
            }
            if self.slots.get(slot_key).is_some_and(|slot| slot.retained.pending()) {
                self.waker()();
            }
            return;
        }
        if stale.is_none() {
            self.answered(doc, &key, bound, call, (edit, held), reply);
        }
        self.advance(doc, &key);
    }

    fn answered(
        &mut self,
        doc: &mut dyn Document,
        key: &Key,
        bound: Bound,
        call: Call,
        edit: (Option<Value>, Option<Value>),
        reply: Result<Reply, String>,
    ) {
        match (call, reply) {
            (Call::Overlay { effect, cx }, reply) => {
                let overlay = match reply {
                    Ok(Reply::Overlay(overlay)) => capped(overlay, bound.id()),
                    Ok(_) => Overlay::default(),
                    Err(error) => {
                        log::debug!("plugin `{}` effect `{effect}` draws no overlay: {error}", bound.id());
                        Overlay::default()
                    }
                };
                let Key::Effect(plugin, instance) = key else { return };
                if let Some(slot) = self.overlays.get_mut(&(plugin.clone(), *instance)).filter(|slot| bound.asked_by(&slot.open)) {
                    slot.shown = Some(Arc::new(viewer::Shown { overlay, plugin: plugin.clone(), cx, bound, effect, place: slot.place }));
                }
            }
            (Call::Tool { .. }, Ok(Reply::Overlay(overlay))) => {
                if let Some(tool) = self.tool.as_mut().filter(|tool| bound.asked_by(&tool.open)) {
                    tool.overlay = capped(overlay, bound.id());
                }
            }
            (Call::Drag { .. }, Ok(Reply::Value(Some(value)))) => match edit {
                (Some(edit), Some(held)) if doc.holds(&edit) != held => {
                    log::info!("plugin `{}`: a handle's drag reply was dropped because its parameter changed while the plugin answered", bound.id());
                }
                (Some(edit), _) => {
                    if let Err(error) = doc.write(&edit, value) {
                        self.notice = Some(error);
                    }
                }
                (None, _) => {}
            },
            (_, Err(error)) => self.notice = Some(error),
            _ => {}
        }
    }

    fn wait(&mut self, doc: &mut dyn Document, done: impl Fn(&Surfaces) -> bool) {
        while !done(self) && !self.spent() {
            if !self.work(doc) {
                wait_for_work(remote::POLL);
            }
        }
    }
}
