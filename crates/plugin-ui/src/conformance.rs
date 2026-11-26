use craft_plugin::{Drag, Dropped, Item, ListEvent, TextStyle, UiEvent, UiEventKind, Widget};
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
        .with_max_steps(64)
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

fn list(count: u32, first: u32, shown: u32, selected: Vec<u32>) -> Widget {
    let items = (first..first + shown).map(|row| Item::new(&format!("file {row}"))).collect();
    Widget::List { id: 4, columns: vec![], count, first, items, selected, multi: false, rows: 6 }
}

fn shows_a_later_window(events: &[UiEvent]) -> bool {
    let shows: Vec<(u32, u32)> = events
        .iter()
        .filter_map(|event| match event {
            UiEvent { id: 4, kind: UiEventKind::List(ListEvent::Show { first, count }) } => Some((*first, *count)),
            _ => None,
        })
        .collect();
    shows.len() == events.len()
        && (1..=3).contains(&shows.len())
        && shows.last().is_some_and(|(first, count)| *first > 100 && *count >= 6 && first + count <= 100_000)
}

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "a button click is one Click on its id",
            size: [200.0, 80.0],
            tree: vec![Widget::Button { id: 9, text: "Apply".into(), enabled: true }],
            steps: vec![Step::Click("Apply")],
            expect: Expect::Events(vec![event(9, UiEventKind::Click)]),
        },
        Case {
            name: "clicking a list row selects it, and Enter then activates it",
            size: [240.0, 200.0],
            tree: vec![list(3, 0, 3, vec![])],
            steps: vec![Step::Click("file 1"), Step::Key(egui::Key::Enter)],
            expect: Expect::Events(vec![
                event(4, UiEventKind::List(ListEvent::Select(vec![1]))),
                event(4, UiEventKind::List(ListEvent::Activate(1))),
            ]),
        },
        Case {
            name: "scrolling a long list past its built rows asks a few times, not every frame, for the rows now in view",
            size: [240.0, 200.0],
            tree: vec![list(100_000, 0, 128, vec![])],
            steps: vec![Step::Scroll("file 2", 20_000.0)],
            expect: Expect::Holds("one to three Shows, the last a window after row 100 inside the list", shows_a_later_window),
        },
        Case {
            name: "a status item one row high sends its button's Click",
            size: [240.0, 22.0],
            tree: vec![Widget::Row {
                children: vec![
                    Widget::Button { id: 3, text: "Ping".into(), enabled: true },
                    Widget::Label { text: "0 pings".into(), style: TextStyle::Body },
                ],
            }],
            steps: vec![Step::Click("Ping")],
            expect: Expect::Events(vec![event(3, UiEventKind::Click)]),
        },
        Case {
            name: "a tool option bar row toggles its checkbox",
            size: [400.0, 26.0],
            tree: vec![Widget::Row {
                children: vec![
                    Widget::Checkbox { id: 6, text: "Soft".into(), value: false },
                    Widget::Label { text: "hard stamp".into(), style: TextStyle::Body },
                ],
            }],
            steps: vec![Step::Click("Soft")],
            expect: Expect::Events(vec![event(6, UiEventKind::Bool(true))]),
        },
    ]
}
