//! Stories of `DragSource`.

use super::DragSource;
use crate::contract::{Props as _, Show as _, ShowContainer as _};
use crate::display::Text;
use crate::testing::{Drive, StoryData, StoryOutput, story};
use egui::Ui;

#[story(DragSource<u8>, covers = [Default], drive = Drive::Never)]
fn drag_source(ui: &mut Ui, _: &mut StoryData) -> StoryOutput {
    DragSource::new(1_u8)
        .label("Layer 1")
        .show(ui, |ui, _| {
            Text::new("Layer 1").show(ui);
        })
        .output
        .into()
}
