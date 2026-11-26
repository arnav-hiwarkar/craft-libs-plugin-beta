//! A key area takes key focus from a click on a button inside it and then takes its keys; a text field inside keeps
//! the focus a press gives it, and its keys.

use craft_keymap::{Chord, Key as Chorded};
use craft_ui::buttons::Button;
use craft_ui::contract::{Bind, Props as _, Show as _, ShowContainer as _};
use craft_ui::sections::KeyArea;
use craft_ui::testing::{Fixture, trial};
use craft_ui::text::TextField;
use egui::{Key, Modifiers};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[trial]
fn a_click_on_a_button_in_a_key_area_focuses_the_area_and_its_key_arrives_while_a_field_keeps_its_own() {
    let (pressed, clicks) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let text = Rc::new(RefCell::new(String::from("name")));
    let (seen, counted, typed) = (Rc::clone(&pressed), Rc::clone(&clicks), Rc::clone(&text));
    let mut fixture = Fixture::new(move |ui| {
        let area = KeyArea::new(vec![Chord::plain(Chorded::Delete)]).label("Layers").show(ui, |ui, _| {
            if Button::new("Count").show(ui).clicked() {
                counted.set(counted.get() + 1);
            }
            TextField::new("Name", Bind::bound(&mut *typed.borrow_mut())).show(ui);
        });
        seen.set(seen.get() + area.output.pressed.len());
    });
    fixture.idle(3);
    let count = fixture.by_label("Count").expect("the button shows");
    fixture.click(&count).idle(2);
    assert_eq!(clicks.get(), 1, "the button still acts on the click");
    let area = fixture.by_label("Layers").expect("the area is a group named by its label");
    assert_eq!(craft_ui::testing::access::focused_key(fixture.tree()), area.node, "the click gave the area key focus");
    fixture.key(Key::Delete, Modifiers::NONE).idle(2);
    assert_eq!(pressed.get(), 1, "Delete reached the focused area");
    let field = fixture.by_label("Name").expect("the field shows");
    fixture.click(&field).idle(2);
    assert_eq!(craft_ui::testing::access::focused_key(fixture.tree()), field.node, "a press on the field gives it focus");
    fixture.key(Key::Delete, Modifiers::NONE).idle(2);
    assert_eq!(pressed.get(), 1, "the focused field keeps Delete");
}
