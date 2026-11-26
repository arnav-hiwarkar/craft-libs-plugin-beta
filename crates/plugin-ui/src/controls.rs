use craft_plugin::{Axis, Gap, Gradient, MenuItem, Span, TextStyle, UiEventKind};
use egui::containers::menu::SubMenuButton;
use egui::{Popup, Rect, Sense, SetOpenCommand, Stroke, StrokeKind, UiBuilder, vec2};

use crate::Host;
use crate::panel::Renderer;

const ARROWS: egui::EventFilter = egui::EventFilter { horizontal_arrows: true, vertical_arrows: true, tab: false, escape: false };
const SPLIT_STEP: f32 = 0.05;
const SPLIT_LIMITS: (f32, f32) = (0.1, 0.9);

pub(crate) fn gap(spacing: &egui::style::Spacing, gap: Gap) -> f32 {
    match gap {
        Gap::Small => spacing.item_spacing.y,
        Gap::Medium => spacing.interact_size.y * 0.5,
        Gap::Large => spacing.interact_size.y,
    }
}

fn arrow_step(ui: &egui::Ui, response: &egui::Response, back: egui::Key, forward: egui::Key) -> i32 {
    if !response.has_focus() {
        return 0;
    }
    ui.memory_mut(|memory| memory.set_focus_lock_filter(response.id, ARROWS));
    if !ui.memory(|memory| memory.had_focus_last_frame(response.id)) {
        return 0;
    }
    ui.input(|i| i32::from(i.key_pressed(forward)) - i32::from(i.key_pressed(back)))
}

fn pending<T: Clone + Send + Sync + 'static>(ui: &egui::Ui, id: egui::Id, shown: &T) -> T {
    let held = ui.input(|i| i.pointer.any_down());
    match ui.data(|data| data.get_temp::<T>(id)) {
        Some(sent) if held => sent,
        Some(_) => {
            ui.data_mut(|data| data.remove::<T>(id));
            shown.clone()
        }
        None => shown.clone(),
    }
}

