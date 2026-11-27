use craft_plugin::{MenuItem as Item, UiEventKind};
use craft_ui::buttons::Button;
use craft_ui::contract::{Props as _, Show as _, ShowContainer as _, Trigger};
use craft_ui::display::Divider;
use craft_ui::menu::{Menu, MenuItem};

use crate::Painter;

fn size(item: &Item) -> u32 {
    1 + item.items.iter().map(size).sum::<u32>()
}

impl Painter<'_> {
    pub(crate) fn menu(&mut self, ui: &mut egui::Ui, id: u64, text: Option<&str>, items: &[Item]) {
        let opener = match text {
            Some(text) => {
                let button = Button::new(text.to_string()).id_salt(self.salt(id)).show(ui);
                Some((Menu::new(text.to_string()).opened_by(&button.response, Trigger::Press), button.response))
            }
            None => self.last.clone().map(|last| (Menu::context("Context menu", &last), last)),
        };
        let Some((menu, opener)) = opener else { return };
        menu.id_salt((self.salt(id), "menu")).show(ui, |ui, _| self.rows(ui, id, items, 0));
        self.last = Some(opener);
    }

    fn rows(&mut self, ui: &mut egui::Ui, id: u64, items: &[Item], first: u32) {
        let mut at = first;
        for item in items {
            let index = at;
            at += size(item);
            if item.separator {
                Divider::horizontal().show(ui);
            } else if item.items.is_empty() {
                let row = MenuItem::new(item.text.clone()).enabled(!item.disabled);
                let row = match item.checked {
                    Some(on) => row.check(on),
                    None => row,
                };
                if row.show(ui).ran {
                    self.emit(id, UiEventKind::Number(f64::from(index)));
                }
            } else {
                let head = MenuItem::submenu(item.text.clone()).enabled(!item.disabled).show(ui);
                Menu::submenu(item.text.clone(), &head.response).show(ui, |ui, _| self.rows(ui, id, &item.items, index + 1));
            }
        }
    }
}
