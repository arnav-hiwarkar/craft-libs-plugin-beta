use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use craft_plugin::{AppEvent, Call, Effect, EffectSpec, Param, RenderCx, ToolEvent, ToolEventKind, UiEvent, UiEventKind, Value as Wire, Widget};
use serde_json::{Value, json};

use super::remote::{Bound, Lane, Request, forward};
use super::{Document, Gesture, Key, Owner, Surface, Surfaces};
use crate::{Host, Limits, Plugin};

#[derive(Default)]
struct Doc {
    project: Arc<AtomicBool>,
    values: HashMap<String, String>,
    notes: Vec<String>,
    events: Vec<AppEvent>,
}

struct Told<'a>(&'a mut Vec<String>);

impl Host for Told<'_> {
    fn notify(&mut self, message: &str) {
        self.0.push(message.to_string());
    }
}

impl Document for Doc {
    fn project(&self) -> Arc<AtomicBool> {
        self.project.clone()
    }

    fn app_events(&mut self) -> Vec<AppEvent> {
        std::mem::take(&mut self.events)
    }

    fn serve(&mut self, _plugin: &Arc<Plugin>, _gesture: Option<&Gesture>, call: &mut dyn FnMut(&mut dyn Host)) -> Result<(), String> {
        call(&mut Told(&mut self.notes));
        Ok(())
    }

    fn holds(&self, edit: &Value) -> Value {
        json!(edit["param"].as_str().and_then(|param| self.values.get(param)))
    }

    fn write(&mut self, edit: &Value, value: String) -> Result<(), String> {
        let param = edit["param"].as_str().ok_or("an edit names its parameter")?;
        self.values.insert(param.to_string(), value);
        Ok(())
    }
}

static HELD: Mutex<()> = Mutex::new(());
static HOLD: AtomicBool = AtomicBool::new(false);

struct Pin;

impl Effect for Pin {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("pin", "Pin", "Test").param(Param::custom("pin", "Pin", "0,0"))
    }

    fn overlay(&self, _cx: &craft_plugin::Cx, overlay: &mut craft_plugin::Overlay) {
        overlay.custom("pin", "pin", [1.0, 2.0]);
    }

    fn drag(&self, _cx: &craft_plugin::Cx, _key: &str, to: [f64; 2], value: &mut String) {
        while HOLD.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(1));
        }
        *value = format!("{},{}", to[0], to[1]);
    }
}

#[derive(Default)]
struct Tell {
    told: u32,
}

impl craft_plugin::Panel for Tell {
    fn ui(&mut self, ui: &mut craft_plugin::Ui) {
        if ui.button("Tell") {
            self.told += 1;
            craft_plugin::host::notify(format!("told {}", self.told));
        }
        ui.label(&format!("{} told", self.told));
    }
}

fn load(id: &str) -> (tempfile::TempDir, Arc<Plugin>) {
    let dir = tempfile::tempdir().expect("temp dir");
    let manifest = serde_json::from_value(json!({"id": id, "name": "Test", "version": "1.0.0", "entry": {"wasm": "test.wasm"}})).expect("a manifest");
    let plugin = craft_plugin::Plugin::new().effect(Pin).panel("tell", "Tell", Tell::default()).command("none", "None", || Ok(()));
    let plugin = Plugin::in_process(manifest, plugin, dir.path().into(), dir.path().join("data"), Limits::default()).expect("the plugin loads");
    (dir, Arc::new(plugin))
}

fn request(surfaces: &Surfaces, doc: &Doc, key: Key, plugin: &Arc<Plugin>, call: Call) -> Request {
    Request { key, bound: Bound::of(plugin, &doc.project, Arc::downgrade(&surfaces.open)), call, edit: None, held: None, gesture: None }
}

