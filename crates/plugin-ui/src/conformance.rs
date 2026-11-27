use craft_plugin::{
    Axis, Column, Drag, Dropped, Gap, Item, ListEdit, ListEvent, MenuItem, Pointer, PointerPhase, Span, TextStyle, UiEvent, UiEventKind, Widget,
};
use egui::{Color32, FontFamily};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::{Host, Theme, Triangulate};

#[derive(Default)]
pub struct Bare<'a> {
    pub unanswered: &'a [u64],
}

impl Host for Bare<'_> {
    type Payload = ();

    fn tr(&self, english: &str) -> String {
        english.to_string()
    }
    fn unanswered(&self, id: u64) -> bool {
        self.unanswered.contains(&id)
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
    fn frame(&mut self, ui: &mut egui::Ui, _: &craft_plugin::FrameRequest, size: egui::Vec2) -> Option<egui::Response> {
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, "Frame"));
        ui.painter().rect_filled(rect, 0.0, Color32::DARK_GRAY);
        Some(response)
    }
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
    TypeFocused(&'static str),
    Key(egui::Key),
    Chord(egui::Modifiers, egui::Key),
    Scroll(&'static str, f32),
    Drag(&'static str, &'static str),
    Hold,
    Reply(Reply),
}

pub type Reply = fn(&[Widget], &[UiEvent]) -> Vec<Widget>;

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

pub type Painter = Box<dyn FnMut(&mut egui::Ui, &[Widget], &[u64]) -> Vec<UiEvent>>;

#[derive(Default)]
struct Plugin {
    tree: Vec<Widget>,
    sent: Vec<UiEvent>,
    held: bool,
    queued: Vec<UiEvent>,
    call: Option<Vec<UiEvent>>,
}

impl Plugin {
    fn unanswered(&self) -> Vec<u64> {
        self.queued.iter().chain(self.call.iter().flatten()).map(|event| event.id).collect()
    }

    fn take(&mut self, events: Vec<UiEvent>) {
        self.sent.extend(events.iter().cloned());
        if self.held {
            self.queued.extend(events.into_iter().filter(|event| !matches!(event.kind, UiEventKind::List(ListEvent::Show { .. }))));
            self.call();
        }
    }

    fn call(&mut self) {
        if self.call.is_none() && !self.queued.is_empty() {
            self.call = Some(std::mem::take(&mut self.queued));
        }
    }

    fn answer(&mut self, answer: Reply) {
        if let Some(events) = self.call.take() {
            self.tree = answer(&self.tree, &events);
            self.call();
        }
    }
}

pub fn run(case: &Case, mut paint: Painter) -> Vec<UiEvent> {
    let plugin = Plugin { tree: case.tree.clone(), ..Plugin::default() };
    let mut harness = Harness::builder().with_size(egui::vec2(case.size[0], case.size[1])).with_max_steps(64).build_ui_state(
        move |ui, plugin: &mut Plugin| {
            let unanswered = plugin.unanswered();
            let events = paint(ui, &plugin.tree, &unanswered);
            plugin.take(events);
        },
        plugin,
    );
    harness.run();
    harness.state_mut().sent.clear();
    for step in &case.steps {
        match step {
            Step::Click(name) => harness.get_by_label(name).click(),
            Step::DoubleClick(name) => {
                let at = harness.get_by_label(name).rect().center();
                let button =
                    |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
                harness.input_mut().events.extend([egui::Event::PointerMoved(at), button(true), button(false), button(true), button(false)]);
            }
            Step::RightClick(name) => harness.get_by_label(name).click_secondary(),
            Step::Type(name, text) => {
                harness.get_by_label(name).focus();
                harness.run();
                harness.get_by_label(name).type_text(text);
            }
            Step::TypeFocused(text) => harness.event(egui::Event::Text(text.to_string())),
            Step::Key(key) => harness.key_press(*key),
            Step::Chord(modifiers, key) => harness.key_press_modifiers(*modifiers, *key),
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
            Step::Drag(from, to) => {
                let start = harness.get_by_label(from).rect().center();
                let target = harness.get_by_label(to).rect();
                let end = egui::pos2(target.center().x, target.top() + target.height() / 4.0);
                let button =
                    |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
                harness.event(egui::Event::PointerMoved(start));
                harness.run();
                harness.event(button(start, true));
                harness.run();
                for step in 1..=4 {
                    harness.event(egui::Event::PointerMoved(start.lerp(end, step as f32 / 4.0)));
                    harness.run();
                }
                harness.event(button(end, false));
            }
            Step::Hold => harness.state_mut().held = true,
            Step::Reply(answer) => harness.state_mut().answer(*answer),
        }
        harness.run();
    }
    harness.run();
    std::mem::take(&mut harness.state_mut().sent)
}

pub fn run_all(mut painter: impl FnMut() -> Painter) -> Vec<String> {
    cases().iter().filter_map(|case| case.check(&run(case, painter())).err()).collect()
}

fn event(id: u64, kind: UiEventKind) -> UiEvent {
    UiEvent { id, kind }
}

fn list(count: u32, first: u32, shown: u32, selected: Vec<u32>) -> Widget {
    let items = (first..first + shown).map(|row| Item::new(&format!("file {row}"))).collect();
    Widget::List { id: 4, columns: vec![], count, first, items, selected, multi: false, rows: 6, edit: Default::default() }
}

fn tree(items: Vec<Item>) -> Widget {
    Widget::List {
        id: 4,
        columns: vec![],
        count: items.len() as u32,
        first: 0,
        items,
        selected: vec![],
        multi: false,
        rows: 6,
        edit: Default::default(),
    }
}

fn editable(mut widget: Widget, allowed: ListEdit) -> Widget {
    if let Widget::List { edit, .. } = &mut widget {
        *edit = allowed;
    }
    widget
}

fn renamable() -> Widget {
    editable(list(3, 0, 3, vec![]), ListEdit { rename: true, ..Default::default() })
}

fn reorderable(selected: Vec<u32>) -> Widget {
    editable(list(5, 0, 5, selected), ListEdit { reorder: true, ..Default::default() })
}

fn sortable() -> Widget {
    let mut widget = editable(list(3, 0, 3, vec![]), ListEdit { sortable: true, ..Default::default() });
    if let Widget::List { columns, .. } = &mut widget {
        *columns = ["Name", "Size"].map(|title| Column { title: title.into(), ..Default::default() }).to_vec();
    }
    widget
}

fn wide_table() -> Widget {
    let mut widget = sortable();
    if let Widget::List { columns, .. } = &mut widget {
        *columns = (0..17).map(|at| Column { title: format!("Col {at}"), ..Default::default() }).collect();
    }
    widget
}

fn list_event(kind: ListEvent) -> UiEvent {
    event(4, UiEventKind::List(kind))
}

fn multi(mut widget: Widget) -> Widget {
    if let Widget::List { multi, .. } = &mut widget {
        *multi = true;
    }
    widget
}

fn select(rows: &[u32]) -> UiEvent {
    event(4, UiEventKind::List(ListEvent::Select(rows.to_vec())))
}

fn ends_on_the_last_row_and_asks_for_its_window(events: &[UiEvent]) -> bool {
    matches!(
        events,
        [first, UiEvent { id: 4, kind: UiEventKind::List(ListEvent::Show { first: shown, count }) }]
            if *first == select(&[99_999]) && *shown > 99_000 && shown + count == 100_000
    )
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

fn button(id: u64, text: &str) -> Widget {
    Widget::Button { id, text: text.into(), enabled: true }
}

fn item(text: &str) -> MenuItem {
    MenuItem { text: text.into(), ..Default::default() }
}

fn menu(id: u64, text: Option<&str>) -> Widget {
    let items = vec![
        item("Rename"),
        MenuItem { checked: Some(false), ..item("Pinned") },
        MenuItem { separator: true, ..Default::default() },
        MenuItem { items: vec![item("By name"), item("By date")], ..item("Sort") },
        MenuItem { disabled: true, ..item("Delete") },
        item("Close"),
    ];
    Widget::Menu { id, text: text.map(str::to_string), items }
}

fn pad() -> Vec<Widget> {
    vec![Widget::Canvas { id: 9, size: [20.0, 40.0], shapes: vec![] }]
}

fn at(phase: PointerPhase) -> Pointer {
    Pointer::new([10.0, 20.0], phase)
}

fn radio() -> Widget {
    Widget::Radio { id: 13, text: "Mode".into(), options: vec!["Plain".into(), "Soft".into(), "Hard".into()], selected: 0 }
}

fn notes(value: &str) -> Widget {
    Widget::TextArea { id: 14, value: value.into(), hint: "Notes".into(), rows: 4 }
}

fn upper_case(_: &[Widget], call: &[UiEvent]) -> Vec<Widget> {
    let typed = call.iter().rev().find_map(|event| match &event.kind {
        UiEventKind::Text(text) => Some(text.to_uppercase()),
        _ => None,
    });
    vec![notes(&typed.unwrap_or_default())]
}

fn refuses(tree: &[Widget], _: &[UiEvent]) -> Vec<Widget> {
    tree.to_vec()
}

fn sorts(tree: &[Widget], call: &[UiEvent]) -> Vec<Widget> {
    let mut tree = tree.to_vec();
    for event in call {
        if let (UiEventKind::List(ListEvent::Sort { column, descending }), Some(Widget::List { edit, .. })) = (&event.kind, tree.first_mut()) {
            edit.sorted = Some((*column, *descending));
        }
    }
    tree
}

fn speed(value: f64) -> Widget {
    Widget::Slider { id: 5, text: "Speed".into(), value, min: 0.0, max: 10.0 }
}

fn takes_the_first_value(_: &[Widget], call: &[UiEvent]) -> Vec<Widget> {
    let first = call.iter().find_map(|event| match event.kind {
        UiEventKind::Number(value) => Some(value),
        _ => None,
    });
    vec![speed(first.unwrap_or_default())]
}

fn each_value_above_the_last(events: &[UiEvent]) -> bool {
    let values: Vec<f64> = events
        .iter()
        .filter_map(|event| match event {
            UiEvent { id: 5, kind: UiEventKind::Number(value) } => Some(*value),
            _ => None,
        })
        .collect();
    values.len() == 3 && values.len() == events.len() && values.windows(2).all(|pair| pair[1] > pair[0])
}

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "an icon button is clicked by its tooltip text and sends one Click",
            size: [200.0, 80.0],
            tree: vec![Widget::IconButton { id: 11, icon: "trash".into(), text: "Delete".into(), enabled: true }],
            steps: vec![Step::Click("Delete")],
            expect: Expect::Events(vec![event(11, UiEventKind::Click)]),
        },
        Case {
            name: "clicking an image with no upload yet, in a grid cell under a scroll area, sends one Click with its id",
            size: [200.0, 120.0],
            tree: vec![Widget::Scroll {
                id: 7,
                height: 100.0,
                rows: None,
                children: vec![Widget::Grid {
                    id: 8,
                    columns: 3,
                    children: vec![Widget::Column {
                        children: vec![Widget::Image { id: 9, key: "thumb/0".into(), size: [40.0, 30.0] }, Widget::Tooltip { text: "Linear".into() }],
                    }],
                }],
            }],
            steps: vec![Step::Click("thumb/0")],
            expect: Expect::Events(vec![event(9, UiEventKind::Click)]),
        },
        Case {
            name: "clicking an off toggle sends Bool(true)",
            size: [200.0, 80.0],
            tree: vec![Widget::Toggle { id: 12, text: "Live".into(), value: false }],
            steps: vec![Step::Click("Live")],
            expect: Expect::Events(vec![event(12, UiEventKind::Bool(true))]),
        },
        Case {
            name: "clicking a radio choice sends its index",
            size: [300.0, 80.0],
            tree: vec![radio()],
            steps: vec![Step::Click("Hard")],
            expect: Expect::Events(vec![event(13, UiEventKind::Number(2.0))]),
        },
        Case {
            name: "the keyboard alone picks the next radio choice: Right, then Space",
            size: [300.0, 80.0],
            tree: vec![radio()],
            steps: vec![Step::Key(egui::Key::Tab), Step::Key(egui::Key::ArrowRight), Step::Key(egui::Key::Space)],
            expect: Expect::Holds("one or more Number(1), and no other choice", |events| {
                !events.is_empty() && events.iter().all(|event| *event == UiEvent { id: 13, kind: UiEventKind::Number(1.0) })
            }),
        },
        Case {
            name: "typing in a text area sends its text and Enter adds a new line",
            size: [300.0, 160.0],
            tree: vec![Widget::TextArea { id: 14, value: String::new(), hint: "Notes".into(), rows: 4 }],
            steps: vec![Step::Type("Notes", "ab"), Step::Key(egui::Key::Enter)],
            expect: Expect::Holds("a first Text \"ab\" and a last Text ending in a new line", |events| {
                let texts: Vec<&str> = events
                    .iter()
                    .filter_map(|event| match event {
                        UiEvent { id: 14, kind: UiEventKind::Text(text) } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();
                texts.len() == events.len() && texts.first() == Some(&"ab") && texts.last().is_some_and(|text| text.ends_with('\n'))
            }),
        },
        Case {
            name: "a paragraph and a gap leave the button after them clickable",
            size: [300.0, 160.0],
            tree: vec![
                Widget::Text {
                    spans: vec![Span { text: "Two ".into(), style: TextStyle::Body }, Span { text: "styles".into(), style: TextStyle::Strong }],
                },
                Widget::Gap { gap: Gap::Large },
                button(15, "Apply"),
            ],
            steps: vec![Step::Click("Apply")],
            expect: Expect::Events(vec![event(15, UiEventKind::Click)]),
        },
        Case {
            name: "Name gives the widget before it the accessible name it is clicked by",
            size: [200.0, 80.0],
            tree: vec![Widget::IconButton { id: 16, icon: "x".into(), text: "X".into(), enabled: true }, Widget::Name { text: "Remove clip".into() }],
            steps: vec![Step::Click("Remove clip")],
            expect: Expect::Events(vec![event(16, UiEventKind::Click)]),
        },
        Case {
            name: "Focus puts the keyboard on the widget before it, so Enter clicks that one",
            size: [200.0, 120.0],
            tree: vec![button(17, "First"), button(18, "Second"), Widget::Focus { call: 3 }],
            steps: vec![Step::Key(egui::Key::Enter)],
            expect: Expect::Events(vec![event(18, UiEventKind::Click)]),
        },
        Case {
            name: "a submenu item sends its depth-first index, counting separators and submenus",
            size: [400.0, 300.0],
            tree: vec![menu(19, Some("More"))],
            steps: vec![Step::Click("More"), Step::Click("Sort"), Step::Click("By date")],
            expect: Expect::Events(vec![event(19, UiEventKind::Number(5.0))]),
        },
        Case {
            name: "right-clicking a widget opens its context menu and an item after a submenu keeps its index",
            size: [400.0, 300.0],
            tree: vec![button(20, "Clip"), menu(21, None)],
            steps: vec![Step::RightClick("Clip"), Step::Click("Close")],
            expect: Expect::Events(vec![event(21, UiEventKind::Number(7.0))]),
        },
        Case {
            name: "right-clicking a list row selects it and opens the context menu that follows the list",
            size: [400.0, 300.0],
            tree: vec![list(3, 0, 3, vec![]), menu(21, None)],
            steps: vec![Step::RightClick("file 1"), Step::Click("Rename")],
            expect: Expect::Events(vec![select(&[1]), event(21, UiEventKind::Number(0.0))]),
        },
        Case {
            name: "the Menu key opens the context menu of the focused widget",
            size: [400.0, 300.0],
            tree: vec![button(20, "Clip"), menu(21, None)],
            steps: vec![Step::Key(egui::Key::Tab), Step::Key(egui::Key::Menu), Step::Click("Rename")],
            expect: Expect::Events(vec![event(21, UiEventKind::Number(0.0))]),
        },
        Case {
            name: "both panes of a split take clicks",
            size: [300.0, 120.0],
            tree: vec![Widget::Split { id: 22, axis: Axis::Horizontal, first: vec![button(1, "Left")], second: vec![button(2, "Right")] }],
            steps: vec![Step::Click("Left"), Step::Click("Right")],
            expect: Expect::Events(vec![event(1, UiEventKind::Click), event(2, UiEventKind::Click)]),
        },
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
            name: "Down after a click selects the next row",
            size: [240.0, 200.0],
            tree: vec![list(5, 0, 5, vec![])],
            steps: vec![Step::Click("file 1"), Step::Key(egui::Key::ArrowDown)],
            expect: Expect::Events(vec![select(&[1]), select(&[2])]),
        },
        Case {
            name: "End on a long list selects the last row and asks for the rows around it",
            size: [240.0, 200.0],
            tree: vec![list(100_000, 0, 128, vec![])],
            steps: vec![Step::Click("file 2"), Step::Key(egui::Key::End)],
            expect: Expect::Holds("Select([2]), Select([99999]) then one Show ending at the last row", |events| {
                events.first() == Some(&select(&[2])) && ends_on_the_last_row_and_asks_for_its_window(&events[1..])
            }),
        },
        Case {
            name: "Right on a shut tree row opens it",
            size: [240.0, 200.0],
            tree: vec![tree(vec![Item::new("Shots").open(false), Item::new("Notes")])],
            steps: vec![Step::Click("Shots"), Step::Key(egui::Key::ArrowRight)],
            expect: Expect::Events(vec![select(&[0]), event(4, UiEventKind::List(ListEvent::Toggle { row: 0, open: true }))]),
        },
        Case {
            name: "clicking the disclosure triangle of a shut row opens it without selecting it",
            size: [240.0, 200.0],
            tree: vec![tree(vec![Item::new("Shots").open(false), Item::new("Notes")])],
            steps: vec![Step::Click("Expand Shots")],
            expect: Expect::Events(vec![event(4, UiEventKind::List(ListEvent::Toggle { row: 0, open: true }))]),
        },
        Case {
            name: "Left on a child row selects its parent, on a shut row its parent again, and on the open root closes it",
            size: [240.0, 200.0],
            tree: vec![tree(vec![Item::new("Shots").open(true), Item::new("shot 1").depth(1).open(false), Item::new("take 1").depth(2)])],
            steps: vec![Step::Click("take 1"), Step::Key(egui::Key::ArrowLeft), Step::Key(egui::Key::ArrowLeft), Step::Key(egui::Key::ArrowLeft)],
            expect: Expect::Events(vec![
                select(&[2]),
                select(&[1]),
                select(&[0]),
                event(4, UiEventKind::List(ListEvent::Toggle { row: 0, open: false })),
            ]),
        },
        Case {
            name: "Shift+Down extends a multi selection from the clicked row",
            size: [240.0, 200.0],
            tree: vec![multi(list(5, 0, 5, vec![]))],
            steps: vec![
                Step::Click("file 1"),
                Step::Chord(egui::Modifiers::SHIFT, egui::Key::ArrowDown),
                Step::Chord(egui::Modifiers::SHIFT, egui::Key::ArrowDown),
            ],
            expect: Expect::Events(vec![select(&[1]), select(&[1, 2]), select(&[1, 2, 3])]),
        },
        Case {
            name: "right-clicking an unselected row selects it, and right-clicking a selected row keeps the selection",
            size: [240.0, 200.0],
            tree: vec![list(5, 0, 5, vec![3])],
            steps: vec![Step::RightClick("file 2"), Step::RightClick("file 3")],
            expect: Expect::Events(vec![select(&[2])]),
        },
        Case {
            name: "F2 on the focused row opens its name for editing, and typing and Enter rename it",
            size: [240.0, 200.0],
            tree: vec![renamable()],
            steps: vec![Step::Key(egui::Key::Tab), Step::Key(egui::Key::F2), Step::TypeFocused("New"), Step::Key(egui::Key::Enter)],
            expect: Expect::Events(vec![list_event(ListEvent::Rename { row: 0, text: "New".into() })]),
        },
        Case {
            name: "Escape in the name field cancels the rename and sends nothing",
            size: [240.0, 200.0],
            tree: vec![renamable()],
            steps: vec![Step::Key(egui::Key::Tab), Step::Key(egui::Key::F2), Step::TypeFocused("New"), Step::Key(egui::Key::Escape)],
            expect: Expect::Events(vec![]),
        },
        Case {
            name: "dragging the selected row above the first row moves it to the top",
            size: [240.0, 200.0],
            tree: vec![reorderable(vec![2])],
            steps: vec![Step::Drag("file 2", "file 0")],
            expect: Expect::Events(vec![list_event(ListEvent::Move { rows: vec![2], to: 0 })]),
        },
        Case {
            name: "a row dragged out of the list and let go elsewhere moves nothing",
            size: [240.0, 260.0],
            tree: vec![reorderable(vec![2]), button(23, "Elsewhere")],
            steps: vec![Step::Drag("file 2", "Elsewhere")],
            expect: Expect::Events(vec![]),
        },
        Case {
            name: "a rename is kept when the name field loses the focus to Tab",
            size: [240.0, 200.0],
            tree: vec![renamable()],
            steps: vec![Step::Key(egui::Key::Tab), Step::Key(egui::Key::F2), Step::TypeFocused("New"), Step::Key(egui::Key::Tab)],
            expect: Expect::Events(vec![list_event(ListEvent::Rename { row: 0, text: "New".into() })]),
        },
        Case {
            name: "a scroll area sized in rows holds a button that can be clicked",
            size: [240.0, 200.0],
            tree: vec![Widget::Scroll {
                id: 24,
                height: 0.0,
                rows: Some(3),
                children: vec![button(25, "One"), button(26, "Two"), button(27, "Three")],
            }],
            steps: vec![Step::Click("Three")],
            expect: Expect::Events(vec![event(27, UiEventKind::Click)]),
        },
        Case {
            name: "Alt+Down moves the selected row below the next one",
            size: [240.0, 200.0],
            tree: vec![reorderable(vec![1])],
            steps: vec![Step::Key(egui::Key::Tab), Step::Chord(egui::Modifiers::ALT, egui::Key::ArrowDown)],
            expect: Expect::Events(vec![list_event(ListEvent::Move { rows: vec![1], to: 3 })]),
        },
        Case {
            name: "the seventeenth column of a table has a heading that sorts by it",
            size: [2400.0, 200.0],
            tree: vec![wide_table()],
            steps: vec![Step::Click("Col 16")],
            expect: Expect::Events(vec![list_event(ListEvent::Sort { column: 16, descending: false })]),
        },
        Case {
            name: "clicking a column heading sorts by it, and once the plugin sorts, clicking it again flips the order",
            size: [240.0, 200.0],
            tree: vec![sortable()],
            steps: vec![Step::Hold, Step::Click("Size"), Step::Reply(sorts), Step::Click("Size")],
            expect: Expect::Events(vec![
                list_event(ListEvent::Sort { column: 1, descending: false }),
                list_event(ListEvent::Sort { column: 1, descending: true }),
            ]),
        },
        Case {
            name: "a sort the plugin refuses shows the plugin's order once it answers, so the next click asks for the same sort again",
            size: [240.0, 200.0],
            tree: vec![sortable()],
            steps: vec![Step::Hold, Step::Click("Size"), Step::Reply(refuses), Step::Click("Size")],
            expect: Expect::Events(vec![
                list_event(ListEvent::Sort { column: 1, descending: false }),
                list_event(ListEvent::Sort { column: 1, descending: false }),
            ]),
        },
        Case {
            name: "text typed during a slow call is never replaced by an older reply, and the plugin's own text shows once it has answered every letter",
            size: [300.0, 160.0],
            tree: vec![notes("")],
            steps: vec![
                Step::Hold,
                Step::Type("Notes", "a"),
                Step::TypeFocused("b"),
                Step::Reply(upper_case),
                Step::TypeFocused("c"),
                Step::Reply(upper_case),
                Step::Reply(upper_case),
                Step::TypeFocused("d"),
            ],
            expect: Expect::Events(["a", "ab", "abc", "ABCd"].map(|text| event(14, UiEventKind::Text(text.into()))).to_vec()),
        },
        Case {
            name: "a slider moved during a slow call does not jump back to the value of an older reply",
            size: [300.0, 80.0],
            tree: vec![speed(0.0)],
            steps: vec![
                Step::Hold,
                Step::Key(egui::Key::Tab),
                Step::Key(egui::Key::ArrowRight),
                Step::Key(egui::Key::ArrowRight),
                Step::Reply(takes_the_first_value),
                Step::Key(egui::Key::ArrowRight),
            ],
            expect: Expect::Holds("three Numbers, each above the one before", each_value_above_the_last),
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
        Case {
            name: "an app-drawn frame is clicked by its accessible name and sends one Click",
            size: [240.0, 140.0],
            tree: vec![Widget::Frame {
                id: 7,
                request: craft_plugin::FrameRequest { time: 1.0, scale: 0.5, ..Default::default() },
                size: [160.0, 90.0],
            }],
            steps: vec![Step::Click("Frame")],
            expect: Expect::Events(vec![event(7, UiEventKind::Click)]),
        },
        Case {
            name: "a right-click on a canvas sends Context at the pointer",
            size: [200.0, 80.0],
            tree: pad(),
            steps: vec![Step::RightClick("Plug-in drawing")],
            expect: Expect::Events(vec![
                event(9, UiEventKind::Pointer(at(PointerPhase::Hover))),
                event(9, UiEventKind::Context(at(PointerPhase::Down))),
            ]),
        },
        Case {
            name: "a double click on a canvas sends its downs and ups, then Open",
            size: [200.0, 80.0],
            tree: pad(),
            steps: vec![Step::DoubleClick("Plug-in drawing")],
            expect: Expect::Events(
                [PointerPhase::Hover, PointerPhase::Down, PointerPhase::Up, PointerPhase::Down, PointerPhase::Up]
                    .map(|phase| event(9, UiEventKind::Pointer(at(phase))))
                    .into_iter()
                    .chain([event(9, UiEventKind::Open(at(PointerPhase::Up)))])
                    .collect(),
            ),
        },
    ]
}
