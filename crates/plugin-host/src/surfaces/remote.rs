use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};
use std::time::Duration;

use craft_plugin::{AudioRequest, Call, ControlsReply, Form, FrameRequest, Overlay, PanelReply, PickFile, Place};
use serde_json::Value;

use super::forms::{self, Values};
use super::{Document, Gesture, Key};
use crate::{Access, Host, Pixels, Plugin, Samples};

pub(super) const POLL: Duration = Duration::from_micros(500);
const RECHECK: Duration = Duration::from_millis(10);

type Served = Box<dyn FnOnce(&mut dyn Document) + Send>;

struct Job {
    project: Arc<AtomicBool>,
    run: Served,
}

pub(super) type Waker = Arc<dyn Fn() + Send + Sync>;

struct Forwarded {
    jobs: Mutex<Vec<Job>>,
    ready: Condvar,
}

static FORWARDED: Forwarded = Forwarded { jobs: Mutex::new(Vec::new()), ready: Condvar::new() };

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(super) fn serve_one(doc: &mut dyn Document) -> bool {
    let project = doc.project();
    let job = {
        let mut jobs = lock(&FORWARDED.jobs);
        let mine = jobs.iter().position(|job| Arc::ptr_eq(&job.project, &project) || job.project.load(Ordering::SeqCst));
        mine.map(|at| jobs.remove(at))
    };
    job.map(|job| (job.run)(doc)).is_some()
}

pub(super) fn wait_for_work(timeout: Duration) {
    let jobs = lock(&FORWARDED.jobs);
    if jobs.is_empty() {
        let _ = FORWARDED.ready.wait_timeout(jobs, timeout);
    }
}

#[cfg(test)]
pub(super) fn forward(project: Arc<AtomicBool>, run: impl FnOnce(&mut dyn Document) + Send + 'static) {
    lock(&FORWARDED.jobs).push(Job { project, run: Box::new(run) });
}

pub fn run_now(doc: &mut dyn Document, plugin: &Arc<Plugin>, call: Call, gesture: Gesture) -> Result<(), String> {
    let asking = Arc::new(());
    let bound = Bound::of(plugin, &doc.project(), Arc::downgrade(&asking));
    let request = Request { key: Key::Command(bound.id().to_string()), bound, call, edit: None, held: None, gesture: Some(gesture) };
    let running = std::thread::Builder::new()
        .name(format!("craft-plugin-command-{}", plugin.manifest().id))
        .spawn(move || run(&request, &(Arc::new(|| {}) as Waker), false).map(drop))
        .map_err(|error| error.to_string())?;
    while !running.is_finished() {
        if !serve_one(doc) {
            wait_for_work(POLL);
        }
    }
    running.join().map_err(|_| "the plugin's command panicked".to_string())?
}

#[derive(Clone)]
pub(super) struct Bound {
    pub(super) plugin: Arc<Plugin>,
    project: Arc<AtomicBool>,
    surface: Weak<()>,
}

impl Bound {
    pub(super) fn of(plugin: &Arc<Plugin>, project: &Arc<AtomicBool>, surface: Weak<()>) -> Bound {
        Bound { plugin: plugin.clone(), project: project.clone(), surface }
    }

    pub(super) fn asked_by(&self, surface: &Arc<()>) -> bool {
        std::ptr::eq(self.surface.as_ptr(), Arc::as_ptr(surface))
    }

    pub(super) fn same(&self, other: &Bound) -> bool {
        Weak::ptr_eq(&self.surface, &other.surface)
    }

    pub(super) fn from(&self, plugin: &Arc<Plugin>, project: &Arc<AtomicBool>) -> bool {
        Arc::ptr_eq(&self.plugin, plugin) && Arc::ptr_eq(&self.project, project)
    }

    pub(super) fn id(&self) -> &str {
        &self.plugin.manifest().id
    }

    pub(super) fn check(&self) -> Result<(), String> {
        let why = match () {
            _ if self.project.load(Ordering::SeqCst) => "its project closed",
            _ if self.surface.strong_count() == 0 => "the panel, overlay, tool or menu that asked for it closed",
            _ if self.plugin.stopped() => "its plugin was reloaded or turned off",
            _ => return Ok(()),
        };
        Err(format!("the app dropped this request because {why}"))
    }
}

