//! Stories of `PaintArea`.

use super::PaintArea;
use crate::contract::Show as _;
use crate::testing::{StoryData, StoryOutput, story};
use egui::Ui;

#[story(PaintArea<'static>, covers = [Default])]
fn paint_area(ui: &mut Ui, _: &mut StoryData) -> StoryOutput {
    PaintArea::new("Drawing", |painter, rect| painter.checker(rect)).show(ui).into()
}
