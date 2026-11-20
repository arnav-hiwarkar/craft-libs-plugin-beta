use std::collections::HashSet;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

pub(crate) struct Document {
    pub(crate) anchors: HashSet<String>,
    pub(crate) links: Vec<Link>,
    pub(crate) code_spans: Vec<CodeSpan>,
    pub(crate) prose: Vec<Prose>,
    pub(crate) tables: Vec<Table>,
}

pub(crate) struct Prose {
    pub(crate) line: usize,
    pub(crate) text: String,
    pub(crate) first_cell_of_table: Option<usize>,
}

pub(crate) struct Link {
    pub(crate) line: usize,
    pub(crate) dest: String,
}

pub(crate) struct CodeSpan {
    pub(crate) line: usize,
    pub(crate) text: String,
}

#[derive(Default)]
pub(crate) struct Table {
    pub(crate) header: Vec<Cell>,
    pub(crate) rows: Vec<Row>,
}

pub(crate) struct Row {
    pub(crate) line: usize,
    pub(crate) cells: Vec<Cell>,
}

#[derive(Default)]
pub(crate) struct Cell {
    pub(crate) text: String,
    pub(crate) links: Vec<String>,
}

impl Table {
    pub(crate) fn first_header(&self) -> &str {
        self.header.first().map_or("", |cell| cell.text.as_str())
    }

    pub(crate) fn column(&self, name: &str) -> Option<usize> {
        self.header.iter().position(|cell| cell.text == name)
    }
}

impl Row {
    pub(crate) fn cell(&self, column: Option<usize>) -> Option<&Cell> {
        column.and_then(|index| self.cells.get(index))
    }

    pub(crate) fn text(&self, column: Option<usize>) -> &str {
        self.cell(column).map_or("", |cell| cell.text.trim())
    }
}

pub(crate) fn parse(text: &str) -> Document {
    let line_starts: Vec<usize> = text.match_indices('\n').map(|(offset, _)| offset).collect();
    let line_of = |offset: usize| line_starts.partition_point(|&newline| newline < offset) + 1;
    let mut document = Document { anchors: HashSet::new(), links: Vec::new(), code_spans: Vec::new(), prose: Vec::new(), tables: Vec::new() };
    let mut in_code_block = false;
    let mut heading: Option<String> = None;
    let mut table: Option<Table> = None;
    let mut row: Option<Row> = None;
    let mut cell: Option<Cell> = None;
    for (event, range) in Parser::new_ext(text, Options::ENABLE_TABLES).into_offset_iter() {
        let line = line_of(range.start);
        match event {
            Event::Start(Tag::Heading { .. }) => heading = Some(String::new()),
            Event::End(TagEnd::Heading(_)) => {
                if let Some(title) = heading.take() {
                    add_anchor(&mut document.anchors, &slug(&title));
                }
            }
            Event::Start(Tag::Table(_)) => table = Some(Table::default()),
            Event::End(TagEnd::Table) => document.tables.extend(table.take()),
            Event::Start(Tag::TableHead | Tag::TableRow) => row = Some(Row { line, cells: Vec::new() }),
            Event::End(TagEnd::TableHead) => {
                if let (Some(table), Some(row)) = (table.as_mut(), row.take()) {
                    table.header = row.cells;
                }
            }
            Event::Start(Tag::CodeBlock(_)) => in_code_block = true,
            Event::End(TagEnd::CodeBlock) => in_code_block = false,
            Event::End(TagEnd::TableRow) => {
                if let (Some(table), Some(row)) = (table.as_mut(), row.take()) {
                    table.rows.push(row);
                }
            }
            Event::Start(Tag::TableCell) => cell = Some(Cell::default()),
            Event::End(TagEnd::TableCell) => {
                if let (Some(row), Some(cell)) = (row.as_mut(), cell.take()) {
                    row.cells.push(cell);
                }
            }
            Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) => {
                if let Some(cell) = cell.as_mut() {
                    cell.links.push(dest_url.to_string());
                }
                document.links.push(Link { line, dest: dest_url.to_string() });
            }
            Event::Code(code) => {
                push_text(&mut heading, &mut cell, &code);
                document.code_spans.push(CodeSpan { line, text: code.to_string() });
            }
            Event::Text(text) => {
                push_text(&mut heading, &mut cell, &text);
                if !in_code_block {
                    let in_body = table.as_ref().is_some_and(|table| !table.header.is_empty());
                    let first_cell = in_body && row.as_ref().is_some_and(|row| row.cells.is_empty()) && cell.is_some();
                    let first_cell_of_table = first_cell.then_some(document.tables.len());
                    document.prose.push(Prose { line, text: text.to_string(), first_cell_of_table });
                }
            }
            Event::SoftBreak | Event::HardBreak => push_text(&mut heading, &mut cell, " "),
            Event::InlineHtml(_) => push_text(&mut None, &mut cell, " "),
            _ => {}
        }
    }
    document
}

fn push_text(heading: &mut Option<String>, cell: &mut Option<Cell>, text: &str) {
    if let Some(heading) = heading.as_mut() {
        heading.push_str(text);
    }
    if let Some(cell) = cell.as_mut() {
        cell.text.push_str(text);
    }
}

fn slug(title: &str) -> String {
    title
        .trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            '-' | '_' => Some(c),
            _ if c.is_alphanumeric() => Some(c),
            _ => None,
        })
        .collect()
}

fn add_anchor(anchors: &mut HashSet<String>, base: &str) {
    let mut anchor = base.to_owned();
    let mut suffix = 0;
    while anchors.contains(&anchor) {
        suffix += 1;
        anchor = format!("{base}-{suffix}");
    }
    anchors.insert(anchor);
}