struct Remote {
    bound: Bound,
    wake: Waker,
    gesture: Option<Gesture>,
    forms: bool,
}

impl Remote {
    fn on_app<T: Send + 'static>(&self, run: impl FnOnce(&mut dyn Host) -> T + Send + 'static) -> Result<T, String> {
        self.bound.check()?;
        let (reply, answer) = channel();
        let (bound, gesture) = (self.bound.clone(), self.gesture.clone());
        let project = bound.project.clone();
        let run = Box::new(move |doc: &mut dyn Document| {
            let admitted = gesture.as_ref().map_or(Ok(()), |gesture| doc.admit(gesture));
            if let Err(why) = &admitted {
                bound.plugin.warn(why.clone());
            }
            let (mut run, mut answered) = (Some(run), None);
            let served = bound.check().and(admitted).and_then(|()| {
                doc.serve(&bound.plugin, gesture.as_ref(), &mut |host| {
                    if let Some(run) = run.take() {
                        answered = Some(run(host));
                    }
                })
            });
            let answered = served.and_then(|()| answered.ok_or_else(|| "the app did not answer".to_string()));
            if let Err(error) = &answered {
                log::info!("plugin `{}`: {error}", bound.id());
            }
            let _ = reply.send(answered);
        });
        lock(&FORWARDED.jobs).push(Job { project, run });
        FORWARDED.ready.notify_all();
        (self.wake)();
        let answered = loop {
            match answer.recv_timeout(RECHECK) {
                Ok(answered) => break answered,
                Err(RecvTimeoutError::Timeout) => self.bound.check()?,
                Err(RecvTimeoutError::Disconnected) => return Err("the app closed before it answered".into()),
            }
        };
        self.bound.check()?;
        answered
    }
}

impl Host for Remote {
    fn access(&self, command: &str, params: &Value) -> Access {
        let (command, params) = (command.to_string(), params.clone());
        self.on_app(move |host| host.access(&command, &params)).unwrap_or_else(Access::Refused)
    }

    fn command(&mut self, id: &str, params: Value) -> Result<Value, String> {
        let id = id.to_string();
        self.on_app(move |host| host.command(&id, params))?
    }

    fn notify(&mut self, message: &str) {
        let message = message.to_string();
        let _ = self.on_app(move |host| host.notify(&message));
    }

    fn panel(&mut self, panel: &str, instance: Option<&str>, open: bool) -> Result<(), String> {
        let (panel, instance) = (panel.to_string(), instance.map(str::to_string));
        self.on_app(move |host| host.panel(&panel, instance.as_deref(), open))?
    }

    fn command_state(&mut self, command: &str, enabled: bool, checked: Option<bool>) -> Result<(), String> {
        let command = command.to_string();
        self.on_app(move |host| host.command_state(&command, enabled, checked))?
    }

    fn frame(&mut self, request: &FrameRequest) -> Result<Pixels, String> {
        let request = request.clone();
        let render = self.on_app(move |host| host.frame_job(&request))??;
        let pixels = render();
        self.bound.check().map(|()| pixels)
    }

    fn audio(&mut self, request: &AudioRequest) -> Result<Samples, String> {
        let request = request.clone();
        let mix = self.on_app(move |host| host.audio_job(&request))??;
        let samples = mix();
        self.bound.check().map(|()| samples)
    }

    fn pick_file(&mut self, request: &PickFile) -> Result<Option<PathBuf>, String> {
        let request = request.clone();
        self.on_app(move |host| host.pick_file(&request))?
    }