impl<H: Host> Renderer<'_, H> {
    pub(crate) fn icon_button(&mut self, ui: &mut egui::Ui, id: u64, icon: &str, text: &str, enabled: bool) {
        let side = ui.spacing().interact_size.y;
        let response = ui
            .add_enabled_ui(enabled, |ui| {
                let (rect, response) = ui.allocate_exact_size(vec2(side, side), Sense::click());
                response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), text));
                let visuals = ui.style().interact(&response);
                ui.painter().rect(rect, visuals.corner_radius, visuals.weak_bg_fill, visuals.bg_stroke, StrokeKind::Inside);
                let color = if ui.is_enabled() { self.theme.text } else { self.theme.weak };
                self.paint_icon(ui.painter(), rect.shrink(3.0), icon, text, color);
                response
            })
            .inner
            .on_hover_text(text);
        self.register(id, &response, text);
        if response.clicked() {
            self.emit(id, UiEventKind::Click);
        }
    }

    pub(crate) fn toggle(&mut self, ui: &mut egui::Ui, id: u64, text: &str, value: bool) {
        let height = ui.spacing().interact_size.y * 0.8;
        let switch = vec2(height * 1.8, height);
        let galley =
            egui::WidgetText::from(self.text(text, TextStyle::Body)).into_galley(ui, None, ui.available_width() - switch.x, egui::TextStyle::Body);
        let gap = ui.spacing().item_spacing.x;
        let size = vec2(switch.x + gap + galley.size().x, switch.y.max(galley.size().y));
        let (whole, response) = ui.allocate_exact_size(size, Sense::click());
        response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Switch, true, value, text));
        let rect = Rect::from_min_size(egui::pos2(whole.left(), whole.center().y - switch.y / 2.0), switch);
        let on = ui.ctx().animate_bool_responsive(response.id, value);
        let track = if value { self.theme.accent } else { self.theme.surface };
        let radius = rect.height() / 2.0;
        let painter = ui.painter();
        painter.rect(rect, radius, track, Stroke::new(1.0, self.theme.border), StrokeKind::Inside);
        let knob = egui::pos2(egui::lerp((rect.left() + radius)..=(rect.right() - radius), on), rect.center().y);
        painter.circle_filled(knob, radius * 0.75, self.theme.text);
        if response.has_focus() {
            painter.rect_stroke(rect.expand(2.0), radius + 2.0, Stroke::new(1.0, self.theme.accent), StrokeKind::Outside);
        }
        painter.galley(egui::pos2(rect.right() + gap, whole.center().y - galley.size().y / 2.0), galley, self.theme.text);
        self.register(id, &response, text);
        if response.clicked() {
            self.emit(id, UiEventKind::Bool(!value));
        }
    }

    pub(crate) fn radio(&mut self, ui: &mut egui::Ui, id: u64, text: &str, options: &[String], selected: usize) {
        let responses = ui
            .horizontal_wrapped(|ui| {
                ui.label(self.text(text, TextStyle::Body));
                options
                    .iter()
                    .enumerate()
                    .map(|(index, option)| {
                        let response = ui.radio(index == selected, option);
                        self.host.register(&format!("{}.{id}.{index}", self.scope), &response, option);
                        response
                    })
                    .collect::<Vec<_>>()
            })
            .inner;
        let count = responses.len() as i32;
        let mut chosen = None;
        for (index, response) in responses.iter().enumerate() {
            if response.clicked() {
                chosen = Some(index);
            }
            let step = arrow_step(ui, response, egui::Key::ArrowLeft, egui::Key::ArrowRight)
                + arrow_step(ui, response, egui::Key::ArrowUp, egui::Key::ArrowDown);
            if step != 0 {
                let next = (index as i32 + step).rem_euclid(count) as usize;
                responses[next].request_focus();
                chosen = Some(next);
            }
        }
        if let Some(response) = responses.get(selected).or(responses.first()) {
            self.register(id, response, text);
        }
        if let Some(index) = chosen.filter(|index| *index != selected) {
            self.emit(id, UiEventKind::Number(index as f64));
        }
    }

    pub(crate) fn text_area(&mut self, ui: &mut egui::Ui, id: u64, value: &str, hint: &str, rows: u32) {
        let mut text = value.to_string();
        let edit = egui::TextEdit::multiline(&mut text).hint_text(hint).desired_rows(rows.clamp(1, 64) as usize).desired_width(f32::INFINITY);
        let response = ui.add(edit);
        ui.ctx().accesskit_node_builder(response.id, |node| node.set_label(hint));
        self.register(id, &response, hint);
        if response.changed() {
            self.emit(id, UiEventKind::Text(text));
        }
    }

    pub(crate) fn paragraph(&mut self, ui: &mut egui::Ui, spans: &[Span]) {
        let mut job = egui::text::LayoutJob::default();
        for span in spans {
            self.text(&span.text, span.style).append_to(&mut job, ui.style(), egui::FontSelection::Default, egui::Align::Min);
        }
        let text: String = spans.iter().map(|span| span.text.as_str()).collect();
        let response = ui.add(egui::Label::new(job).wrap());
        self.label_registered(&response, &text);
    }

    pub(crate) fn name(&mut self, ui: &egui::Ui, text: &str) {
        if let Some(response) = &self.last {
            ui.ctx().accesskit_node_builder(response.id, |node| node.set_label(text));
        }
    }

    pub(crate) fn focus(&mut self, ui: &egui::Ui, call: u64) {
        let asked = egui::Id::unique(("plugin-focus", self.scope));
        let pass = ui.ctx().cumulative_pass_nr();
        let honoured = ui.data(|data| data.get_temp::<(u64, u64)>(asked)).is_some_and(|(seen, at)| seen == call && at + 1 >= pass);
        if let Some(response) = &self.last {
            if !honoured {
                response.request_focus();
            }
            ui.data_mut(|data| data.insert_temp(asked, (call, pass)));
        }
    }

    pub(crate) fn menu(&mut self, ui: &mut egui::Ui, id: u64, text: Option<&str>, items: &[MenuItem]) {
        let mut picked = None;
        match text {
            Some(text) => {
                let response = ui.menu_button(text, |ui| self.menu_items(ui, id, items, 0, &mut picked)).response;
                self.register(id, &response, text);
            }
            None => {
                let Some(target) = self.last.clone() else { return };
                let by_key = egui::Id::unique(("plugin-menu-key", self.scope, id));
                let key = target.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Menu));
                let keyboard = key || (!target.secondary_clicked() && ui.data(|data| data.get_temp::<bool>(by_key).unwrap_or(false)));
                let popup =
                    if keyboard { Popup::menu(&target).open_memory(key.then_some(SetOpenCommand::Bool(true))) } else { Popup::context_menu(&target) };
                let open = popup.show(|ui| self.menu_items(ui, id, items, 0, &mut picked)).is_some();
                ui.data_mut(|data| data.insert_temp(by_key, keyboard && open));
                self.last = Some(target);
            }
        }
        if let Some(index) = picked {
            self.emit(id, UiEventKind::Number(f64::from(index)));
        }
    }

    pub(crate) fn menu_items(&mut self, ui: &mut egui::Ui, id: u64, items: &[MenuItem], first: u32, picked: &mut Option<u32>) {
        let mut index = first;
        for item in items {
            let at = index;
            index += 1 + count_items(&item.items);
            if item.separator {
                ui.separator();
                continue;
            }
            let response = if item.items.is_empty() {
                let mut button = egui::Button::new(item.text.as_str());
                if let Some(checked) = item.checked {
                    button = button.selected(checked).right_text(if checked { "✔" } else { "" });
                }
                let response = ui.add_enabled(!item.disabled, button);
                if let Some(checked) = item.checked {
                    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::MenuItemCheckBox, !item.disabled, checked, &item.text));
                }
                if response.clicked() {
                    *picked = Some(at);
                }
                response
            } else {
                let button = SubMenuButton::new(item.text.as_str());
                ui.add_enabled_ui(!item.disabled, |ui| button.ui(ui, |ui| self.menu_items(ui, id, &item.items, at + 1, picked)).0).inner
            };
            self.host.register(&format!("{}.{id}.{at}", self.scope), &response, &item.text);
        }
    }

    pub(crate) fn split(&mut self, ui: &mut egui::Ui, id: u64, axis: Axis, first: &[craft_plugin::Widget], second: &[craft_plugin::Widget]) {
        let share_id = egui::Id::unique(("plugin-split", self.scope, id));
        let mut share = ui.data(|data| data.get_temp::<f32>(share_id)).unwrap_or(0.5);
        let bar = ui.spacing().item_spacing.x * 2.0;
        let start = ui.cursor().min;
        let (along, across) = match axis {
            Axis::Horizontal => (ui.available_width(), ui.available_height()),
            Axis::Vertical => (finite_height(ui), ui.available_width()),
        };
        let room = (along - bar).max(0.0);
        let first_len = (room * share).round();
        let pane = |offset: f32, len: f32| match axis {
            Axis::Horizontal => Rect::from_min_size(start + vec2(offset, 0.0), vec2(len, across)),
            Axis::Vertical => Rect::from_min_size(start + vec2(0.0, offset), vec2(across, len)),
        };
        let mut used = [0.0_f32; 2];
        for (index, (rect, children)) in [(pane(0.0, first_len), first), (pane(first_len + bar, room - first_len), second)].into_iter().enumerate() {
            let mut child = ui.new_child(UiBuilder::new().max_rect(rect).id_salt(("pane", index)));
            child.shrink_clip_rect(rect);
            match axis {
                Axis::Horizontal => self.all(&mut child, children),
                Axis::Vertical => {
                    egui::ScrollArea::vertical()
                        .id_salt(("pane-scroll", index))
                        .max_height(rect.height())
                        .auto_shrink([false, false])
                        .show(&mut child, |ui| self.all(ui, children));
                }
            }
            let size = child.min_rect().size();
            used[index] = match axis {
                Axis::Horizontal => size.y,
                Axis::Vertical => size.x,
            };
        }
        let thickness = match axis {
            Axis::Horizontal => used[0].max(used[1]),
            Axis::Vertical => across,
        };
        let divider = match axis {
            Axis::Horizontal => Rect::from_min_size(start + vec2(first_len, 0.0), vec2(bar, thickness)),
            Axis::Vertical => Rect::from_min_size(start + vec2(0.0, first_len), vec2(across, bar)),
        };
        let whole = match axis {
            Axis::Horizontal => Rect::from_min_size(start, vec2(along, thickness)),
            Axis::Vertical => Rect::from_min_size(start, vec2(across, along)),
        };
        ui.allocate_rect(whole, Sense::hover());
        let response = ui.interact(divider, egui::Id::unique(("plugin-split-divider", self.scope, id)), Sense::click_and_drag());
        let name = self.host.tr("Divider");
        response.widget_info(|| egui::WidgetInfo {
            value: Some(f64::from(share * 100.0)),
            ..egui::WidgetInfo::labeled(egui::Role::Splitter, true, &name)
        });
        let (back, forward, moved, cursor) = match axis {
            Axis::Horizontal => (egui::Key::ArrowLeft, egui::Key::ArrowRight, response.drag_delta().x, egui::CursorIcon::ResizeHorizontal),
            Axis::Vertical => (egui::Key::ArrowUp, egui::Key::ArrowDown, response.drag_delta().y, egui::CursorIcon::ResizeVertical),
        };
        let stepped = arrow_step(ui, &response, back, forward) as f32 * SPLIT_STEP;
        if response.dragged() && room > 0.0 {
            share += moved / room;
        }
        share = (share + stepped).clamp(SPLIT_LIMITS.0, SPLIT_LIMITS.1);
        ui.data_mut(|data| data.insert_temp(share_id, share));
        let line = if response.hovered() || response.dragged() || response.has_focus() { self.theme.accent } else { self.theme.border };
        ui.painter().rect_filled(divider.shrink(bar * 0.25), 1.0, line);
        let response = response.on_hover_cursor(cursor);
        self.register(id, &response, "Divider");
    }

    pub(crate) fn curve(&mut self, ui: &mut egui::Ui, id: u64, text: &str, points: &[[f64; 2]]) {
        let sent = egui::Id::unique(("plugin-curve-sent", self.scope, id));
        let shown = pending(ui, sent, &points.to_vec());
        let label = ui.label(self.text(text, TextStyle::Body));
        let Some((response, edited)) = self.host.curve(ui, &format!("{}.{id}", self.scope), &shown) else {
            return self.cannot_edit(ui, id, "This app cannot edit curves here", text);
        };
        let response = response.labelled_by(label.id);
        self.register(id, &response, text);
        if let Some(points) = edited.filter(|edited| *edited != shown) {
            ui.data_mut(|data| data.insert_temp(sent, points.clone()));
            self.emit(id, UiEventKind::Points(points));
        }
    }

    pub(crate) fn gradient(&mut self, ui: &mut egui::Ui, id: u64, text: &str, value: &Gradient) {
        let sent = egui::Id::unique(("plugin-gradient-sent", self.scope, id));
        let shown = pending(ui, sent, value);
        let label = ui.label(self.text(text, TextStyle::Body));
        let Some((response, edited)) = self.host.gradient(ui, &format!("{}.{id}", self.scope), &shown) else {
            return self.cannot_edit(ui, id, "This app cannot edit gradients here", text);
        };
        let response = response.labelled_by(label.id);
        self.register(id, &response, text);
        if let Some(gradient) = edited.filter(|edited| *edited != shown) {
            ui.data_mut(|data| data.insert_temp(sent, gradient.clone()));
            self.emit(id, UiEventKind::Gradient(gradient));
        }
    }

    pub(crate) fn cannot_edit(&mut self, ui: &mut egui::Ui, id: u64, english: &str, text: &str) {
        let response = ui.label(self.text(&self.host.tr(english), TextStyle::Weak));
        self.register(id, &response, text);
    }
}

fn count_items(items: &[MenuItem]) -> u32 {
    items.iter().map(|item| 1 + count_items(&item.items)).sum()
}

fn finite_height(ui: &egui::Ui) -> f32 {
    let height = ui.available_height();
    if height.is_finite() && height > 0.0 { height } else { ui.spacing().interact_size.y * 12.0 }
}
