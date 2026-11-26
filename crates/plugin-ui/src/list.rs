use craft_plugin::{Column, ColumnWidth, Item, LIST_WINDOW, ListEvent, MAX_LIST_ITEMS, TextStyle, UiEventKind};
use egui::{Align2, FontId, Key, Rect, Sense, Stroke, StrokeKind, UiBuilder, pos2, vec2};

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

#[derive(Clone, Default)]
struct Cursor {
    asked: Option<(u32, u32)>,
    current: Option<u32>,
    anchor: Option<u32>,
    focus: Option<u32>,
    scroll: Option<f32>,
    area: Option<Rect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Move(u32),
    Toggle(bool),
    Activate,
    Flip,
}

impl Rows<'_> {
    fn item(&self, row: u32) -> Option<&Item> {
        row.checked_sub(self.first).and_then(|at| self.items.get(at as usize))
    }

    fn holds(&self, shown: &std::ops::Range<u32>) -> bool {
        let built = self.first..self.first.saturating_add(self.items.len() as u32);
        shown.is_empty() || (built.start <= shown.start && shown.end <= built.end)
    }

    fn parent(&self, row: u32) -> Option<u32> {
        let depth = self.item(row)?.depth;
        (self.first..row).rev().find(|at| self.item(*at).is_some_and(|item| item.depth < depth))
    }

    fn action(&self, row: u32, key: Key, page: u32) -> Option<Action> {
        let last = self.count.saturating_sub(1);
        let item = self.item(row)?;
        let action = match key {
            Key::ArrowUp => Action::Move(row.saturating_sub(1)),
            Key::ArrowDown => Action::Move((row + 1).min(last)),
            Key::Home => Action::Move(0),
            Key::End => Action::Move(last),
            Key::PageUp => Action::Move(row.saturating_sub(page)),
            Key::PageDown => Action::Move(row.saturating_add(page).min(last)),
            Key::ArrowRight => match item.open {
                Some(false) => Action::Toggle(true),
                Some(true) if self.item(row + 1).is_some_and(|child| child.depth > item.depth) => Action::Move(row + 1),
                _ => return None,
            },
            Key::ArrowLeft => match item.open {
                Some(true) => Action::Toggle(false),
                _ => Action::Move(self.parent(row)?),
            },
            Key::Enter => Action::Activate,
            Key::Space => Action::Flip,
            _ => return None,
        };
        (action != Action::Move(row)).then_some(action)
    }

    fn picked(&self, cursor: &Cursor, row: u32, extend: bool, flip: bool) -> Vec<u32> {
        let anchor = cursor.anchor.or(self.selected.last().copied());
        match (self.multi, extend, flip, anchor) {
            (true, _, true, _) if self.selected.contains(&row) => self.selected.iter().copied().filter(|at| *at != row).collect(),
            (true, _, true, _) => self.selected.iter().copied().chain([row]).collect(),
            (true, true, _, Some(anchor)) => (anchor.min(row)..=anchor.max(row)).collect(),
            _ => vec![row],
        }
    }
}

pub(crate) fn window(shown: &std::ops::Range<u32>, count: u32) -> (u32, u32) {
    let len = shown.len() as u32;
    let wanted = len.saturating_mul(3).clamp(LIST_WINDOW.min(count), MAX_LIST_ITEMS);
    let first = shown.start.saturating_sub(wanted.saturating_sub(len) / 2).min(count.saturating_sub(wanted));
    (first, wanted.min(count - first))
}

pub(crate) fn widths(columns: &[ColumnWidth], total: f32, font: f32) -> Vec<f32> {
    let fixed = |width: &ColumnWidth| match width {
        ColumnWidth::Narrow => Some(5.0 * font),
        ColumnWidth::Wide => Some(15.0 * font),
        ColumnWidth::Auto | ColumnWidth::Fill => None,
    };
    let rest = (total - columns.iter().filter_map(fixed).sum::<f32>()).max(0.0);
    let fills = columns.iter().filter(|width| **width == ColumnWidth::Fill).count() as f32;
    let autos = columns.iter().filter(|width| **width == ColumnWidth::Auto).count() as f32;
    let auto = match fills {
        0.0 => rest / autos.max(1.0),
        _ => (rest / (autos + fills)).min(8.0 * font),
    };
    let fill = (rest - auto * autos).max(0.0) / fills.max(1.0);
    columns
        .iter()
        .map(|width| match width {
            ColumnWidth::Auto => auto,
            ColumnWidth::Fill => fill,
            fixed_width => fixed(fixed_width).unwrap_or_default(),
        })
        .collect()
}

