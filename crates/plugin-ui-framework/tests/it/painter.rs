use std::cell::RefCell;
use std::rc::Rc;

use craft_plugin::{TextStyle, UiEvent, Widget};
use craft_ui::testing::Fixture;
use egui::Vec2;

pub fn shown(tree: Vec<Widget>) -> (Fixture, Rc<RefCell<Vec<UiEvent>>>) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&events);
    let mut fixture = Fixture::new(move |ui| seen.borrow_mut().extend(craft_plugin_ui_framework::draw(ui, &tree, "painter", &mut ())));
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
