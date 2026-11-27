use craft_behavior::Axis;
use craft_plugin::{Dropped, UiEventKind, Widget};
use craft_ui::contract::{ShowContainer as _, SizeLimits};
use craft_ui::docking::tree::FrameRules;
use craft_ui::docking::{SplitArea, SplitFramesCx as _};
use craft_ui::layout_components::Stack;
use craft_ui::runtime::drag::OsFiles;
use craft_ui::runtime::memory::with_session;
use craft_ui::sections::{DragSource, DropAction};

use crate::{Painter, values};

const EVEN: f32 = 0.5;
const PANES: (u8, u8) = (0, 1);
const ROWS: u32 = 12;

impl Painter<'_> {
    pub(crate) fn split(&mut self, ui: &mut egui::Ui, id: u64, axis: craft_plugin::Axis, first: &[Widget], second: &[Widget]) {
        let memory = egui::Id::unique(("craft-plugin-ui-framework.split", self.scope, id));
        let kept: Option<f32> = with_session(ui.ctx(), memory, |share: &mut Option<f32>| *share);
        let share = kept.filter(|share| share.is_finite()).map_or(EVEN, |share| share.clamp(0.0, 1.0));
        let axis = match axis {
            craft_plugin::Axis::Horizontal => Axis::X,
            craft_plugin::Axis::Vertical => Axis::Y,
        };
        let room = ui.available_height();
        let height = if room.is_finite() && room > 0.0 { values::extent(room) } else { values::rows_height(ui, ROWS) };
        let limits = |_: &u8| SizeLimits::default();
        let stripless = |_: &u8| true;
        let rules = FrameRules::new(&limits).stripless(&stripless);
        let out = Stack::new().room_height(height).fill().show(ui, |ui, _| {
            SplitArea::pair("Split", axis, PANES, share, rules).divider_label("Divider").show(ui, |ui, body| {
                for (pane, rect) in body.group_frames::<u8>() {
                    let children = if pane == PANES.0 { first } else { second };
                    Stack::new().show_at(ui, rect, |ui, _| self.all(ui, children));
                }
            })
        });
        if let Some(moved) = out.inner.into_iter().flat_map(|split| split.output.share()).filter_map(|event| event.value().copied()).last() {
            with_session(ui.ctx(), memory, |stored: &mut Option<f32>| *stored = Some(moved));
        }
    }

    pub(crate) fn drop_target(&mut self, ui: &mut egui::Ui, id: u64, children: &[Widget]) {
        let files = DropAction::new(|_: &OsFiles| true)
            .show(ui, |ui, _| DropAction::new(|_: &Dropped| true).show(ui, |ui, _| self.all(ui, children)).output.dropped);
        let paths: Vec<String> = files
            .output
            .dropped
            .iter()
            .flat_map(|dropped| &dropped.files)
            .filter_map(|file| file.path.as_ref().map(|path| path.to_string_lossy().into_owned()))
            .filter(|path| !path.is_empty())
            .collect();
        if !paths.is_empty() {
            self.host.accept_files(&paths);
            self.emit(id, UiEventKind::Drop(Dropped::Files { paths }));
        }
        if let Some(dropped) = files.inner.flatten() {
            self.emit(id, UiEventKind::Drop(dropped));
        }
    }

    pub(crate) fn drag_source(&mut self, ui: &mut egui::Ui, _id: u64, drag: &craft_plugin::Drag, children: &[Widget]) {
        match self.host.drag(drag) {
            Some(payload) => {
                DragSource::new(payload).show(ui, |ui, _| self.all(ui, children));
            }
            None => self.all(ui, children),
        }
    }
}