impl<H: Host> Renderer<'_, H> {
    pub(crate) fn list(&mut self, ui: &mut egui::Ui, id: u64, rows: Rows) {
        let row_height = ui.spacing().interact_size.y;
        let stride = row_height + ui.spacing().item_spacing.y;
        let font = ui.text_style_height(&egui::TextStyle::Body);
        let state = egui::Id::unique(("plugin-list-state", self.scope, id));
        let mut cursor: Cursor = ui.data(|data| data.get_temp(state)).unwrap_or_default();
        let cells = rows.columns.len().max(rows.items.iter().map(|item| item.cells.len()).max().unwrap_or(1));
        let kinds: Vec<ColumnWidth> = (0..cells).map(|at| rows.columns.get(at).map_or(ColumnWidth::Auto, |column| column.width)).collect();
        let widths = widths(&kinds, ui.available_width(), font);
        if !rows.columns.is_empty() {
            self.headings(ui, rows.columns, &widths, row_height);
        }
        let height = match rows.rows {
            0 => ui.available_height().max(row_height),
            shown => row_height * shown as f32,
        };
        let guess = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), height));
        let list = ui.interact(cursor.area.unwrap_or(guess), egui::Id::unique(("plugin-list-area", self.scope, id)), Sense::CLICK);
        let (role, name) = if rows.columns.is_empty() { (egui::Role::List, "List") } else { (egui::Role::Grid, "Table") };
        let label = self.host.tr(name);
        list.widget_info(|| egui::WidgetInfo::labeled(role, true, &label));
        self.host.register(&format!("{}.{id}", self.scope), &list, name);
        let tree = rows.items.iter().any(|item| item.open.is_some());
        let current = cursor.current.or(rows.selected.first().copied()).unwrap_or(rows.first);
        let mut menu = list.clone();
        let mut shown_rows = 0..0;
        let mut pressed = None;
        let mut area = egui::ScrollArea::vertical().id_salt(("plugin-list", id)).max_height(height).auto_shrink([false, true]);
        if let Some(offset) = cursor.scroll.take() {
            area = area.vertical_scroll_offset(offset);
        }
        let output = area.show_rows(ui, row_height, rows.count as usize, |ui, range| {
            shown_rows = range.start as u32..range.end as u32;
            for row in shown_rows.clone() {
                let Some(item) = rows.item(row) else {
                    ui.allocate_exact_size(vec2(ui.available_width(), row_height), Sense::hover());
                    continue;
                };
                let line = Line { item, row, selected: rows.selected.contains(&row), current: row == current, tree, table: !rows.columns.is_empty() };
                let response = self.row(ui, id, &line, &widths, row_height);
                if cursor.focus == Some(row) {
                    response.request_focus();
                    cursor.focus = None;
                }
                if response.has_focus() {
                    let arrows = egui::EventFilter { horizontal_arrows: true, vertical_arrows: true, tab: false, escape: false };
                    ui.memory_mut(|memory| memory.set_focus_lock_filter(response.id, arrows));
                    if ui.memory(|memory| memory.had_focus_last_frame(response.id)) {
                        pressed = pressed.or(pressed_keys(ui).map(|key| (row, key)));
                    }
                }
                if response.clicked_by(egui::PointerButton::Primary) || (response.secondary_clicked() && !line.selected) {
                    let held = ui.input(|i| i.modifiers);
                    let picked = rows.picked(&cursor, row, held.shift, held.command);
                    if !(held.shift && rows.multi) {
                        cursor.anchor = Some(row);
                    }
                    cursor.current = Some(row);
                    response.request_focus();
                    self.emit(id, UiEventKind::List(ListEvent::Select(picked)));
                }
                if let Some(open) = line.item.open
                    && ui.ctx().read_response(response.id.with("disclosure")).is_some_and(|toggle| toggle.clicked())
                {
                    self.emit(id, UiEventKind::List(ListEvent::Toggle { row, open: !open }));
                }
                if response.double_clicked() {
                    self.emit(id, UiEventKind::List(ListEvent::Activate(row)));
                }
                menu |= response;
            }
        });
        cursor.area = Some(output.inner_rect);
        if let Some((row, (key, shift))) = pressed {
            let page = ((output.inner_rect.height() / stride).floor() as u32).max(1);
            match rows.action(row, key, page) {
                Some(Action::Move(to)) => {
                    let picked = rows.picked(&cursor, to, shift, false);
                    if !(shift && rows.multi) {
                        cursor.anchor = Some(to);
                    }
                    let top = output.state.clamped_offset().y;
                    let (start, end) = (to as f32 * stride, to as f32 * stride + row_height);
                    if start < top {
                        cursor.scroll = Some(start);
                    } else if end > top + output.inner_rect.height() {
                        cursor.scroll = Some(end - output.inner_rect.height());
                    }
                    (cursor.current, cursor.focus) = (Some(to), Some(to));
                    self.emit(id, UiEventKind::List(ListEvent::Select(picked)));
                }
                Some(Action::Toggle(open)) => self.emit(id, UiEventKind::List(ListEvent::Toggle { row, open })),
                Some(Action::Activate) => self.emit(id, UiEventKind::List(ListEvent::Activate(row))),
                Some(Action::Flip) => {
                    cursor.anchor = Some(row);
                    self.emit(id, UiEventKind::List(ListEvent::Select(rows.picked(&cursor, row, false, true))));
                }
                None => {}
            }
            ui.ctx().request_repaint();
        }
        let waiting = cursor.asked.is_some_and(|(first, count)| {
            first <= shown_rows.start && shown_rows.end <= first.saturating_add(count) && !rows.holds(&(first..first.saturating_add(count)))
        });
        if !rows.holds(&shown_rows) && !waiting {
            let (first, count) = window(&shown_rows, rows.count);
            cursor.asked = Some((first, count));
            self.emit(id, UiEventKind::List(ListEvent::Show { first, count }));
        }
        if cursor.focus.is_some_and(|row| rows.item(row).is_some()) || cursor.scroll.is_some() {
            ui.ctx().request_repaint();
        }
        ui.data_mut(|data| data.insert_temp(state, cursor));
        self.last = Some(menu);
    }

    fn headings(&mut self, ui: &mut egui::Ui, columns: &[Column], widths: &[f32], row_height: f32) {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), row_height), Sense::hover());
        let mut left = rect.left();
        for (column, width) in columns.iter().zip(widths) {
            let cell = Rect::from_min_size(pos2(left, rect.top()), vec2(*width, row_height));
            let layout = egui::Layout::left_to_right(egui::Align::Center);
            let heading = ui.scope_builder(UiBuilder::new().max_rect(cell.shrink2(vec2(4.0, 0.0))).layout(layout), |ui| {
                ui.add(egui::Label::new(self.text(&column.title, TextStyle::Strong)).truncate())
            });
            heading.inner.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ColumnHeader, true, &column.title));
            left += width;
        }
        ui.painter().hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, self.theme.border));
    }

    fn row(&mut self, ui: &mut egui::Ui, id: u64, line: &Line, widths: &[f32], row_height: f32) -> egui::Response {
        let Line { item, row, selected, current, tree, table } = *line;
        let text = item.cells.join("  ");
        let sense = if current { Sense::click() } else { Sense::CLICK };
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), row_height), Sense::hover());
        let response = ui.interact(rect, egui::Id::unique(("plugin-list-row", self.scope, id, row)), sense);
        let role = if table { egui::Role::Row } else { egui::Role::ListItem };
        response.widget_info(|| egui::WidgetInfo::labeled(role, true, &text));
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_selected(selected);
            if let Some(open) = item.open {
                node.set_expanded(open);
            }
        });
        self.host.register(&format!("{}.{id}.{row}", self.scope), &response, &text);
        let painter = ui.painter_at(rect);
        let theme = self.theme;
        if selected {
            painter.rect_filled(rect, 2.0, theme.selected);
        } else if response.hovered() {
            painter.rect_filled(rect, 2.0, theme.faint.gamma_multiply(0.5));
        }
        if response.has_focus() {
            painter.rect_stroke(rect, 2.0, Stroke::new(1.0, theme.accent), StrokeKind::Inside);
        }
        let mut left = rect.left() + 4.0 + f32::from(item.depth) * ui.spacing().indent;
        if tree {
            let triangle = Rect::from_min_size(pos2(left, rect.top()), vec2(row_height, row_height));
            if let Some(open) = item.open {
                self.disclosure(ui, response.id.with("disclosure"), triangle, open, item.cells.first().map_or("", String::as_str));
            }
            left = triangle.right();
        }
        if let Some(icon) = &item.icon {
            let square = Rect::from_min_size(pos2(left, rect.top()), vec2(row_height, row_height));
            self.paint_icon(&painter, square.shrink(2.0), icon, &text, theme.text);
            left = square.right() + 2.0;
        }
        let color = if item.weak { theme.weak } else { theme.text };
        let mut edge = rect.left();
        for (at, cell) in item.cells.iter().enumerate() {
            let width = widths.get(at).copied().unwrap_or_default();
            let start = if at == 0 { left } else { edge + 4.0 };
            let clip = Rect::from_x_y_ranges(start..=(edge + width - 4.0).max(start), rect.y_range());
            ui.painter_at(clip).text(pos2(start, rect.center().y), Align2::LEFT_CENTER, cell, FontId::proportional(12.0), color);
            edge += width;
        }
        response
    }

    fn disclosure(&mut self, ui: &mut egui::Ui, toggle_id: egui::Id, rect: Rect, open: bool, text: &str) {
        let toggle = ui.interact(rect, toggle_id, Sense::CLICK);
        let name = format!("{} {text}", self.host.tr(if open { "Close" } else { "Open" }));
        toggle.widget_info(|| egui::WidgetInfo::labeled(egui::Role::DisclosureTriangle, true, &name));
        ui.ctx().accesskit_node_builder(toggle.id, |node| node.set_expanded(open));
        let c = rect.center();
        let r = rect.height() * 0.2;
        let points = if open {
            vec![pos2(c.x - r, c.y - r * 0.6), pos2(c.x + r, c.y - r * 0.6), pos2(c.x, c.y + r * 0.8)]
        } else {
            vec![pos2(c.x - r * 0.6, c.y - r), pos2(c.x + r * 0.8, c.y), pos2(c.x - r * 0.6, c.y + r)]
        };
        let color = if toggle.hovered() { self.theme.accent } else { self.theme.weak };
        ui.painter().add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
    }
}

