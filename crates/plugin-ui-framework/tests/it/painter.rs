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
