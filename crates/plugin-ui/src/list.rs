use craft_plugin::{Column, ColumnWidth, Item, LIST_WINDOW, ListEdit, ListEvent, MAX_LIST_ITEMS, TextStyle, UiEventKind};
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
    pub edit: &'w ListEdit,
}

#[derive(Clone, Default)]
struct Cursor {
    asked: Option<(u32, u32)>,
    current: Option<u32>,
    anchor: Option<u32>,
    focus: Option<u32>,
    scroll: Option<f32>,
    area: Option<Rect>,
    renaming: Option<Renaming>,
    clicked_name: Option<(u32, f64)>,
    dragging: Option<Vec<u32>>,
    told: Option<(u32, bool)>,
    sent: Option<(u32, bool)>,
}

#[derive(Clone)]
struct Renaming {
    row: u32,
    text: String,
    fresh: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Move(u32),
    Toggle(bool),
    Activate,
    Flip,
    Rename,
    Reorder(u32),
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

    fn moving(&self, row: u32) -> Vec<u32> {
        let mut rows = if self.selected.contains(&row) { self.selected.to_vec() } else { vec![row] };
        rows.sort_unstable();
        rows.dedup();
        rows
    }

    fn action(&self, row: u32, key: Key, alt: bool, page: u32) -> Option<Action> {
        let last = self.count.saturating_sub(1);
        let item = self.item(row)?;
        if alt && self.edit.reorder {
            let moving = self.moving(row);
            let (low, high) = (*moving.first()?, *moving.last()?);
            return match key {
                Key::ArrowUp if low > 0 => Some(Action::Reorder(low - 1)),
                Key::ArrowDown if high + 1 < self.count => Some(Action::Reorder(high + 2)),
                _ => None,
            };
        }
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
            Key::F2 if self.edit.rename => Action::Rename,
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

fn moves(rows: &[u32], to: u32) -> bool {
    let contiguous = rows.windows(2).all(|pair| pair[1] == pair[0] + 1);
    match (rows.first(), rows.last()) {
        (Some(low), Some(high)) => !contiguous || to < *low || to > high + 1,
        _ => false,
    }
}

fn insertion(placed: &[(u32, Rect)], y: f32) -> Option<u32> {
    placed.iter().find(|(_, rect)| y < rect.center().y).map(|(row, _)| *row).or(placed.last().map(|(row, _)| row + 1))
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
        if cursor.told != rows.edit.sorted {
            (cursor.told, cursor.sent) = (rows.edit.sorted, None);
        }
        if !rows.columns.is_empty() {
            let sorted = cursor.sent.or(rows.edit.sorted);
            let heading = Heading { list: id, sortable: rows.edit.sortable, sorted, row_height };
            if let Some(column) = self.headings(ui, rows.columns, &widths, &heading) {
                let descending = sorted == Some((column, false));
                cursor.sent = Some((column, descending));
                self.emit(id, UiEventKind::List(ListEvent::Sort { column, descending }));
            }
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
        let now = ui.input(|i| i.time);
        let double_click = ui.ctx().options(|options| options.input_options.max_double_click_delay);
        if let Some((row, at)) = cursor.clicked_name {
            if now - at > double_click {
                cursor.clicked_name = None;
                cursor.renaming = rows.item(row).map(|item| Renaming { row, text: item.cells.first().cloned().unwrap_or_default(), fresh: true });
            }
            ui.ctx().request_repaint();
        }
        let mut menu = list.clone();
        let mut shown_rows = 0..0;
        let mut placed = Vec::new();
        let mut renamed = None;
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
                let renaming = cursor.renaming.as_ref().is_some_and(|renaming| renaming.row == row);
                let line = Line {
                    item,
                    row,
                    selected: rows.selected.contains(&row),
                    current: row == current,
                    tree,
                    table: !rows.columns.is_empty(),
                    drags: rows.edit.reorder,
                    renaming,
                };
                let (response, cell, name) = self.row(ui, id, &line, &widths, row_height);
                placed.push((row, response.rect));
                if let Some(renaming) = cursor.renaming.as_mut().filter(|renaming| renaming.row == row)
                    && let Some(done) = self.rename_field(ui, response.id.with("rename"), cell, renaming)
                {
                    renamed = Some((row, done));
                }
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
                    let on_name = response.interact_pointer_pos().is_some_and(|at| name.contains(at));
                    if rows.edit.rename && line.selected && rows.selected.len() == 1 && on_name && held.is_none() && response.clicked() {
                        cursor.clicked_name = Some((row, now));
                    }
                    if !(held.shift && rows.multi) {
                        cursor.anchor = Some(row);
                    }
                    cursor.current = Some(row);
                    response.request_focus();
                    self.emit(id, UiEventKind::List(ListEvent::Select(picked)));
                }
                if response.drag_started() {
                    cursor.dragging = Some(rows.moving(row));
                }
                if let Some(open) = line.item.open
                    && ui.ctx().read_response(response.id.with("disclosure")).is_some_and(|toggle| toggle.clicked())
                {
                    self.emit(id, UiEventKind::List(ListEvent::Toggle { row, open: !open }));
                }
                if response.double_clicked() {
                    (cursor.clicked_name, cursor.renaming) = (None, None);
                    self.emit(id, UiEventKind::List(ListEvent::Activate(row)));
                }
                menu |= response;
            }
        });
        cursor.area = Some(output.inner_rect);
        if let Some((row, done)) = renamed {
            if let Some(text) = done.filter(|text| rows.item(row).and_then(|item| item.cells.first()) != Some(text)) {
                self.emit(id, UiEventKind::List(ListEvent::Rename { row, text }));
            }
            (cursor.renaming, cursor.focus) = (None, Some(row));
        }
        if cursor.renaming.as_ref().is_some_and(|renaming| !shown_rows.contains(&renaming.row)) {
            cursor.renaming = None;
        }
        self.drag_rows(ui, id, &mut cursor, &placed, &output, stride);
        if let Some((row, (key, held))) = pressed {
            let page = ((output.inner_rect.height() / stride).floor() as u32).max(1);
            let shift = held.shift;
            match rows.action(row, key, held.alt, page) {
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
                Some(Action::Rename) => {
                    let text = rows.item(row).and_then(|item| item.cells.first().cloned()).unwrap_or_default();
                    cursor.renaming = Some(Renaming { row, text, fresh: true });
                }
                Some(Action::Reorder(to)) => {
                    let moving = rows.moving(row);
                    let next = if to > row { row + 1 } else { row - 1 };
                    (cursor.current, cursor.focus, cursor.anchor) = (Some(next), Some(next), Some(next));
                    self.emit(id, UiEventKind::List(ListEvent::Move { rows: moving, to }));
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

    fn drag_rows(
        &mut self,
        ui: &egui::Ui,
        id: u64,
        cursor: &mut Cursor,
        placed: &[(u32, Rect)],
        output: &egui::scroll_area::ScrollAreaOutput<()>,
        stride: f32,
    ) {
        let (Some(moving), Some(pointer)) = (cursor.dragging.clone(), ui.ctx().pointer_latest_pos()) else { return };
        let area = output.inner_rect;
        let to = insertion(placed, pointer.y);
        if ui.input(|i| !i.pointer.any_down() || i.pointer.any_released()) {
            cursor.dragging = None;
            if let Some(to) = to.filter(|to| moves(&moving, *to)) {
                self.emit(id, UiEventKind::List(ListEvent::Move { rows: moving, to }));
            }
            return;
        }
        if let Some(to) = to.filter(|to| moves(&moving, *to)) {
            let y = placed
                .iter()
                .find(|(row, _)| *row == to)
                .map_or_else(|| placed.last().map_or(area.top(), |(_, rect)| rect.bottom()), |(_, rect)| rect.top());
            ui.painter_at(area.expand(1.0)).hline(area.x_range(), y, Stroke::new(2.0, self.theme.accent));
        }
        let top = output.state.clamped_offset().y;
        let bottom = (output.content_size.y - area.height()).max(0.0);
        if pointer.y < area.top() + stride && top > 0.0 {
            cursor.scroll = Some((top - stride / 4.0).max(0.0));
        } else if pointer.y > area.bottom() - stride && top < bottom {
            cursor.scroll = Some((top + stride / 4.0).min(bottom));
        }
    }

    fn rename_field(&mut self, ui: &mut egui::Ui, field_id: egui::Id, cell: Rect, renaming: &mut Renaming) -> Option<Option<String>> {
        if renaming.fresh {
            let mut state = egui::text_edit::TextEditState::load(ui.ctx(), field_id).unwrap_or_default();
            state.cursor.set_char_range(Some(egui::text::CCursorRange::select_all_str(&renaming.text)));
            state.store(ui.ctx(), field_id);
            ui.memory_mut(|memory| memory.request_focus(field_id));
            ui.ctx().request_repaint();
        }
        let hint = self.host.tr("Rename");
        let mut field_ui = ui.new_child(UiBuilder::new().max_rect(cell));
        let field =
            field_ui.add(egui::TextEdit::singleline(&mut renaming.text).id(field_id).desired_width(cell.width()).font(FontId::proportional(12.0)));
        ui.ctx().accesskit_node_builder(field.id, |node| node.set_label(hint.as_str()));
        if std::mem::take(&mut renaming.fresh) {
            return None;
        }
        field.lost_focus().then(|| ui.input(|i| i.key_pressed(Key::Enter)).then(|| renaming.text.clone()))
    }

    fn headings(&mut self, ui: &mut egui::Ui, columns: &[Column], widths: &[f32], heading: &Heading) -> Option<u32> {
        let row_height = heading.row_height;
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), row_height), Sense::hover());
        let mut left = rect.left();
        let mut clicked = None;
        for (at, (column, width)) in columns.iter().zip(widths).enumerate() {
            let at = at as u32;
            let cell = Rect::from_min_size(pos2(left, rect.top()), vec2(*width, row_height));
            let sense = if heading.sortable { Sense::click() } else { Sense::hover() };
            let response = ui.interact(cell, egui::Id::unique(("plugin-list-heading", self.scope, heading.list, at)), sense);
            let order = heading.sorted.filter(|(column, _)| *column == at).map(|(_, descending)| descending);
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ColumnHeader, true, &column.title));
            ui.ctx().accesskit_node_builder(response.id, |node| match order {
                Some(true) => node.set_sort_direction(egui::accesskit::SortDirection::Descending),
                Some(false) => node.set_sort_direction(egui::accesskit::SortDirection::Ascending),
                None => {}
            });
            if heading.sortable && response.hovered() {
                ui.painter().rect_filled(cell, 2.0, self.theme.faint.gamma_multiply(0.5));
            }
            let arrow = if order.is_some() { row_height } else { 0.0 };
            let clip = Rect::from_x_y_ranges((cell.left() + 4.0)..=(cell.right() - 4.0 - arrow).max(cell.left() + 4.0), cell.y_range());
            let title = self.text(&column.title, TextStyle::Strong);
            let galley = egui::WidgetText::from(title).into_galley(ui, Some(egui::TextWrapMode::Truncate), clip.width(), egui::TextStyle::Body);
            ui.painter_at(clip).galley(pos2(clip.left(), cell.center().y - galley.size().y / 2.0), galley, self.theme.text);
            if let Some(descending) = order {
                let c = pos2(cell.right() - 4.0 - row_height / 2.0, cell.center().y);
                let r = row_height * 0.18;
                let points = if descending {
                    vec![pos2(c.x - r, c.y - r * 0.6), pos2(c.x + r, c.y - r * 0.6), pos2(c.x, c.y + r * 0.8)]
                } else {
                    vec![pos2(c.x - r, c.y + r * 0.6), pos2(c.x + r, c.y + r * 0.6), pos2(c.x, c.y - r * 0.8)]
                };
                ui.painter().add(egui::Shape::convex_polygon(points, self.theme.weak, Stroke::NONE));
            }
            if response.clicked() {
                clicked = Some(at);
            }
            left += width;
        }
        ui.painter().hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, self.theme.border));
        clicked
    }

    fn row(&mut self, ui: &mut egui::Ui, id: u64, line: &Line, widths: &[f32], row_height: f32) -> (egui::Response, Rect, Rect) {
        let Line { item, row, selected, current, tree, table, drags, renaming } = *line;
        let text = item.cells.join("  ");
        let mut sense = if current { Sense::click() } else { Sense::CLICK };
        if drags {
            sense |= Sense::DRAG;
        }
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
        let (mut first_cell, mut name) = (Rect::NOTHING, Rect::NOTHING);
        for (at, cell) in item.cells.iter().enumerate() {
            let width = widths.get(at).copied().unwrap_or_default();
            let start = if at == 0 { left } else { edge + 4.0 };
            let clip = Rect::from_x_y_ranges(start..=(edge + width - 4.0).max(start), rect.y_range());
            if at == 0 {
                first_cell = clip;
            }
            if !(at == 0 && renaming) {
                let drawn = ui.painter_at(clip).text(pos2(start, rect.center().y), Align2::LEFT_CENTER, cell, FontId::proportional(12.0), color);
                if at == 0 {
                    name = drawn.intersect(clip);
                }
            }
            edge += width;
        }
        (response, first_cell, name)
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
    drags: bool,
    renaming: bool,
}

