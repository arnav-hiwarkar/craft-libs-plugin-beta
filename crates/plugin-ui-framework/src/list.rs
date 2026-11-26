use std::borrow::Cow;
use std::ops::Range;

use craft_behavior::EditEvent;
use craft_behavior::{ColumnKey, ColumnWidth, DisplayOrder, ItemSource, Selection};
use craft_plugin::{Column, Item, LIST_WINDOW, ListEdit, ListEvent, MAX_LIST_ITEMS, UiEventKind};
use craft_ui::contract::{
    Bind, CollectionEvent, ColumnEvent, ColumnSort, ColumnSpec, Props as _, ReorderMove, ReorderRule, SelectMode, ShowCollection as _,
};
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
    pub(crate) edit: &'a ListEdit,
}

const KEYS: [&str; 16] = ["c0", "c1", "c2", "c3", "c4", "c5", "c6", "c7", "c8", "c9", "c10", "c11", "c12", "c13", "c14", "c15"];

#[derive(Clone, Default)]
struct Kept {
    lead: Option<u32>,
    anchor: Option<u32>,
    asked: Option<Range<u32>>,
    sent: Option<(Vec<u32>, Vec<u32>)>,
    told: Option<(u32, bool)>,
    sorting: Option<(u32, bool)>,
    owner: Option<egui::Id>,
}

const ALT: craft_keymap::Mods = craft_keymap::Mods { alt: true, ..craft_keymap::Mods::NONE };
const ALT_UP: craft_keymap::Chord = craft_keymap::Chord::new(ALT, craft_keymap::Key::Up);
const ALT_DOWN: craft_keymap::Chord = craft_keymap::Chord::new(ALT, craft_keymap::Key::Down);

fn stepped(moving: &[u32], down: bool, count: u32) -> Option<u32> {
    let (low, high) = (*moving.first()?, *moving.last()?);
    match down {
        false if low > 0 => Some(low - 1),
        true if high + 1 < count => Some(high + 2),
        _ => None,
    }
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
        let mut shown = ListRow::new(name).depth(usize::from(item.depth));
        if let Some(open) = item.open {
            shown = shown.group(open);
        }
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

fn committed<T>(event: &EditEvent<T>) -> Option<&T> {
    match event {
        EditEvent::Commit { after, .. } | EditEvent::Set { after, .. } => Some(after),
        _ => None,
    }
}

fn moved(change: &ReorderMove<u32>, count: u32) -> ListEvent {
    let rows = sorted(change.keys.clone());
    let to = (0..count).filter(|row| !rows.contains(row)).nth(change.to).unwrap_or(count);
    ListEvent::Move { rows, to }
}

fn column_of(key: &craft_behavior::ColumnKey) -> Option<u32> {
    KEYS.iter().position(|held| ColumnKey::new(held) == *key).map(|at| at as u32)
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
        let held = |key: Option<u32>| key.filter(|row| *row < rows.count && (given.is_empty() || given.contains(row)));
        let lead = held(kept.lead).or_else(|| given.last().copied()).or((rows.count > 0).then_some(0));
        let anchor = held(kept.anchor).or_else(|| given.first().copied());
        let before = Selection::new(given.clone(), lead, anchor);
        let mode = if rows.multi { SelectMode::Multi } else { SelectMode::Single };
        let mut selection = before.clone();
        let edit = rows.edit;
        let reorder = edit.reorder.then(ReorderRule::new);
        if let Some(owner) = kept.owner.filter(|_| edit.reorder) {
            craft_ui::runtime::keys::claim(ui.ctx(), owner, ALT_UP);
            craft_ui::runtime::keys::claim(ui.ctx(), owner, ALT_DOWN);
        }
        let out = if rows.columns.is_empty() {
            let list = List::new(String::new()).id_salt(self.salt(id)).select(mode).tree(true).rename(edit.rename);
            let list = match reorder {
                Some(rule) => list.reorder(rule),
                None => list,
            };
            list.selection(Bind::bound(&mut selection)).show_items(ui, rows, &mut |row, _| rows.row(*row))
        } else {
            let specs = rows
                .columns
                .iter()
                .zip(KEYS)
                .map(|(column, key)| {
                    let spec = ColumnSpec::new(ColumnKey::new(key), column.title.clone(), width(column.width));
                    if edit.sortable { spec.sortable() } else { spec }
                })
                .collect();
            if kept.told != edit.sorted {
                (kept.told, kept.sorting) = (edit.sorted, None);
            }
            let shown = kept.sorting.or(edit.sorted);
            let sort = shown.and_then(|(column, descending)| KEYS.get(column as usize).map(|key| ColumnSort::new(ColumnKey::new(key), descending)));
            let table = Table::new(String::new(), specs).id_salt(self.salt(id)).select(mode).rename(edit.rename).sort(sort);
            let table = match reorder {
                Some(rule) => table.reorder(rule),
                None => table,
            };
            table.selection(Bind::bound(&mut selection)).show_items(ui, rows, &mut |row, _| rows.row(*row))
        };
        self.seen(ui, &out.response, crate::Keys::Panel);
        kept.owner = Some(out.response.id);
        if edit.reorder
            && let Some(lead) = selection.lead
        {
            let moving = if selection.items.contains(&lead) { sorted(selection.items.clone()) } else { vec![lead] };
            for (chord, down) in [(ALT_UP, false), (ALT_DOWN, true)] {
                if craft_ui::runtime::keys::take_claimed(ui.ctx(), out.response.id, chord)
                    && let Some(to) = stepped(&moving, down, rows.count)
                {
                    self.emit(id, UiEventKind::List(ListEvent::Move { rows: moving.clone(), to }));
                }
            }
        }
        let after = sorted(selection.items.clone());
        let change = (given.clone(), after.clone());
        if after != given && kept.sent.as_ref() != Some(&change) {
            self.emit(id, UiEventKind::List(ListEvent::Select(after)));
            kept.sent = Some(change);
        }
        for event in &out.events {
            match event {
                CollectionEvent::Activate(row) => self.emit(id, UiEventKind::List(ListEvent::Activate(*row))),
                CollectionEvent::Open(change) => self.emit(id, UiEventKind::List(ListEvent::Toggle { row: change.key, open: change.open })),
                CollectionEvent::Rename(rename) => {
                    if let Some(rename) = committed(rename) {
                        self.emit(id, UiEventKind::List(ListEvent::Rename { row: rename.key, text: rename.name.clone() }));
                    }
                }
                CollectionEvent::Reorder(change) => {
                    if let Some(change) = committed(change) {
                        self.emit(id, UiEventKind::List(moved(change, rows.count)));
                    }
                }
                CollectionEvent::Column(ColumnEvent::Sort { key, descending, .. }) => {
                    if let Some(column) = column_of(key) {
                        kept.sorting = Some((column, *descending));
                        self.emit(id, UiEventKind::List(ListEvent::Sort { column, descending: *descending }));
                    }
                }
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
        (kept.lead, kept.anchor) = (selection.lead, selection.anchor);
        with_session(ui.ctx(), memory, |stored: &mut Kept| *stored = kept);
    }
}