fn idle(surfaces: &mut Surfaces, doc: &mut Doc) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !surfaces.idle() {
        assert!(Instant::now() < deadline, "the plugin did not answer within 20 s");
        surfaces.frame(doc, Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn a_request_queued_for_a_plugin_that_was_reloaded_is_dropped_and_the_new_loads_worker_stays() {
    const RELOADED: &str = "test.queue-reloaded";
    let (_first_dir, first) = load(RELOADED);
    let (mut surfaces, mut doc) = (Surfaces::default(), Doc::default());
    let command = || Call::Command { command: "none".into() };
    let old = Key::Effect(RELOADED.into(), 1);
    for _ in 0..2 {
        let queued = request(&surfaces, &doc, old.clone(), &first, command());
        surfaces.post(queued);
    }
    first.stop();
    let (_second_dir, second) = load(RELOADED);
    let new = request(&surfaces, &doc, Key::Effect(RELOADED.into(), 2), &second, command());
    surfaces.post(new);
    idle(&mut surfaces, &mut doc);
    assert!(Arc::ptr_eq(&surfaces.workers[&(RELOADED.to_string(), Lane::Command)].plugin, &second));
}

#[test]
fn a_frame_answers_queued_app_requests_only_until_its_budget_is_spent() {
    let mut doc = Doc::default();
    let served = Arc::new(AtomicUsize::new(0));
    for _ in 0..2 {
        let counted = served.clone();
        forward(doc.project.clone(), move |_| {
            std::thread::sleep(Duration::from_millis(20));
            counted.fetch_add(1, Ordering::SeqCst);
        });
    }
    let mut surfaces = Surfaces::default();
    let wakes = Arc::new(AtomicUsize::new(0));
    let woken = wakes.clone();
    surfaces.set_waker(move || {
        woken.fetch_add(1, Ordering::SeqCst);
    });
    surfaces.frame(&mut doc, Duration::ZERO);
    let first = (served.load(Ordering::SeqCst), wakes.load(Ordering::SeqCst));
    surfaces.frame(&mut doc, Duration::ZERO);
    assert_eq!((first, served.load(Ordering::SeqCst)), ((1, 1), 2));
}

fn tool(kind: ToolEventKind, x: f64) -> Call {
    let event = ToolEvent { kind, comp: [x, 0.0], zoom: 1.0, ..Default::default() };
    Call::Tool { tool: "t".into(), event }
}

#[test]
fn a_tool_lane_past_its_bound_drops_its_oldest_moves_and_keeps_every_key() {
    let (_dir, plugin) = load("test.queue-lane");
    let (mut surfaces, doc) = (Surfaces::default(), Doc::default());
    let lane = Key::Tool("test.queue-lane".into(), "t".into());
    for (kind, x) in [(ToolEventKind::Down, 0.0)]
        .into_iter()
        .chain((0..50).flat_map(|x| [(ToolEventKind::Move, f64::from(x)), (ToolEventKind::Key, f64::from(x))]))
    {
        let posted = request(&surfaces, &doc, lane.clone(), &plugin, tool(kind, x));
        surfaces.post(posted);
    }
    let queued: Vec<(ToolEventKind, f64)> = surfaces.lanes[&lane]
        .iter()
        .filter_map(|request| match &request.call {
            Call::Tool { event, .. } => Some((event.kind, event.comp[0])),
            _ => None,
        })
        .collect();
    let kept: Vec<(ToolEventKind, f64)> = (0..50)
        .flat_map(|x| [(x >= 36).then_some((ToolEventKind::Move, f64::from(x))), Some((ToolEventKind::Key, f64::from(x)))])
        .flatten()
        .collect();
    assert_eq!(queued, kept);
}

fn labels(widgets: &[Widget]) -> Vec<String> {
    widgets
        .iter()
        .filter_map(|widget| match widget {
            Widget::Label { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_panel_click_reaches_the_plugin_and_its_host_call_is_answered_by_the_document_that_shows_it() {
    let (_dir, plugin) = load("test.surfaces-tell");
    let (mut surfaces, mut doc) = (Surfaces::default(), Doc::default());
    let owner = Owner::panel("test.surfaces-tell", "tell", None);
    let shown = |surfaces: &mut Surfaces, doc: &mut Doc| {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            surfaces.frame(doc, Duration::ZERO);
            let widgets = surfaces
                .show(doc, &plugin, Surface::Panel { panel: "tell", instance: None }, [200.0, 100.0])
                .expect("the panel shows")
                .widgets
                .to_vec();
            if !surfaces.busy(&owner) && !widgets.is_empty() {
                return widgets;
            }
            assert!(Instant::now() < deadline, "the panel did not answer within 20 s");
        }
    };
    let first = shown(&mut surfaces, &mut doc);
    let Some(Widget::Button { id, .. }) = first.first() else { panic!("the panel starts with its button: {first:?}") };
    surfaces.submit(&owner, vec![UiEvent { id: *id, kind: UiEventKind::Click }], "");
    let after = shown(&mut surfaces, &mut doc);
    assert_eq!((labels(&after), doc.notes.clone(), surfaces.calls(&owner)), (vec!["1 told".to_string()], vec!["told 1".to_string()], 2));
}

fn pinned(surfaces: &mut Surfaces, doc: &mut Doc, plugin: &Arc<Plugin>) -> Arc<super::viewer::Shown> {
    let params = [("pin".to_string(), Wire::Text(doc.values["pin"].clone()))].into();
    let cx = RenderCx { params, instance: 7, ..RenderCx::default() };
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(shown) = surfaces.overlay(doc, plugin, "pin", cx.clone(), [1, 2]) {
            return shown;
        }
        assert!(Instant::now() < deadline, "the overlay did not arrive within 20 s");
        surfaces.frame(doc, Duration::ZERO);
    }
}

fn drag(surfaces: &mut Surfaces, doc: &Doc, shown: &super::viewer::Shown, to: [f64; 2], gesture: &str) {
    let edit = json!({"param": "pin", "gesture": gesture});
    let held = doc.holds(&edit);
    surfaces.drag(shown, "pin", "pin", to, edit, held);
}

#[test]
fn a_drag_reply_that_arrives_after_another_edit_of_its_parameter_leaves_that_edit() {
    let _held = HELD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let (_dir, plugin) = load("test.surfaces-nudge");
    let (mut surfaces, mut doc) = (Surfaces::default(), Doc::default());
    doc.values.insert("pin".into(), "0,0".into());
    let shown = pinned(&mut surfaces, &mut doc, &plugin);
    HOLD.store(true, Ordering::SeqCst);
    drag(&mut surfaces, &doc, &shown, [5.0, 7.0], "drag");
    doc.values.insert("pin".into(), "1,1".into());
    HOLD.store(false, Ordering::SeqCst);
    idle(&mut surfaces, &mut doc);
    let nudged = doc.values["pin"].clone();
    drag(&mut surfaces, &doc, &shown, [3.0, 4.0], "drag");
    idle(&mut surfaces, &mut doc);
    assert_eq!((nudged, doc.values["pin"].as_str()), ("1,1".to_string(), "3,4"));
}

#[test]
fn drags_queued_behind_a_slow_one_each_apply_over_the_value_the_one_before_left() {
    let _held = HELD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let (_dir, plugin) = load("test.surfaces-queued");
    let (mut surfaces, mut doc) = (Surfaces::default(), Doc::default());
    doc.values.insert("pin".into(), "0,0".into());
    let shown = pinned(&mut surfaces, &mut doc, &plugin);
    HOLD.store(true, Ordering::SeqCst);
    for (to, gesture) in [([1.0, 1.0], "one"), ([2.0, 2.0], "two"), ([3.0, 3.0], "three")] {
        drag(&mut surfaces, &doc, &shown, to, gesture);
    }
    HOLD.store(false, Ordering::SeqCst);
    idle(&mut surfaces, &mut doc);
    assert_eq!(doc.values["pin"], "3,3");
}
