use std::cell::RefCell;
use std::rc::Rc;

use craft_plugin::{TextStyle, UiEvent, Widget};
use craft_ui::testing::Fixture;
use egui::Vec2;

#[derive(Default)]
pub struct Plugged {
    pub accepted: Vec<String>,
    pub keys: Vec<String>,
    pub pressed: Rc<RefCell<Vec<String>>>,
}

impl craft_plugin_ui_framework::Host for Plugged {
    fn drag(&self, drag: &craft_plugin::Drag) -> Option<craft_plugin::Dropped> {
        match drag {
            craft_plugin::Drag::Data { data } => Some(craft_plugin::Dropped::Data { plugin: "test.painter".into(), data: data.clone() }),
            craft_plugin::Drag::File { .. } => None,
        }
    }

    fn accept_files(&mut self, paths: &[String]) {
        self.accepted.extend_from_slice(paths);
    }

    fn keys(&self) -> &[String] {
        &self.keys
    }

    fn pressed(&mut self, chord: &str) {
        self.pressed.borrow_mut().push(chord.to_string());
    }
}

pub fn shown(tree: Vec<Widget>) -> (Fixture, Rc<RefCell<Vec<UiEvent>>>) {
    shown_with(tree, Plugged::default())
}

pub fn shown_with(tree: Vec<Widget>, mut host: Plugged) -> (Fixture, Rc<RefCell<Vec<UiEvent>>>) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&events);
    let mut fixture = Fixture::new(move |ui| seen.borrow_mut().extend(craft_plugin_ui_framework::draw(ui, &tree, "painter", &mut host)));
    fixture.room(Vec2::new(480.0, 360.0));
    fixture.settle();
    events.borrow_mut().clear();
    (fixture, events)
}

fn label(text: &str) -> Widget {
    Widget::Label { text: text.into(), style: TextStyle::Body }
}

#[test]
fn a_grid_lays_its_children_row_by_row_over_the_column_count_the_plugin_chose() {
    let children = ["a", "b", "c", "d", "e", "f"].map(label).to_vec();
    let (fixture, _) = shown(vec![Widget::Grid { id: 3, columns: 3, children }]);
    let at = |name: &str| fixture.by_label(name).unwrap_or_else(|| panic!("`{name}` is drawn")).rect;
    let (a, b, c, d) = (at("a"), at("b"), at("c"), at("d"));
    assert!(a.left() < b.left() && b.left() < c.left(), "the first three cells stand side by side");
    assert!((a.center().y - c.center().y).abs() < 1.0, "the first row is level");
    assert!(d.top() >= a.bottom(), "the fourth cell starts the second row");
    assert!((d.left() - a.left()).abs() < 1.0, "the second row starts under the first column");
}

fn canvas() -> Widget {
    Widget::Canvas {
        id: 5,
        size: [200.0, 100.0],
        shapes: vec![craft_plugin::Shape::Circle { center: [100.0, 50.0], radius: 10.0, fill: Some(craft_plugin::Paint::Accent), stroke: None }],
    }
}

