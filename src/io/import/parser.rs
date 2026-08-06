//! `CommonMark` / GFM → [`MarkdownBlock`] parse (pulldown-cmark).

use pulldown_cmark::{Alignment, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use uuid::Uuid;

use super::MarkdownBlock;
use crate::catalog::{
    InlineKind, InlineSpan, ListKind, OutboundLink, TableCell, TableData, TableRow, TextAlign,
    TextHeader, TextRole,
};

/// Parse the supported `CommonMark` subset (plus GFM tables) into semantic text blocks.
#[must_use]
pub fn parse_markdown_blocks(markdown: &str) -> Vec<MarkdownBlock> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_MATH);
    options.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(markdown, options);
    let mut state = ParseState::default();

    for event in parser {
        match event {
            Event::Start(tag) => state.start(&tag),
            Event::End(tag) => state.end(tag),
            Event::Text(text) | Event::Code(text) => state.push_text(&text),
            Event::InlineMath(text) => state.push_inline_math(&text),
            Event::DisplayMath(text) => state.push_display_math(&text),
            Event::SoftBreak => state.push_break(false),
            Event::HardBreak => state.push_break(true),
            Event::TaskListMarker(done) => {
                state.push_text(if done { "[x] " } else { "[ ] " });
            }
            // Footnotes and rules are deferred. Inline HTML is scanned for `<u>`
            // so Tessprek/HTML underline round-trips into `InlineKind::Underline`.
            Event::Html(_) | Event::FootnoteReference(_) | Event::Rule => {}
            Event::InlineHtml(html) => state.push_inline_html(&html),
        }
    }
    state.finish_active();
    state.blocks
}

#[derive(Default)]
struct ParseState {
    blocks: Vec<MarkdownBlock>,
    active: Option<ActiveBlock>,
    list_stack: Vec<ListKind>,
    blockquote_depth: usize,
    /// Active GFM table builder (`None` outside tables).
    table: Option<TableBuilder>,
}

/// Accumulates pulldown-cmark table events into [`TableData`].
#[derive(Debug, Default)]
struct TableBuilder {
    alignments: Vec<Alignment>,
    rows: Vec<TableRow>,
    current_row: Vec<TableCell>,
    current_cell: String,
    in_head: bool,
    cell_index: usize,
}

struct ActiveBlock {
    header: TextHeader,
    body: String,
    pending_links: Vec<OutboundLink>,
    link_stack: Vec<(u32, String)>,
    underline_stack: Vec<u32>,
}

