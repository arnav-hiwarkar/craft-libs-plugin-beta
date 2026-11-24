use std::collections::BTreeMap;

use craft_plugin::{Form, ParamKind, Value};

use crate::Host;
use crate::panel::finite_or;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Ok,
    Cancel,
}

pub fn draw_form<H: Host>(ui: &mut egui::Ui, form: &Form, values: &mut BTreeMap<String, Value>, scope: &str, host: &mut H) -> Option<Answer> {
    egui::Grid::new((scope, "fields")).num_columns(2).show(ui, |ui| {
        for param in &form.fields {
            let label = ui.label(param.name.as_str());
            let value = values.entry(param.id.clone()).or_insert_with(|| param.default_value([1.0, 1.0]));
            let response = field(ui, &param.kind, value, host, egui::IdSalt::new((scope, &param.id))).labelled_by(label.id);
            host.register(&format!("{scope}.{}", param.id), &response, &param.name);
            ui.end_row();
        }
    });
    ui.add_space(8.0);
    let mut answer = None;
    ui.horizontal(|ui| {
        for (choice, text, id) in [(Answer::Ok, "OK", "ok"), (Answer::Cancel, "Cancel", "cancel")] {
            let response = ui.button(host.tr(text));
            host.register(&format!("{scope}.{id}"), &response, text);
            if response.clicked() {
                answer = Some(choice);
            }
        }
    });
    let key = |key| ui.input(|i| i.key_pressed(key));
    answer.or(if key(egui::Key::Escape) { Some(Answer::Cancel) } else { None })
}

fn field<H: Host>(ui: &mut egui::Ui, kind: &ParamKind, value: &mut Value, host: &H, id: egui::IdSalt) -> egui::Response {
    match (kind, value) {
        (ParamKind::Slider { min, max, .. }, Value::Number(n)) => ui.add(egui::Slider::new(n, finite_or(*min, 0.0)..=finite_or(*max, 1.0))),
        (ParamKind::Percent { .. }, Value::Number(n)) => ui.add(egui::DragValue::new(n).suffix("%")),
        (ParamKind::Angle { .. }, Value::Number(n)) => ui.add(egui::DragValue::new(n).suffix("°")),
        (ParamKind::Checkbox { .. }, Value::Bool(on)) => ui.checkbox(on, ""),
        (ParamKind::Popup { options, .. }, Value::Number(n)) => {
            let at = *n as usize;
            let mut chosen = at;
            let selected = options.get(at).cloned().unwrap_or_default();
            let mut response = host.combo_box(ui, id, "", selected, &mut |ui| {
                for (index, option) in options.iter().enumerate() {
                    if ui.selectable_label(index == chosen, option.as_str()).clicked() {
                        chosen = index;
                    }
                }
            });
            if chosen != at {
                *n = chosen as f64;
                response.mark_changed();
            }
            response
        }
        (ParamKind::Point { .. } | ParamKind::Point3 { .. }, Value::List(items)) => {
            let inner = ui.horizontal(|ui| items.iter_mut().map(|item| ui.add(egui::DragValue::new(item))).reduce(|a, b| a | b));
            inner.inner.unwrap_or(inner.response)
        }
        (ParamKind::Color { .. }, Value::List(items)) if items.len() == 4 => {
            let mut color = [items[0] as f32, items[1] as f32, items[2] as f32, items[3] as f32];
            let response = ui.color_edit_button_rgba_unmultiplied(&mut color);
            if response.changed() {
                *items = color.iter().map(|&channel| f64::from(channel)).collect();
            }
            response
        }
        (ParamKind::Text { .. } | ParamKind::Custom { .. }, Value::Text(text)) => ui.text_edit_singleline(text),
        _ => ui.label("—"),
    }
}