    fn form(&mut self, form: &Form) -> Result<Option<Values>, String> {
        if !self.forms {
            return Err("a form opens only when the command runs in the app's window, from its menus, shortcuts or `ui.menu.invoke`".into());
        }
        let (reply, answer) = channel();
        let id = forms::ask(self.bound.id(), self.bound.project.clone(), form, reply);
        (self.wake)();
        let answered = loop {
            match answer.recv_timeout(RECHECK) {
                Ok(answered) => break Ok(answered),
                Err(RecvTimeoutError::Timeout) => {
                    if let Err(error) = self.bound.check() {
                        break Err(error);
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break Err("the form closed without an answer".into()),
            }
        };
        forms::withdraw(id);
        answered
    }

    fn project_closed(&self) -> Option<Arc<AtomicBool>> {
        Some(self.bound.project.clone())
    }

    fn clipboard(&mut self) -> Result<String, String> {
        self.on_app(|host| host.clipboard())?
    }

    fn set_clipboard(&mut self, text: &str) -> Result<(), String> {
        let text = text.to_string();
        self.on_app(move |host| host.set_clipboard(&text))?
    }

    fn data(&mut self, plugin: &str, place: Place, key: &str) -> Result<Option<Vec<u8>>, String> {
        let (plugin, key) = (plugin.to_string(), key.to_string());
        self.on_app(move |host| host.data(&plugin, place, &key))?
    }

    fn set_data(&mut self, plugin: &str, place: Place, key: &str, value: Option<Vec<u8>>) -> Result<(), String> {
        let (plugin, key) = (plugin.to_string(), key.to_string());
        self.on_app(move |host| host.set_data(&plugin, place, &key, value))?
    }
}

pub(super) struct Request {
    pub(super) key: Key,
    pub(super) bound: Bound,
    pub(super) call: Call,
    pub(super) edit: Option<Value>,
    pub(super) held: Option<Value>,
    pub(super) gesture: Option<Gesture>,
}

pub(super) enum Reply {
    Panel(PanelReply, Option<String>),
    Overlay(Overlay),
    Value(Option<String>),
}

pub(super) struct Done {
    pub(super) request: Request,
    pub(super) reply: Result<Reply, String>,
}

fn run(request: &Request, wake: &Waker, forms: bool) -> Result<Reply, String> {
    let mut host = Remote { bound: request.bound.clone(), wake: wake.clone(), gesture: request.gesture.clone(), forms };
    let (plugin, call) = (&request.bound.plugin, &request.call);
    if matches!(call, Call::Button { .. }) {
        host.on_app(|_| ())?;
    }
    match call {
        Call::Panel { .. } => plugin.call(call, None, &mut host).map(|reply| Reply::Panel(reply, None)),
        Call::Controls { .. } => plugin.call(call, None, &mut host).map(|ControlsReply { widgets, value, topics, timers }| {
            Reply::Panel(PanelReply { widgets, topics, timers, ..Default::default() }, value)
        }),
        Call::Drag { .. } => plugin.call(call, None, &mut host).map(Reply::Value),
        Call::Command { .. } | Call::Button { .. } => plugin.call(call, None, &mut host).map(|()| Reply::Value(None)),
        _ => plugin.call(call, None, &mut host).map(Reply::Overlay),
    }
    .map_err(|error| error.to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Lane {
    Ui,
    Viewer,
    Command,
}

impl Lane {
    pub(super) fn of(call: &Call) -> Lane {
        match call {
            Call::Overlay { .. } | Call::Drag { .. } | Call::Tool { .. } => Lane::Viewer,
            Call::Command { .. } => Lane::Command,
            _ => Lane::Ui,
        }
    }
}

pub(super) struct Worker {
    pub(super) plugin: Arc<Plugin>,
    pub(super) requests: Sender<Request>,
}

pub(super) fn worker(plugin: &Arc<Plugin>, lane: Lane, done: Sender<Done>, wake: Waker) -> std::io::Result<Worker> {
    let (requests, queue) = channel::<Request>();
    std::thread::Builder::new().name(format!("craft-plugin-{}-{lane:?}", plugin.manifest().id)).spawn(move || {
        for request in queue {
            let reply = request.bound.check().and_then(|()| run(&request, &wake, true));
            let _ = done.send(Done { request, reply });
            FORWARDED.ready.notify_all();
            wake();
        }
    })?;
    Ok(Worker { plugin: plugin.clone(), requests })
}