impl ParseState {
    fn start(&mut self, tag: &Tag<'_>) {
        match tag {
            Tag::Heading { level, .. } => {
                self.begin(TextHeader::heading(heading_level(*level)));
            }
            Tag::Paragraph if self.active.is_none() => {
                let header = if self.blockquote_depth > 0 {
                    header_for_role(TextRole::Blockquote)
                } else {
                    TextHeader::paragraph()
                };
                self.begin(header);
            }
            Tag::List(start) => {
                self.list_stack.push(if start.is_some() {
                    ListKind::Ordered
                } else {
                    ListKind::Bullet
                });
            }
            Tag::Item => {
                let kind = self.list_stack.last().copied().unwrap_or(ListKind::Bullet);
                let depth = u32::try_from(self.list_stack.len()).unwrap_or(1).max(1);
                self.begin(TextHeader::list_item_at(kind, depth));
            }
            Tag::BlockQuote(_) => {
                self.blockquote_depth += 1;
                if self.active.is_none() {
                    self.begin(header_for_role(TextRole::Blockquote));
                }
            }
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(info) => {
                        let first = info.split_whitespace().next().unwrap_or("");
                        (!first.is_empty()).then_some(first.as_ref())
                    }
                    pulldown_cmark::CodeBlockKind::Indented => None,
                };
                self.begin(TextHeader::code_block(lang));
            }
            Tag::Link { dest_url, .. } => {
                if self.table.is_some() {
                    // Flatten link text into the current cell; skip TLNK for tables.
                    return;
                }
                if self.active.is_none() {
                    self.begin(TextHeader::paragraph());
                }
                if let Some(active) = &mut self.active {
                    let start = u32::try_from(active.body.len()).unwrap_or(u32::MAX);
                    active.link_stack.push((start, dest_url.to_string()));
                }
            }
            Tag::Table(alignments) => {
                self.finish_active();
                self.table = Some(TableBuilder {
                    alignments: alignments.clone(),
                    ..TableBuilder::default()
                });
            }
            Tag::TableHead => {
                if let Some(table) = &mut self.table {
                    table.in_head = true;
                }
            }
            Tag::TableRow => {
                if let Some(table) = &mut self.table {
                    table.current_row.clear();
                    table.cell_index = 0;
                }
            }
            Tag::TableCell => {
                if let Some(table) = &mut self.table {
                    table.current_cell.clear();
                }
            }
            // Other inline tags are intentionally flattened; unsupported block
            // tags contribute text to their enclosing supported block when present.
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_) | TagEnd::CodeBlock => self.finish_active(),
            TagEnd::Paragraph => {
                if matches!(
                    self.active.as_ref().map(|b| b.header.role),
                    Some(TextRole::Paragraph | TextRole::Blockquote)
                ) {
                    self.finish_active();
                }
            }
            TagEnd::Item => {
                if matches!(
                    self.active.as_ref().map(|b| b.header.role),
                    Some(TextRole::ListItem)
                ) {
                    self.finish_active();
                }
            }
            TagEnd::List(_) => {
                self.list_stack.pop();
            }
            TagEnd::BlockQuote(_) => {
                self.blockquote_depth = self.blockquote_depth.saturating_sub(1);
                if matches!(
                    self.active.as_ref().map(|b| b.header.role),
                    Some(TextRole::Blockquote)
                ) {
                    self.finish_active();
                }
            }
            TagEnd::Link => {
                if self.table.is_some() {
                    return;
                }
                if let Some(active) = &mut self.active
                    && let Some((start, dest)) = active.link_stack.pop()
                {
                    let end = u32::try_from(active.body.len()).unwrap_or(u32::MAX);
                    let keep = end > start
                        && (crate::catalog::validate_external_uri(&dest).is_ok()
                            || Uuid::parse_str(dest.trim()).is_ok());
                    if keep {
                        active.pending_links.push(OutboundLink { start, end, dest });
                    }
                }
            }
            TagEnd::TableCell => self.finish_table_cell(),
            TagEnd::TableRow => self.finish_table_row(),
            TagEnd::TableHead => {
                // Header cells may appear directly under `TableHead` (no `TableRow`).
                self.finish_table_row();
                if let Some(table) = &mut self.table {
                    table.in_head = false;
                }
            }
            TagEnd::Table => self.finish_table(),
            _ => {}
        }
    }

    fn begin(&mut self, header: TextHeader) {
        self.finish_active();
        self.active = Some(ActiveBlock {
            header,
            body: String::new(),
            pending_links: Vec::new(),
            link_stack: Vec::new(),
            underline_stack: Vec::new(),
        });
    }

    fn push_inline_html(&mut self, html: &str) {
        if self.table.is_some() {
            return;
        }
        let lower = html.trim().to_ascii_lowercase();
        let is_open = lower == "<u>" || lower.starts_with("<u ");
        let is_close = lower == "</u>";
        if !is_open && !is_close {
            return;
        }
        if self.active.is_none() {
            self.begin(TextHeader::paragraph());
        }
        let Some(active) = &mut self.active else {
            return;
        };
        if is_open {
            let start = u32::try_from(active.body.len()).unwrap_or(u32::MAX);
            active.underline_stack.push(start);
        } else if let Some(start) = active.underline_stack.pop() {
            let end = u32::try_from(active.body.len()).unwrap_or(u32::MAX);
            if end > start {
                active.header.spans.push(InlineSpan {
                    start,
                    end,
                    kind: InlineKind::Underline,
                });
            }
        }
    }

    fn push_text(&mut self, text: &str) {
        if let Some(table) = &mut self.table {
            table.current_cell.push_str(text);
            return;
        }
        if self.active.is_none() && !text.trim().is_empty() {
            self.begin(TextHeader::paragraph());
        }
        if let Some(active) = &mut self.active {
            active.body.push_str(text);
        }
    }

    fn push_inline_math(&mut self, tex: &str) {
        if self.table.is_some() {
            self.push_text(tex);
            return;
        }
        if self.active.is_none() {
            self.begin(TextHeader::paragraph());
        }
        if let Some(active) = &mut self.active {
            let start = u32::try_from(active.body.len()).unwrap_or(u32::MAX);
            active.body.push_str(tex);
            let end = u32::try_from(active.body.len()).unwrap_or(u32::MAX);
            active.header.spans.push(InlineSpan {
                start,
                end,
                kind: InlineKind::Math {
                    tex: tex.to_owned(),
                },
            });
        }
    }

    fn push_display_math(&mut self, tex: &str) {
        if self.table.is_some() {
            self.push_text(tex);
            return;
        }
        self.finish_active();
        self.begin(TextHeader::math());
        self.push_text(tex);
        self.finish_active();
    }

    fn push_break(&mut self, hard: bool) {
        if let Some(table) = &mut self.table {
            table.current_cell.push(if hard { '\n' } else { ' ' });
            return;
        }
        if let Some(active) = &mut self.active {
            active.body.push(if hard { '\n' } else { ' ' });
        }
    }

    fn finish_table_cell(&mut self) {
        let Some(table) = &mut self.table else {
            return;
        };
        let align = table
            .alignments
            .get(table.cell_index)
            .copied()
            .and_then(alignment_to_text_align);
        table.current_row.push(TableCell {
            text: table.current_cell.trim().to_owned(),
            spans: Vec::new(),
            align,
            is_header: table.in_head,
            rowspan: None,
            colspan: None,
        });
        table.current_cell.clear();
        table.cell_index += 1;
    }

    fn finish_table_row(&mut self) {
        let Some(table) = &mut self.table else {
            return;
        };
        if !table.current_row.is_empty() {
            table.rows.push(TableRow {
                cells: std::mem::take(&mut table.current_row),
            });
        }
        table.cell_index = 0;
    }

    fn finish_table(&mut self) {
        let Some(table) = self.table.take() else {
            return;
        };
        if table.rows.is_empty() {
            return;
        }
        let data = TableData { rows: table.rows };
        self.blocks.push(MarkdownBlock {
            header: TextHeader::table(data),
            body: String::new(),
            pending_links: Vec::new(),
        });
    }

    fn finish_active(&mut self) {
        if let Some(active) = self.active.take() {
            let raw = active.body;
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return;
            }
            let lead = raw.len() - raw.trim_start().len();
            let keep_end = lead + trimmed.len();
            let shift = |offset: u32| -> Option<u32> {
                let o = offset as usize;
                if o < lead || o > keep_end {
                    return None;
                }
                u32::try_from(o - lead).ok()
            };
            let mut header = active.header;
            header.spans.retain_mut(|span| {
                let Some(start) = shift(span.start) else {
                    return false;
                };
                let Some(end) = shift(span.end) else {
                    return false;
                };
                if start >= end {
                    return false;
                }
                span.start = start;
                span.end = end;
                true
            });
            let mut pending_links = Vec::new();
            for link in active.pending_links {
                let Some(start) = shift(link.start) else {
                    continue;
                };
                let Some(end) = shift(link.end) else {
                    continue;
                };
                if start >= end {
                    continue;
                }
                pending_links.push(OutboundLink {
                    start,
                    end,
                    dest: link.dest,
                });
            }
            self.blocks.push(MarkdownBlock {
                header,
                body: trimmed.to_owned(),
                pending_links,
            });
        }
    }
}

fn header_for_role(role: TextRole) -> TextHeader {
    TextHeader::with_role(role)
}

fn alignment_to_text_align(align: Alignment) -> Option<TextAlign> {
    match align {
        Alignment::None => None,
        Alignment::Left => Some(TextAlign::Start),
        Alignment::Center => Some(TextAlign::Center),
        Alignment::Right => Some(TextAlign::End),
    }
}

fn heading_level(level: HeadingLevel) -> u32 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}
