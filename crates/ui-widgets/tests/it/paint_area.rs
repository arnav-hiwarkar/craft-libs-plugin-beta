//! A paint area reports a secondary press as one context sample, the second press of a double click with a click count
//! of two, and a hover only when the pointer reached a new place.

use craft_ui::contract::Show as _;
use craft_ui::controls::paint_area::{PaintArea, PaintPhase};
use craft_ui::testing::layout::in_region;
use craft_ui::testing::{Frames, pointer, trial};
use egui::{Rect, pos2, vec2};
use std::cell::{Cell, RefCell};

#[trial]
fn a_paint_area_reports_context_presses_click_counts_and_hovers_only_on_moves() {
    let mut frames = Frames::new();
    let shown = Cell::new(Rect::NOTHING);
    let samples = RefCell::new(Vec::new());
    let mut show = |ui: &mut egui::Ui| {
        let region = Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 100.0));
        let out = in_region(ui, region, |ui| PaintArea::new("Drawing", |_, _| {}).show(ui));
        shown.set(out.rect);
        samples.borrow_mut().extend(out.pointers.iter().map(|sample| (sample.phase, sample.clicks)));
    };
    frames.frame(Vec::new(), &mut show);
    let at = shown.get().center();
    frames.frame(vec![pointer::moved(at)], &mut show);
    frames.frame(Vec::new(), &mut show);
    frames.frame(Vec::new(), &mut show);
    assert_eq!(samples.take(), [(PaintPhase::Hover, 0)], "one hover for the move, none while the pointer rests");
    frames.frame(vec![pointer::secondary(at, true), pointer::secondary(at, false)], &mut show);
    frames.frame(Vec::new(), &mut show);
    frames.frame(Vec::new(), &mut show);
    assert_eq!(samples.take(), [(PaintPhase::Context, 1)], "a right click is one context sample");
    pointer::click(&mut frames, at, &mut show);
    pointer::click(&mut frames, at, &mut show);
    let phases = samples.take();
    assert_eq!(
        phases,
        [(PaintPhase::Down, 1), (PaintPhase::Up, 1), (PaintPhase::Down, 2), (PaintPhase::Up, 2)],
        "the second press of a double click counts two clicks"
    );
}
