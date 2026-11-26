use std::borrow::Cow;
use std::ops::Range;

use craft_behavior::{ColumnKey, ColumnWidth, DisplayOrder, ItemSource, Selection};
use craft_plugin::{Column, Item, LIST_WINDOW, ListEvent, MAX_LIST_ITEMS, UiEventKind};
use craft_ui::contract::{Bind, CollectionEvent, ColumnSpec, Props as _, SelectMode, ShowCollection as _};
use craft_ui::list::{List, ListRow};
use craft_ui::runtime::memory::with_session;
use craft_ui::table::Table;

use crate::{Painter, icons};

pub(crate) struct Rows<'a> {
    pub(crate) columns: &'a [Column],
    pub(crate) count: u32,
    pub(crate) first: u32,
    pub(crate) items: &'a [Item],
    pub(crate) selected: &'a [u32],
    pub(crate) multi: bool,
}

const KEYS: [&str; 16] = ["c0", "c1", "c2", "c3", "c4", "c5", "c6", "c7", "c8", "c9", "c10", "c11", "c12", "c13", "c14", "c15"];

#[derive(Clone, Default)]
struct Kept {
    given: Vec<u32>,
    selection: Selection<u32>,
    asked: Option<Range<u32>>,
}

impl Rows<'_> {
    fn item(&self, row: u32) -> Option<&Item> {
        row.checked_sub(self.first).and_then(|at| self.items.get(at as usize))
    }

    fn holds(&self, shown: &Range<u32>) -> bool {
        shown.start >= self.first && shown.end <= self.first + self.items.len() as u32
    }

    fn window(&self, shown: &Range<u32>) -> Range<u32> {
        let room = LIST_WINDOW.min(MAX_LIST_ITEMS);
        let first = shown.start.saturating_sub(room.saturating_sub(shown.len() as u32) / 2).min(self.count.saturating_sub(room));
        first..(first + room).min(self.count)
    }

    fn row<'r>(&self, row: u32) -> ListRow<'r> {
        let Some(item) = self.item(row) else { return ListRow::new(String::new()) };
        let name = item.cells.first().cloned().unwrap_or_default();
        let mut shown = ListRow::new(name);
        for (cell, key) in item.cells.iter().zip(KEYS).skip(1).take(self.columns.len().saturating_sub(1)) {
            shown = shown.cell(ColumnKey::new(key), cell.clone());
        }
        if let Some(icon) = &item.icon {
            shown = shown.icon(icons::spec(icon, item.cells.first().map_or("", String::as_str)));
        }
        if item.weak {
            shown = shown.ink(craft_ui::list::RowInk::Muted);
        }
        shown
    }
}

impl DisplayOrder<u32> for Rows<'_> {
    fn len(&self) -> usize {
        self.count as usize
    }

    fn key(&self, at: usize) -> u32 {
        at as u32
    }

    fn index_of(&self, key: &u32) -> Option<usize> {
        (*key < self.count).then_some(*key as usize)
    }

    fn depth(&self, at: usize) -> usize {
        self.item(at as u32).map_or(0, |item| usize::from(item.depth))
    }

    fn has_children(&self, at: usize) -> bool {
        self.item(at as u32).is_some_and(|item| item.open.is_some())
    }

    fn open(&self, at: usize) -> bool {
        self.item(at as u32).is_some_and(|item| item.open == Some(true))
    }

    fn text(&self, at: usize) -> Option<Cow<'_, str>> {
        self.item(at as u32).and_then(|item| item.cells.first()).map(|text| Cow::Borrowed(text.as_str()))
    }
}

impl ItemSource<u32> for Rows<'_> {}

fn width(width: craft_plugin::ColumnWidth) -> ColumnWidth {
    match width {
        craft_plugin::ColumnWidth::Auto => ColumnWidth::Content,
        craft_plugin::ColumnWidth::Narrow => ColumnWidth::Controls(4.0),
        craft_plugin::ColumnWidth::Wide => ColumnWidth::Controls(10.0),
        craft_plugin::ColumnWidth::Fill => ColumnWidth::fill(1.0),
    }
}

fn sorted(mut rows: Vec<u32>) -> Vec<u32> {
    rows.sort_unstable();
    rows.dedup();
    rows
}

impl Painter<'_> {
    pub(crate) fn list(&mut self, ui: &mut egui::Ui, id: u64, rows: &Rows<'_>) {
        let memory = egui::Id::unique(("craft-plugin-ui-framework.list", self.scope, id));
        let given = sorted(rows.selected.to_vec());
        let mut kept: Kept = with_session(ui.ctx(), memory, |kept: &mut Kept| kept.clone());
        if kept.given != given {
            let lead = given.last().copied();
            kept.selection = Selection::new(given.clone(), lead, given.first().copied());
            kept.given = given.clone();
        }
        let mode = if rows.multi { SelectMode::Multi } else { SelectMode::Single };
        let mut selection = kept.selection.clone();
        let out = if rows.columns.is_empty() {
            List::new(String::new()).id_salt(self.salt(id)).select(mode).tree(true).selection(Bind::bound(&mut selection)).show_items(
                ui,
                rows,
                &mut |row, _| rows.row(*row),
            )
        } else {
            let specs = rows
                .columns
                .iter()
                .zip(KEYS)
                .map(|(column, key)| ColumnSpec::new(ColumnKey::new(key), column.title.clone(), width(column.width)))
                .collect();
            Table::new(String::new(), specs).id_salt(self.salt(id)).select(mode).selection(Bind::bound(&mut selection)).show_items(
                ui,
                rows,
                &mut |row, _| rows.row(*row),
            )
        };
        let before = sorted(kept.selection.items.clone());
        let after = sorted(selection.items.clone());
        if after != before {
            self.emit(id, UiEventKind::List(ListEvent::Select(after)));
        }
        for event in &out.events {
            match event {
                CollectionEvent::Activate(row) => self.emit(id, UiEventKind::List(ListEvent::Activate(*row))),
                CollectionEvent::Open(change) => self.emit(id, UiEventKind::List(ListEvent::Toggle { row: change.key, open: change.open })),
                _ => {}
            }
        }
        let shown = out.visible.start as u32..out.visible.end as u32;
        if !shown.is_empty() && !rows.holds(&shown) && kept.asked.as_ref().is_none_or(|asked| !(asked.start <= shown.start && shown.end <= asked.end))
        {
            let window = rows.window(&shown);
            self.emit(id, UiEventKind::List(ListEvent::Show { first: window.start, count: window.len() as u32 }));
            kept.asked = Some(window);
        }
        if rows.holds(&shown) {
            kept.asked = None;
        }
        kept.selection = selection;
        with_session(ui.ctx(), memory, |stored: &mut Kept| *stored = kept);
    }
}
