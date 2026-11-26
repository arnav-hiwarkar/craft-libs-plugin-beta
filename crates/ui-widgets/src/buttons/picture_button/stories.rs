//! Stories of `PictureButton`.

use super::PictureButton;
use crate::contract::Show as _;
use crate::runtime::env::Env;
use crate::testing::{StoryData, StoryOutput, story};
use egui::{Color32, ColorImage, Ui, Vec2};

/// A diagonal gradient, so the fitted picture shows.
fn gradient() -> ColorImage {
    const SIDE: usize = 32;
    let pixels = (0..SIDE * SIDE).map(|at| Color32::from_rgb((at % SIDE * 255 / SIDE) as u8, (at / SIDE * 255 / SIDE) as u8, 160)).collect();
    ColorImage::new([SIDE, SIDE], pixels)
}

#[story(PictureButton<'static>, covers = [Default])]
fn picture_button(ui: &mut Ui, _: &mut StoryData) -> StoryOutput {
    let side = Env::of(ui).tokens.size.thumb.lg;
    PictureButton::new("Gradient", 1, &gradient, Vec2::splat(side)).show(ui).into()
}