#[derive(Clone, Copy)]
struct Line<'w> {
    item: &'w Item,
    row: u32,
    selected: bool,
    current: bool,
    tree: bool,
    table: bool,
}

fn pressed_keys(ui: &egui::Ui) -> Option<(Key, bool)> {
    ui.input(|i| {
        i.events.iter().find_map(|event| match event {
            egui::Event::Key { key, pressed: true, modifiers, .. } => Some((*key, modifiers.shift)),
            _ => None,
        })
    })
}

#[cfg(test)]
mod tests {
    use craft_plugin::{ColumnWidth, Item};
    use egui::Key;

    use super::{Action, Rows, widths, window};

    #[test]
    fn a_window_covers_the_rows_in_view_with_one_screen_either_side_and_stays_inside_the_list() {
        assert_eq!(window(&(500..520), 100_000), (446, 128));
        assert_eq!(window(&(5_000..5_200), 100_000), (4_800, 600));
        assert_eq!(window(&(99_990..100_000), 100_000), (99_872, 128));
        assert_eq!(window(&(0..10), 30), (0, 30));
    }

    #[test]
    fn narrow_and_wide_columns_take_font_sized_widths_fill_takes_the_rest_and_auto_shares_it() {
        use ColumnWidth::{Auto, Fill, Narrow, Wide};
        assert_eq!(widths(&[Narrow, Auto, Auto], 300.0, 10.0), vec![50.0, 125.0, 125.0]);
        assert_eq!(widths(&[Wide, Auto, Fill], 400.0, 10.0), vec![150.0, 80.0, 170.0]);
        assert_eq!(widths(&[Fill, Narrow], 40.0, 10.0), vec![0.0, 50.0]);
        assert_eq!(widths(&[Auto, Auto], 300.0, 10.0), vec![150.0, 150.0]);
    }