struct Heading {
    list: u64,
    sortable: bool,
    sorted: Option<(u32, bool)>,
    row_height: f32,
}

fn pressed_keys(ui: &egui::Ui) -> Option<(Key, egui::Modifiers)> {
    ui.input(|i| {
        i.events.iter().find_map(|event| match event {
            egui::Event::Key { key, pressed: true, modifiers, .. } => Some((*key, *modifiers)),
            _ => None,
        })
    })
}

#[cfg(test)]
mod tests {
    use craft_plugin::{ColumnWidth, Item, ListEdit};
    use egui::{Key, Rect, pos2, vec2};

    use super::{Action, Rows, insertion, moves, widths, window};

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
        let edit = ListEdit { rename: true, reorder: true, ..Default::default() };
        let rows = Rows { columns: &[], count: 50, first: 10, items: &items, selected: &[12], multi: false, rows: 0, edit: &edit };
        assert_eq!(rows.action(10, Key::ArrowRight, false, 5), Some(Action::Move(11)));
        assert_eq!(rows.action(11, Key::ArrowRight, false, 5), Some(Action::Toggle(true)));
        assert_eq!(rows.action(10, Key::ArrowLeft, false, 5), Some(Action::Toggle(false)));
        assert_eq!(rows.action(12, Key::ArrowLeft, false, 5), Some(Action::Move(11)));
        assert_eq!(rows.action(13, Key::ArrowLeft, false, 5), Some(Action::Move(10)));
        assert_eq!(rows.action(14, Key::ArrowLeft, false, 5), None);
        assert_eq!(rows.action(10, Key::ArrowUp, false, 5), Some(Action::Move(9)));
        assert_eq!(rows.action(12, Key::PageDown, false, 5), Some(Action::Move(17)));
        assert_eq!(rows.action(12, Key::PageUp, false, 5), Some(Action::Move(7)));
        assert_eq!(rows.action(12, Key::End, false, 5), Some(Action::Move(49)));
        assert_eq!(rows.action(12, Key::Home, false, 5), Some(Action::Move(0)));
        assert_eq!(rows.action(12, Key::Enter, false, 5), Some(Action::Activate));
        assert_eq!(rows.action(12, Key::A, false, 5), None);
    }

    #[test]
    fn alt_arrows_move_the_selection_by_one_row_and_f2_renames_only_in_lists_that_allow_them() {
        let items: Vec<Item> = (0..5).map(|row| Item::new(&format!("file {row}"))).collect();
        let edit = ListEdit { rename: true, reorder: true, ..Default::default() };
        let rows = Rows { columns: &[], count: 5, first: 0, items: &items, selected: &[2, 1], multi: true, rows: 0, edit: &edit };
        assert_eq!(rows.action(1, Key::ArrowDown, true, 5), Some(Action::Reorder(4)));
        assert_eq!(rows.action(2, Key::ArrowUp, true, 5), Some(Action::Reorder(0)));
        assert_eq!(rows.action(4, Key::ArrowDown, true, 5), None);
        assert_eq!(rows.action(0, Key::ArrowUp, true, 5), None);
        assert_eq!(rows.action(1, Key::F2, false, 5), Some(Action::Rename));
        let fixed = Rows { edit: &ListEdit::default(), ..rows };
        assert_eq!((fixed.action(1, Key::F2, false, 5), fixed.action(1, Key::ArrowDown, true, 5)), (None, Some(Action::Move(2))));
    }

    #[test]
    fn a_drop_goes_before_the_row_whose_upper_half_holds_the_pointer_and_a_drop_beside_the_rows_moves_nothing() {
        let placed: Vec<(u32, Rect)> = (3..6).map(|row| (row, Rect::from_min_size(pos2(0.0, row as f32 * 20.0), vec2(100.0, 20.0)))).collect();
        assert_eq!(
            [insertion(&placed, 61.0), insertion(&placed, 75.0), insertion(&placed, 500.0), insertion(&[], 5.0)],
            [Some(3), Some(4), Some(6), None]
        );
        assert_eq!(
            [moves(&[2], 0), moves(&[2], 2), moves(&[2], 3), moves(&[2], 4), moves(&[1, 2], 3), moves(&[1, 3], 2)],
            [true, false, false, true, false, true]
        );
    }
}
