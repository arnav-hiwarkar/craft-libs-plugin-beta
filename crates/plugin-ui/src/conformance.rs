use craft_plugin::{Drag, Dropped, UiEvent, UiEventKind, Widget};
use egui::{Color32, FontFamily};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::{Host, Theme, Triangulate};

pub struct Bare;

impl Host for Bare {
    type Payload = ();

    fn tr(&self, english: &str) -> String {
        english.to_string()
    }
    fn register(&mut self, _: &str, _: &egui::Response, _: &str) {}
    fn texture(&mut self, _: &egui::Context, _: &str) -> Option<egui::TextureId> {
        None
    }
    fn triangulate(&self) -> Triangulate {
        |_| Vec::new()
    }
    fn drag(&self, _: &Drag) -> Option<()> {
        None
    }
    fn dropped(&self, _: &()) -> Option<Dropped> {
        None
    }
    fn accept_files(&mut self, _: &[String]) {}
}

pub fn theme() -> Theme {
    let grey = |v| Color32::from_gray(v);
    Theme {
        text: Color32::WHITE,
        weak: Color32::GRAY,
        accent: Color32::LIGHT_BLUE,
        background: Color32::BLACK,
        surface: Color32::DARK_GRAY,
        border: Color32::GRAY,
        error: Color32::RED,
        good: Color32::GREEN,
        warn: Color32::YELLOW,
        faint: grey(90),
        selected: Color32::DARK_BLUE,
        series: [grey(40), grey(60), grey(80), grey(100), grey(120), grey(140), grey(160), grey(180)],
        heading: FontFamily::Proportional,
    }
}

#[derive(Clone, Debug)]
pub enum Step {
    Click(&'static str),
    DoubleClick(&'static str),
    RightClick(&'static str),
    Type(&'static str, &'static str),
    Key(egui::Key),
    Scroll(&'static str, f32),
}

pub enum Expect {
    Events(Vec<UiEvent>),
    Holds(&'static str, fn(&[UiEvent]) -> bool),
}

pub struct Case {
    pub name: &'static str,
    pub size: [f32; 2],
    pub tree: Vec<Widget>,
    pub steps: Vec<Step>,
    pub expect: Expect,
}

impl Case {
    pub fn check(&self, events: &[UiEvent]) -> Result<(), String> {
        match &self.expect {
            Expect::Events(wanted) if wanted == events => Ok(()),
            Expect::Events(wanted) => Err(format!("{}: wanted {wanted:?}, the painter sent {events:?}", self.name)),
            Expect::Holds(_, holds) if holds(events) => Ok(()),
            Expect::Holds(what, _) => Err(format!("{}: wanted {what}, the painter sent {events:?}", self.name)),
        }
    }
}

pub type Painter = Box<dyn FnMut(&mut egui::Ui, &[Widget]) -> Vec<UiEvent>>;

pub fn run(case: &Case, mut paint: Painter) -> Vec<UiEvent> {
    let tree = case.tree.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(case.size[0], case.size[1]))
        .build_ui_state(move |ui, events: &mut Vec<UiEvent>| events.extend(paint(ui, &tree)), Vec::new());
    harness.run();
    harness.state_mut().clear();
    for step in &case.steps {
        match step {
            Step::Click(name) => harness.get_by_label(name).click(),
            Step::DoubleClick(name) => {
                let node = harness.get_by_label(name);
                node.click();
                node.click();
            }
            Step::RightClick(name) => harness.get_by_label(name).click_secondary(),
            Step::Type(name, text) => {
                harness.get_by_label(name).focus();
                harness.run();
                harness.get_by_label(name).type_text(text);
            }
            Step::Key(key) => harness.key_press(*key),
            Step::Scroll(name, delta) => {
                harness.get_by_label(name).hover();
                harness.run();
                harness.event(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -*delta),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: Default::default(),
                });
            }
        }
        harness.run();
    }
    harness.run();
    std::mem::take(harness.state_mut())
}

pub fn run_all(mut painter: impl FnMut() -> Painter) -> Vec<String> {
    cases().iter().filter_map(|case| case.check(&run(case, painter())).err()).collect()
}

fn event(id: u64, kind: UiEventKind) -> UiEvent {
    UiEvent { id, kind }
}

pub fn cases() -> Vec<Case> {
    vec![Case {
        name: "a button click is one Click on its id",
        size: [200.0, 80.0],
        tree: vec![Widget::Button { id: 9, text: "Apply".into(), enabled: true }],
        steps: vec![Step::Click("Apply")],
        expect: Expect::Events(vec![event(9, UiEventKind::Click)]),
    }]
}
