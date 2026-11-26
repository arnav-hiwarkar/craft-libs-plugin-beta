use craft_plugin::{Column, Item, LIST_WINDOW, ListEvent, MAX_LIST_ITEMS, TextStyle, UiEventKind};
use egui::{Sense, vec2};

use crate::Host;
use crate::panel::Renderer;

pub(crate) struct Rows<'w> {
    pub columns: &'w [Column],
    pub count: u32,
    pub first: u32,
    pub items: &'w [Item],
    pub selected: &'w [u32],
    pub multi: bool,
    pub rows: u32,
}

impl Rows<'_> {
    fn item(&self, row: u32) -> Option<&Item> {
        row.checked_sub(self.first).and_then(|at| self.items.get(at as usize))
    }

    fn holds(&self, shown: &std::ops::Range<u32>) -> bool {
        let built = self.first..self.first.saturating_add(self.items.len() as u32);
        shown.is_empty() || (built.start <= shown.start && shown.end <= built.end)
    }
}

pub(crate) fn window(shown: &std::ops::Range<u32>, count: u32) -> (u32, u32) {
    let len = shown.len() as u32;
    let wanted = len.saturating_mul(3).clamp(LIST_WINDOW.min(count), MAX_LIST_ITEMS);
    let first = shown.start.saturating_sub(wanted.saturating_sub(len) / 2).min(count.saturating_sub(wanted));
    (first, wanted.min(count - first))
}

impl<H: Host> Renderer<'_, H> {
    pub(crate) fn list(&mut self, ui: &mut egui::Ui, id: u64, rows: Rows) {
        let row_height = ui.spacing().interact_size.y;
        if !rows.columns.is_empty() {
            ui.horizontal(|ui| {
                for column in rows.columns {
                    ui.label(self.text(&column.title, TextStyle::Strong));
                }
            });
        }
        let height = match rows.rows {
            0 => ui.available_height().max(row_height),
            shown => row_height * shown as f32,
        };
        let asked = egui::Id::unique(("plugin-list-window", self.scope, id));
        let mut shown_rows = 0..0;
        let area = egui::ScrollArea::vertical().id_salt(("plugin-list", id)).max_height(height).auto_shrink([false, true]);
        let response = area.show_rows(ui, row_height, rows.count as usize, |ui, range| {
            shown_rows = range.start as u32..range.end as u32;
            for row in shown_rows.clone() {
                self.row(ui, id, &rows, row, row_height);
            }
        });
        let name = self.host.tr("List");
        let list = ui.interact(response.inner_rect, egui::Id::unique(("plugin-list-area", self.scope, id)), Sense::hover());
        list.widget_info(|| egui::WidgetInfo::labeled(egui::Role::List, true, &name));
        self.register(id, &list, "List");
        let waiting = ui.data(|data| data.get_temp::<(u32, u32)>(asked)).is_some_and(|(first, count)| {
            first <= shown_rows.start && shown_rows.end <= first.saturating_add(count) && !rows.holds(&(first..first.saturating_add(count)))
        });
        if !rows.holds(&shown_rows) && !waiting {
            let (first, count) = window(&shown_rows, rows.count);
            ui.data_mut(|data| data.insert_temp(asked, (first, count)));
            self.emit(id, UiEventKind::List(ListEvent::Show { first, count }));
        }
    }

    fn row(&mut self, ui: &mut egui::Ui, id: u64, rows: &Rows, row: u32, row_height: f32) {
        let Some(item) = rows.item(row) else {
            ui.allocate_exact_size(vec2(ui.available_width(), row_height), Sense::hover());
            return;
        };
        let selected = rows.selected.contains(&row);
        let text = item.cells.join("  ");
        let response = ui
            .horizontal(|ui| {
                ui.add_space(f32::from(item.depth) * ui.spacing().indent);
                if let Some(icon) = &item.icon {
                    let (rect, _) = ui.allocate_exact_size(vec2(row_height, row_height), Sense::hover());
                    self.paint_icon(ui.painter(), rect.shrink(2.0), icon, &text, self.theme.text);
                }
                let style = if item.weak { TextStyle::Weak } else { TextStyle::Body };
                ui.add_sized(vec2(ui.available_width(), row_height), egui::Button::selectable(selected, self.text(&text, style)))
            })
            .inner;
        self.host.register(&format!("{}.{id}.{row}", self.scope), &response, &text);
        if response.clicked_by(egui::PointerButton::Primary) {
            let held = ui.input(|i| i.modifiers);
            let picked = match (rows.multi, held.command, held.shift, rows.selected.last()) {
                (true, true, _, _) if selected => rows.selected.iter().copied().filter(|at| *at != row).collect(),
                (true, true, _, _) => rows.selected.iter().copied().chain([row]).collect(),
                (true, _, true, Some(&anchor)) => (anchor.min(row)..=anchor.max(row)).collect(),
                _ => vec![row],
            };
            response.request_focus();
            self.emit(id, UiEventKind::List(ListEvent::Select(picked)));
        }
        if response.double_clicked() || (response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
            self.emit(id, UiEventKind::List(ListEvent::Activate(row)));
        }
        self.last = Some(response);
    }
}

#[cfg(test)]
mod tests {
    use super::window;

    #[test]
    fn a_window_covers_the_rows_in_view_with_one_screen_either_side_and_stays_inside_the_list() {
        assert_eq!(window(&(500..520), 100_000), (446, 128));
        assert_eq!(window(&(5_000..5_200), 100_000), (4_800, 600));
        assert_eq!(window(&(99_990..100_000), 100_000), (99_872, 128));
        assert_eq!(window(&(0..10), 30), (0, 30));
    }
}