    #[test]
    fn keys_move_through_a_tree_open_and_close_rows_and_left_goes_to_the_parent() {
        let items =
            [Item::new("a").open(true), Item::new("b").depth(1).open(false), Item::new("c").depth(2), Item::new("d").depth(1), Item::new("e")];
        let rows = Rows { columns: &[], count: 50, first: 10, items: &items, selected: &[], multi: false, rows: 0 };
        assert_eq!(rows.action(10, Key::ArrowRight, 5), Some(Action::Move(11)));
        assert_eq!(rows.action(11, Key::ArrowRight, 5), Some(Action::Toggle(true)));
        assert_eq!(rows.action(10, Key::ArrowLeft, 5), Some(Action::Toggle(false)));
        assert_eq!(rows.action(12, Key::ArrowLeft, 5), Some(Action::Move(11)));
        assert_eq!(rows.action(13, Key::ArrowLeft, 5), Some(Action::Move(10)));
        assert_eq!(rows.action(14, Key::ArrowLeft, 5), None);
        assert_eq!(rows.action(10, Key::ArrowUp, 5), Some(Action::Move(9)));
        assert_eq!(rows.action(12, Key::PageDown, 5), Some(Action::Move(17)));
        assert_eq!(rows.action(12, Key::PageUp, 5), Some(Action::Move(7)));
        assert_eq!(rows.action(12, Key::End, 5), Some(Action::Move(49)));
        assert_eq!(rows.action(12, Key::Home, 5), Some(Action::Move(0)));
        assert_eq!(rows.action(12, Key::Enter, 5), Some(Action::Activate));
        assert_eq!(rows.action(12, Key::A, 5), None);
    }
}
