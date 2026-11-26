//! Stories of `KeyArea`.

use super::KeyArea;
use crate::buttons::Button;
use crate::contract::{Props as _, Show as _, ShowContainer as _};
use crate::display::Text;
use crate::testing::{Drive, StoryData, StoryOutput, story};
use craft_keymap::{Chord, Key};
use egui::Ui;

#[story(KeyArea, covers = [Default], drive = Drive::Never)]
fn key_area(ui: &mut Ui, data: &mut StoryData) -> StoryOutput {
    let pressed = data.note("pressed", 0u32);
    let out = KeyArea::new(vec![Chord::plain(Key::Delete)]).label("Layers").show(ui, |ui, _| {
        Text::new("Click anywhere here, then press Delete").show(ui);
        Button::new("Count").show(ui);
    });
    *pressed += out.output.pressed.len() as u32;
    out.output.into()
}