fn pointers(events: &[UiEvent]) -> Vec<craft_plugin::Pointer> {
    events
        .iter()
        .filter_map(|event| match event {
            UiEvent { id: 5, kind: craft_plugin::UiEventKind::Pointer(pointer) } if pointer.phase != craft_plugin::PointerPhase::Hover => {
                Some(*pointer)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_drag_over_a_plugin_canvas_comes_out_as_down_drags_and_up_in_the_canvas_own_coordinates() {
    let (mut fixture, events) = shown(vec![canvas()]);
    let area = fixture.by_label("Plug-in drawing").expect("the canvas has its node");
    fixture.drag(&area, Vec2::new(area.rect.width() / 4.0, 0.0));
    let samples = pointers(&events.borrow());
    let phases: Vec<_> = samples.iter().map(|pointer| pointer.phase).collect();
    assert_eq!(phases.first(), Some(&craft_plugin::PointerPhase::Down), "{phases:?}");
    assert_eq!(phases.last(), Some(&craft_plugin::PointerPhase::Up), "{phases:?}");
    assert!(phases.contains(&craft_plugin::PointerPhase::Drag), "{phases:?}");
    let (down, up) = (samples[0].pos, samples[samples.len() - 1].pos);
    assert!((down[0] - 100.0).abs() < 1.0 && (down[1] - 50.0).abs() < 1.0, "the press at the area's middle is the canvas's middle: {down:?}");
    assert!((up[0] - 150.0).abs() < 1.0 && (up[1] - 50.0).abs() < 1.0, "a quarter of the area's width is a quarter of the canvas's: {up:?}");
}

#[test]
fn a_key_pressed_while_the_canvas_has_the_focus_reaches_the_plugin_with_its_modifiers() {
    let (mut fixture, events) = shown(vec![canvas()]);
    let area = fixture.by_label("Plug-in drawing").expect("the canvas has its node");
    fixture.click(&area);
    events.borrow_mut().clear();
    fixture.key(egui::Key::A, egui::Modifiers::SHIFT);
    let keys: Vec<_> = events
        .borrow()
        .iter()
        .filter_map(|event| if let craft_plugin::UiEventKind::Key(key) = &event.kind { Some(key.clone()) } else { None })
        .collect();
    let shift = craft_plugin::Modifiers { shift: true, ..Default::default() };
    assert_eq!(keys, vec![craft_plugin::Key { name: "A".into(), modifiers: shift }]);
}

fn item(text: &str) -> craft_plugin::MenuItem {
    craft_plugin::MenuItem { text: text.into(), ..Default::default() }
}

#[test]
fn picking_a_submenu_row_sends_its_depth_first_index() {
    let more = craft_plugin::MenuItem { items: vec![item("Inner A"), item("Inner B")], ..item("More") };
    let separator = craft_plugin::MenuItem { separator: true, ..Default::default() };
    let menu = Widget::Menu { id: 8, text: Some("Actions".into()), items: vec![item("First"), separator, more, item("Last")] };
    let (mut fixture, events) = shown(vec![menu]);
    let opener = fixture.by_label("Actions").expect("the menu button");
    fixture.click(&opener);
    fixture.settle();
    let head = fixture.by_label("More").expect("the open menu shows its submenu row");
    fixture.click(&head);
    fixture.settle();
    let inner = fixture.by_label("Inner B").expect("the submenu opens");
    fixture.click(&inner);
    fixture.settle();
    assert_eq!(*events.borrow(), vec![UiEvent { id: 8, kind: craft_plugin::UiEventKind::Number(4.0) }]);
}

#[test]
fn dragging_a_plugin_drag_source_onto_its_drop_target_drops_the_data_the_host_made_of_it() {
    let swatch = Widget::DragSource { id: 30, drag: craft_plugin::Drag::Data { data: "red".into() }, children: vec![label("Red swatch")] };
    let well = Widget::DropTarget { id: 31, children: vec![label("Colour well")] };
    let (mut fixture, events) = shown(vec![swatch, label("between"), well]);
    let from = fixture.by_label("Red swatch").expect("the source's child").rect.center();
    let to = fixture.by_label("Colour well").expect("the target's child").rect.center();
    fixture.drag_hold_at(from, to - from);
    fixture.release_at(to);
    fixture.settle();
    let dropped = craft_plugin::Dropped::Data { plugin: "test.painter".into(), data: "red".into() };
    assert_eq!(*events.borrow(), vec![UiEvent { id: 31, kind: craft_plugin::UiEventKind::Drop(dropped) }]);
}

#[test]
fn a_drag_source_the_host_cannot_carry_draws_its_children_and_drags_nothing() {
    let file = Widget::DragSource {
        id: 32,
        drag: craft_plugin::Drag::File { root: craft_plugin::Root::Data, path: "a.txt".into() },
        children: vec![label("a.txt")],
    };
    let well = Widget::DropTarget { id: 33, children: vec![label("Inbox")] };
    let (mut fixture, events) = shown(vec![file, label("between"), well]);
    let from = fixture.by_label("a.txt").expect("the child shows").rect.center();
    let to = fixture.by_label("Inbox").expect("the target's child").rect.center();
    fixture.drag_hold_at(from, to - from);
    fixture.release_at(to);
    fixture.settle();
    assert_eq!(*events.borrow(), vec![]);
}

#[test]
fn a_gap_and_an_old_space_take_the_gap_tokens_between_their_neighbours() {
    let (fixture, _) =
        shown(vec![label("one"), Widget::Gap { gap: craft_plugin::Gap::Large }, label("two"), Widget::Space { size: 0.0 }, label("three")]);
    let at = |name: &str| fixture.by_label(name).unwrap_or_else(|| panic!("`{name}` is drawn")).rect;
    let gaps = craft_ui::testing::style::tokens(craft_theme::Mode::Light, craft_theme::Contrast::Standard).space.gap;
    let (one, two, three) = (at("one"), at("two"), at("three"));
    let flow = three.top() - two.bottom();
    assert!((two.top() - one.bottom() - flow - gaps.section).abs() < 0.5, "a large gap adds the section gap: {one:?} {two:?} {three:?}");
}

fn renamable() -> Widget {
    let items = ["file 0", "file 1"].map(craft_plugin::Item::new).to_vec();
    let edit = craft_plugin::ListEdit { rename: true, ..Default::default() };
    Widget::List { id: 4, columns: vec![], count: 2, first: 0, items, selected: vec![], multi: false, rows: 6, edit }
}

fn renamed(finish: egui::Key) -> Vec<UiEvent> {
    let (mut fixture, events) = shown(vec![renamable()]);
    fixture.key(egui::Key::Tab, egui::Modifiers::NONE).key(egui::Key::F2, egui::Modifiers::NONE).settle();
    assert!(!fixture.by_role(craft_ui::contract::Role::TEXT_INPUT).is_empty(), "F2 opens a name field over the focused row");
    fixture.type_text("New").settle();
    fixture.key(finish, egui::Modifiers::NONE).settle();
    events.take()
}

#[test]
fn f2_renames_the_focused_row_in_place_and_escape_cancels_the_rename() {
    let rename = craft_plugin::ListEvent::Rename { row: 0, text: "New".into() };
    assert_eq!(renamed(egui::Key::Enter), vec![UiEvent { id: 4, kind: craft_plugin::UiEventKind::List(rename) }]);
    assert_eq!(renamed(egui::Key::Escape), vec![]);
}

#[test]
fn a_panel_key_pressed_while_a_panel_control_holds_the_key_focus_reaches_the_host_and_a_text_field_keeps_its_keys() {
    let pressed = Rc::new(RefCell::new(Vec::new()));
    let host = Plugged { keys: vec!["Delete".into(), "Cmd+D".into()], pressed: Rc::clone(&pressed), ..Default::default() };
    let tree =
        vec![Widget::Button { id: 1, text: "Apply".into(), enabled: true }, Widget::TextEdit { id: 2, value: "abc".into(), hint: "Name".into() }];
    let (mut fixture, _) = shown_with(tree, host);
    fixture.key(egui::Key::Tab, egui::Modifiers::NONE).settle();
    fixture.key(egui::Key::Delete, egui::Modifiers::NONE).settle();
    assert_eq!(pressed.take(), vec!["Delete".to_string()], "Delete while the button holds the key focus is the panel's key");
    let name = fixture.by_label("Name").expect("the text field");
    fixture.click(&name).settle();
    fixture.key(egui::Key::Delete, egui::Modifiers::NONE).settle();
    assert_eq!(pressed.take(), Vec::<String>::new(), "a text field keeps Delete for its text");
}

#[test]
fn the_split_divider_moves_by_drag_and_by_arrow_keys_and_keeps_the_share() {
    let button = |id, text: &str| Widget::Button { id, text: text.into(), enabled: true };
    let split = Widget::Split { id: 40, axis: craft_plugin::Axis::Horizontal, first: vec![button(1, "Left")], second: vec![button(2, "Right")] };
    let (mut fixture, events) = shown(vec![split]);
    let right = |fixture: &Fixture| fixture.by_label("Right").expect("the second pane's button").rect.left();
    let start = right(&fixture);
    let divider = fixture.by_label("Divider").expect("the split's divider");
    fixture.drag(&divider, Vec2::new(60.0, 0.0)).settle();
    let dragged = right(&fixture);
    assert!((dragged - start - 60.0).abs() < 2.0, "the second pane follows the divider: {start} then {dragged}");
    fixture.idle(3);
    assert!((right(&fixture) - dragged).abs() < 0.5, "the share stays where the drag left it");
    let divider = fixture.by_label("Divider").expect("the split's divider");
    fixture.access(craft_ui::testing::focus::AccessActionKind::Focus, &divider).settle();
    fixture.key(egui::Key::ArrowLeft, egui::Modifiers::NONE).settle();
    assert!(right(&fixture) < dragged - 1.0, "Left moves the divider back: {dragged} then {}", right(&fixture));
    assert_eq!(*events.borrow(), vec![], "moving the divider sends the plugin nothing");
}

#[test]
fn an_image_takes_the_size_the_plugin_gave_and_a_click_on_it_sends_its_id() {
    let image = Widget::Image { id: 9, key: "thumb/0".into(), size: [40.0, 30.0] };
    let (mut fixture, events) = shown(vec![image]);
    let target = fixture.by_label("thumb/0").expect("the image is named by its key");
    assert_eq!(target.rect.size(), Vec2::new(40.0, 30.0));
    fixture.click(&target);
    fixture.settle();
    assert_eq!(*events.borrow(), vec![UiEvent { id: 9, kind: craft_plugin::UiEventKind::Click }]);
}

#[test]
fn the_default_triangulation_covers_a_concave_polygon_with_indices_into_its_own_points_in_either_winding() {
    use craft_plugin_ui_framework::Host as _;
    let ell = [[0.0, 0.0], [40.0, 0.0], [40.0, 10.0], [10.0, 10.0], [10.0, 30.0], [0.0, 30.0]];
    let triangulate = ().triangulate();
    for ring in [ell.to_vec(), ell.iter().rev().copied().collect()] {
        let triangles = triangulate(&ring);
        assert_eq!(triangles.len(), 4, "a hexagon is four triangles");
        let area = |[a, b, c]: [usize; 3]| {
            let ([ax, ay], [bx, by], [cx, cy]) = (ring[a], ring[b], ring[c]);
            ((bx - ax) * (cy - ay) - (by - ay) * (cx - ax)).abs() / 2.0
        };
        let covered: f64 = triangles.iter().copied().map(area).sum();
        assert_eq!(covered, 600.0, "the triangles over the given points cover the L's 600 square points");
    }
    assert!(triangulate(&[[0.0, 0.0], [1.0, 0.0]]).is_empty(), "two points are no polygon");
}
