//! Foreign-format importers under [`crate::io`].
//!
//! v0 implements the `CommonMark` subset from `docs/decisions.md`: ATX headings,
//! paragraphs, lists, fenced code, and blockquotes, plus GFM pipe tables
//! (`Options::ENABLE_TABLES` → [`TextRole::Table`] + [`TableData`]). Inline
//! presentation is parsed once and flattened into clean canonical text.
//!
//! Obsidian front matter (`id` / tags / aliases), deterministic `doc_id` seeds,
//! and `[[wikilink]]` rewrite helpers support vault batch import.
//!
//! HTML import lives in [`html`].

pub mod html;

use std::path::{Path, PathBuf};

use pulldown_cmark::{Alignment, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use crate::catalog::{
    DocumentCatalog, InlineKind, InlineSpan, ListKind, OutboundLink, TableCell, TableData,
    TableRow, TesFile, TesWriterSession, TextAlign, TextHeader, TextRole, doc_id_from_seed,
};
use crate::error::{Result, TesError};
use crate::layout::DocKind;

pub use html::{HtmlImportOptions, HtmlImportReport, import_html_v0, parse_html_blocks};

/// Shared wikilink name → catalog `doc_id` resolver (vault batch import).
pub type WikilinkResolver = std::sync::Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Options for Markdown → `.tes` import.
#[derive(Clone)]
pub struct MarkdownImportOptions {
    /// Kind stored in the superblock and catalog.
    pub doc_kind: DocKind,
    /// Catalog title. When absent, front matter, first heading, or filename wins.
    pub title: Option<String>,
    /// Stable document UUID string. When absent: keep existing output catalog
    /// id (D2), else `UUIDv5` from [`Self::doc_id_seed`], else random.
    pub doc_id: Option<String>,
    /// Seed for deterministic [`doc_id_from_seed`] when `doc_id` is absent and
    /// the output file does not already exist.
    pub doc_id_seed: Option<String>,
    /// Catalog tags (merged with front matter tags; options win on duplicates
    /// by extending after front matter).
    pub tags: Vec<String>,
    /// Catalog category override (e.g. vault top-level folder).
    pub category: Option<String>,
    /// Catalog section override (path under category, e.g. `Books/Authors`).
    pub section: Option<String>,
    /// Catalog aliases (merged with front matter aliases).
    pub aliases: Vec<String>,
    /// Catalog slug override (else front matter `id:`).
    pub slug: Option<String>,
    /// When true, [`Self::slug`] is authoritative even when `None` (skips front matter id).
    pub slug_override: bool,
    /// When set, rewrite resolved `[[wikilinks]]` to Markdown UUID links before parse.
    pub wikilink_resolver: Option<WikilinkResolver>,
}

impl Default for MarkdownImportOptions {
    fn default() -> Self {
        Self {
            doc_kind: DocKind::Document,
            title: None,
            doc_id: None,
            doc_id_seed: None,
            tags: Vec::new(),
            category: None,
            section: None,
            aliases: Vec::new(),
            slug: None,
            slug_override: false,
            wikilink_resolver: None,
        }
    }
}

impl std::fmt::Debug for MarkdownImportOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarkdownImportOptions")
            .field("doc_kind", &self.doc_kind)
            .field("title", &self.title)
            .field("doc_id", &self.doc_id)
            .field("doc_id_seed", &self.doc_id_seed)
            .field("tags", &self.tags)
            .field("category", &self.category)
            .field("section", &self.section)
            .field("aliases", &self.aliases)
            .field("slug", &self.slug)
            .field("slug_override", &self.slug_override)
            .field(
                "wikilink_resolver",
                &self.wikilink_resolver.as_ref().map(|_| "<fn>"),
            )
            .finish()
    }
}

/// Parsed Obsidian-style YAML front matter (subset).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarkdownFrontMatter {
    /// Optional title.
    pub title: Option<String>,
    /// Obsidian string id (maps to slug / `doc_id` seed).
    pub id: Option<String>,
    /// Tags list.
    pub tags: Vec<String>,
    /// Aliases list.
    pub aliases: Vec<String>,
}

/// Result summary for a completed import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownImportReport {
    /// Source Markdown path.
    pub input: PathBuf,
    /// Sealed `.tes` path.
    pub output: PathBuf,
    /// Stable document id written to the catalog.
    pub doc_id: String,
    /// Catalog title.
    pub title: String,
    /// Number of semantic text chunks written.
    pub chunk_count: usize,
    /// Catalog slug when set.
    pub slug: Option<String>,
}

/// One semantic block produced by the Markdown parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownBlock {
    /// Tessera semantic header.
    pub header: TextHeader,
    /// Clean body text.
    pub body: String,
    /// Outbound links over [`Self::body`] (written to `TLNK` on seal).
    pub pending_links: Vec<OutboundLink>,
}

/// Import a Markdown file and seal a `.tes` document.
///
/// # Errors
///
/// Returns [`TesError::Io`] if the source cannot be read or the `.tes` cannot be written,
/// [`TesError::InvalidDocId`] if `options.doc_id` is not a UUID, or catalog/session
/// errors from [`TesWriterSession`].
pub fn import_markdown_v0(
    input: impl AsRef<Path>,
    output: impl AsRef<Path>,
    options: &MarkdownImportOptions,
) -> Result<MarkdownImportReport> {
    let input = input.as_ref();
    let output = output.as_ref();
    let source = std::fs::read_to_string(input)?;
    let (front, markdown) = parse_front_matter(&source);
    let markdown = if let Some(resolver) = options.wikilink_resolver.as_ref() {
        rewrite_wikilinks(markdown, resolver.as_ref())
    } else {
        markdown.to_owned()
    };
    let blocks = parse_markdown_blocks(&markdown);

    let title = options
        .title
        .clone()
        .or(front.title.clone())
        .or_else(|| first_heading(&blocks))
        .or_else(|| {
            input
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "Untitled".to_owned());

    let seed = options
        .doc_id_seed
        .clone()
        .or_else(|| front.id.clone())
        .or_else(|| {
            input
                .file_name()
                .and_then(|s| s.to_str())
                .map(str::to_owned)
        });
    let doc_id = resolve_import_doc_id(output, options.doc_id.as_deref(), seed.as_deref())?;

    let mut tags = front.tags.clone();
    extend_unique(&mut tags, &options.tags);
    let mut aliases = front.aliases.clone();
    extend_unique(&mut aliases, &options.aliases);
    let slug = if options.slug_override {
        options.slug.clone()
    } else {
        options.slug.clone().or(front.id.clone())
    };

    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|err| std::io::Error::other(format!("format import timestamp: {err}")))?;

    let mut catalog = DocumentCatalog::new(&doc_id, &title, &now, &now, options.doc_kind);
    catalog.tags = tags;
    catalog.category.clone_from(&options.category);
    catalog.section.clone_from(&options.section);
    catalog.aliases = aliases;
    catalog.slug.clone_from(&slug);

    let _ = std::fs::remove_file(output);
    let mut session = TesWriterSession::create(output, options.doc_kind);
    session.set_catalog(catalog)?;
    seal_text_blocks(&mut session, &blocks)?;
    session.commit()?;

    Ok(MarkdownImportReport {
        input: input.to_path_buf(),
        output: output.to_path_buf(),
        doc_id,
        title,
        chunk_count: blocks.len(),
        slug,
    })
}

/// Resolve `doc_id` for import: explicit option, else keep existing output catalog
/// (D2), else `UUIDv5` from seed, else random.
///
/// # Errors
///
/// Returns [`TesError::InvalidDocId`] when an explicit id is not a UUID.
pub fn resolve_import_doc_id(
    output: &Path,
    explicit: Option<&str>,
    seed: Option<&str>,
) -> Result<String> {
    if let Some(value) = explicit {
        return Ok(Uuid::parse_str(value)
            .map_err(|_| TesError::InvalidDocId {
                value: value.to_owned(),
            })?
            .to_string());
    }
    if output.is_file()
        && let Ok(file) = TesFile::open(output)
        && let Some(catalog) = file.catalog()
    {
        return Ok(catalog.doc_id.clone());
    }
    if let Some(seed) = seed {
        return Ok(doc_id_from_seed(seed).to_string());
    }
    Ok(Uuid::new_v4().to_string())
}

/// Append text blocks and materialize pending outbound links into `TLNK`.
///
/// # Errors
///
/// Returns session / link validation errors.
pub fn seal_text_blocks(session: &mut TesWriterSession, blocks: &[MarkdownBlock]) -> Result<()> {
    for block in blocks {
        session.add_text_with_outbound_links(
            block.header.clone(),
            &block.body,
            &block.pending_links,
        )?;
    }
    Ok(())
}

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

fn first_heading(blocks: &[MarkdownBlock]) -> Option<String> {
    blocks
        .iter()
        .find(|b| b.header.role == TextRole::Heading)
        .map(|b| b.body.clone())
}

/// Split Obsidian-style YAML front matter from the Markdown body.
#[must_use]
pub fn parse_front_matter(source: &str) -> (MarkdownFrontMatter, &str) {
    let Some(after_open) = source
        .strip_prefix("---\n")
        .or_else(|| source.strip_prefix("---\r\n"))
    else {
        return (MarkdownFrontMatter::default(), source);
    };
    let Some(end) = after_open
        .find("\n---\n")
        .or_else(|| after_open.find("\n---\r\n"))
    else {
        return (MarkdownFrontMatter::default(), source);
    };
    let front = &after_open[..end];
    let body_start = if after_open[end..].starts_with("\n---\r\n") {
        end + "\n---\r\n".len()
    } else {
        end + "\n---\n".len()
    };
    (parse_front_matter_body(front), &after_open[body_start..])
}

fn parse_front_matter_body(front: &str) -> MarkdownFrontMatter {
    let mut out = MarkdownFrontMatter::default();
    let mut lines = front.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("title:") {
            out.title = Some(unquote(rest.trim()));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("id:") {
            let value = unquote(rest.trim());
            if !value.is_empty() {
                out.id = Some(value);
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("tags:") {
            out.tags = parse_yaml_string_list(rest.trim(), &mut lines);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("aliases:") {
            out.aliases = parse_yaml_string_list(rest.trim(), &mut lines);
        }
    }
    out
}

fn parse_yaml_string_list<'a>(
    inline: &str,
    lines: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
) -> Vec<String> {
    if inline == "[]" {
        return Vec::new();
    }
    if inline.starts_with('[') && inline.ends_with(']') {
        return inline[1..inline.len() - 1]
            .split(',')
            .map(|s| unquote(s.trim()))
            .filter(|s| !s.is_empty())
            .collect();
    }
    if !inline.is_empty() {
        return vec![unquote(inline)];
    }
    let mut items = Vec::new();
    while let Some(next) = lines.peek() {
        let t = next.trim();
        if let Some(item) = t.strip_prefix("- ") {
            items.push(unquote(item.trim()));
            lines.next();
        } else if t.is_empty() {
            lines.next();
        } else {
            break;
        }
    }
    items
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        value[1..value.len() - 1].to_owned()
    } else {
        value.to_owned()
    }
}

/// One `[[target]]` / `[[target|label]]` span in Markdown source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WikilinkSpan<'a> {
    /// Byte offset of the opening `[[`.
    pub start: usize,
    /// Byte offset immediately after the closing `]]`.
    pub end: usize,
    /// Link target (left of `|`, trimmed).
    pub target: &'a str,
    /// Display label (right of `|`, or the full inner text when unlabeled).
    pub label: &'a str,
}

/// Invoke `visitor` for each Obsidian-style wikilink in `markdown`.
pub fn visit_wikilinks(markdown: &str, mut visitor: impl FnMut(WikilinkSpan<'_>)) {
    let bytes = markdown.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'['
            && bytes[i + 1] == b'['
            && let Some(close) = find_wikilink_end(markdown, i + 2)
        {
            let inner = &markdown[i + 2..close];
            let (target, label) = if let Some((t, l)) = inner.split_once('|') {
                (t.trim(), l.trim())
            } else {
                let t = inner.trim();
                (t, t)
            };
            visitor(WikilinkSpan {
                start: i,
                end: close + 2,
                target,
                label,
            });
            i = close + 2;
            continue;
        }
        i += 1;
    }
}

/// Collect wikilink targets for which `is_resolved` returns false (unique via `out`).
pub fn collect_unresolved_wikilinks(
    markdown: &str,
    is_resolved: impl Fn(&str) -> bool,
    out: &mut std::collections::HashSet<String>,
) {
    visit_wikilinks(markdown, |span| {
        if !span.target.is_empty() && !is_resolved(span.target) {
            out.insert(span.target.to_owned());
        }
    });
}

/// Rewrite `[[target]]` / `[[target|label]]` to `[label](uuid)` when `resolve` returns an id.
///
/// Unresolved wikilinks are left unchanged in the output.
#[must_use]
pub fn rewrite_wikilinks(markdown: &str, resolve: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut cursor = 0;
    visit_wikilinks(markdown, |span| {
        if let Some(uuid) = resolve(span.target) {
            out.push_str(&markdown[cursor..span.start]);
            out.push('[');
            out.push_str(span.label);
            out.push_str("](");
            out.push_str(&uuid);
            out.push(')');
            cursor = span.end;
        }
    });
    out.push_str(&markdown[cursor..]);
    out
}

fn find_wikilink_end(markdown: &str, start: usize) -> Option<usize> {
    let bytes = markdown.as_bytes();
    let mut j = start;
    while j + 1 < bytes.len() {
        if bytes[j] == b']' && bytes[j + 1] == b']' {
            return Some(j);
        }
        j += 1;
    }
    None
}

fn extend_unique(dst: &mut Vec<String>, extras: &[String]) {
    for item in extras {
        if !dst.iter().any(|existing| existing == item) {
            dst.push(item.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::export::{ExportOptions, ExportView, export_view};
    use tempfile::tempdir;

    #[test]
    fn parses_nested_list_depth_from_markdown() {
        let md = concat!(
            "- top\n",
            "  - nested\n",
            "    - deeper\n",
            "1. ordered top\n",
            "   1. ordered nested\n",
        );
        let blocks = parse_markdown_blocks(md);
        let items: Vec<_> = blocks
            .iter()
            .filter(|b| b.header.role == TextRole::ListItem)
            .collect();
        assert!(items.len() >= 4, "got {} items: {items:?}", items.len());
        assert_eq!(items[0].header.list_depth_or_default(), 1);
        assert_eq!(items[1].header.list_depth_or_default(), 2);
        assert_eq!(items[2].header.list_depth_or_default(), 3);
    }

    #[test]
    fn parses_underline_html_into_inline_span() {
        let blocks = parse_markdown_blocks("Note the <u>underlined</u> word.\n");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].body, "Note the underlined word.");
        assert_eq!(blocks[0].header.spans.len(), 1);
        assert_eq!(blocks[0].header.spans[0].kind, InlineKind::Underline);
        assert_eq!(
            &blocks[0].body
                [blocks[0].header.spans[0].start as usize..blocks[0].header.spans[0].end as usize],
            "underlined"
        );
    }

    #[test]
    fn parses_commonmark_subset_into_semantic_blocks() {
        let md = concat!(
            "# Methods\n\n",
            "A **bold** paragraph with [a link](https://example.com).\n\n",
            "> Quoted *text*.\n\n",
            "1. First\n",
            "2. Second\n\n",
            "```rust\nlet x = 1;\n```\n",
        );
        let blocks = parse_markdown_blocks(md);
        assert_eq!(blocks.len(), 6);
        assert_eq!(blocks[0].header, TextHeader::heading(1));
        assert_eq!(blocks[0].body, "Methods");
        assert_eq!(blocks[1].body, "A bold paragraph with a link.");
        assert_eq!(blocks[1].pending_links.len(), 1);
        assert_eq!(blocks[1].pending_links[0].dest, "https://example.com");
        assert_eq!(
            &blocks[1].body[blocks[1].pending_links[0].start as usize
                ..blocks[1].pending_links[0].end as usize],
            "a link"
        );
        assert_eq!(blocks[2].header.role, TextRole::Blockquote);
        assert_eq!(blocks[3].header.list_kind, Some(ListKind::Ordered));
        assert_eq!(blocks[5].header.role, TextRole::CodeBlock);
        assert_eq!(blocks[5].body, "let x = 1;");
    }

    #[test]
    fn parses_gfm_pipe_table_into_table_data() {
        let md = concat!(
            "| Name | Score |\n",
            "| :--- | ----: |\n",
            "| Ada | 10 |\n",
            "| Bob | 7 |\n",
        );
        let blocks = parse_markdown_blocks(md);
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        assert_eq!(blocks[0].header.role, TextRole::Table);
        assert!(blocks[0].body.is_empty());
        let table = blocks[0].header.table.as_ref().expect("TableData");
        assert_eq!(table.rows.len(), 3);
        assert!(table.rows[0].cells[0].is_header);
        assert_eq!(table.rows[0].cells[0].text, "Name");
        assert_eq!(table.rows[0].cells[0].align, Some(TextAlign::Start));
        assert_eq!(table.rows[0].cells[1].text, "Score");
        assert_eq!(table.rows[0].cells[1].align, Some(TextAlign::End));
        assert!(!table.rows[1].cells[0].is_header);
        assert_eq!(table.rows[1].cells[0].text, "Ada");
        assert_eq!(table.rows[2].cells[1].text, "7");
    }

    #[test]
    fn imports_obsidian_pipe_table_round_trips_as_table_role() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("table.md");
        let output = dir.path().join("table.tes");
        std::fs::write(&input, "# Notes\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n").unwrap();
        let report =
            import_markdown_v0(&input, &output, &MarkdownImportOptions::default()).unwrap();
        assert_eq!(report.chunk_count, 2);

        let file = TesFile::open(&output).unwrap();
        let mut saw_table = false;
        for entry in file.chunks() {
            if entry.chunk_type != crate::catalog::ChunkType::Text {
                continue;
            }
            let raw = file.decode_payload(entry).unwrap();
            let (header, body) = crate::catalog::decode_text_payload(&raw).unwrap();
            if header.role == TextRole::Table {
                saw_table = true;
                assert!(body.is_empty());
                let table = header.table.as_ref().expect("TableData on header");
                assert_eq!(table.rows.len(), 2);
                assert_eq!(table.rows[0].cells[0].text, "A");
                assert_eq!(table.rows[1].cells[1].text, "2");
            }
        }
        assert!(saw_table, "expected a table text chunk");
    }

    #[test]
    fn imports_minimal_fixture_and_round_trips_views() {
        let dir = tempdir().unwrap();
        let input =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/assets/markdown/minimal.md");
        let output = dir.path().join("minimal.tes");
        let report =
            import_markdown_v0(&input, &output, &MarkdownImportOptions::default()).unwrap();
        assert_eq!(report.title, "Minimal note");
        assert_eq!(report.chunk_count, 2);

        let linear = export_view(&output, ExportView::Linear, &ExportOptions::default()).unwrap();
        assert_eq!(
            linear,
            "# Minimal note\n\nLorem ipsum dolor sit amet, consectetur adipiscing elit.\n"
        );
        let ai = export_view(&output, ExportView::AiText, &ExportOptions::default()).unwrap();
        assert_eq!(
            ai,
            "Minimal note\n\nLorem ipsum dolor sit amet, consectetur adipiscing elit.\n"
        );
    }

    #[test]
    fn external_https_link_round_trips_through_tes() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("linked.md");
        let output = dir.path().join("linked.tes");
        std::fs::write(
            &input,
            "# Linked\n\nSee [the docs](https://example.com/path) for more.\n",
        )
        .unwrap();
        import_markdown_v0(&input, &output, &MarkdownImportOptions::default()).unwrap();

        let file = crate::catalog::TesFile::open(&output).unwrap();
        assert_eq!(file.links().len(), 1);
        assert_eq!(
            file.links()[0].external_uri(),
            Some("https://example.com/path")
        );

        let md = export_view(&output, ExportView::Markdown, &ExportOptions::default()).unwrap();
        assert!(md.contains("[the docs](https://example.com/path)"));

        let html = export_view(&output, ExportView::Html, &ExportOptions::default()).unwrap();
        assert!(html.contains("href=\"https://example.com/path\""));

        let tessprek = crate::edit::encode_tessprek(&file, "deadbeef").unwrap();
        assert!(tessprek.contains("[the docs](https://example.com/path)"));
    }

    #[test]
    fn front_matter_title_wins_over_heading() {
        let (front, body) = parse_front_matter("---\ntitle: \"Front\"\n---\n# Heading\n");
        assert_eq!(front.title.as_deref(), Some("Front"));
        assert_eq!(body, "# Heading\n");
    }

    #[test]
    fn parses_obsidian_front_matter_lists() {
        let (front, _) = parse_front_matter(
            "---\nid: Erasure\ntags:\n  - Books\n  - Fiction\naliases:\n  - American Fiction\n---\n# Erasure\n",
        );
        assert_eq!(front.id.as_deref(), Some("Erasure"));
        assert_eq!(front.tags, vec!["Books", "Fiction"]);
        assert_eq!(front.aliases, vec!["American Fiction"]);
    }

    #[test]
    fn rewrite_wikilinks_resolves_known_targets() {
        let out = rewrite_wikilinks("See [[Erasure|the novel]] and [[Missing]].", &|name| {
            if name == "Erasure" {
                Some("550e8400-e29b-41d4-a716-446655440000".into())
            } else {
                None
            }
        });
        assert!(out.contains("[the novel](550e8400-e29b-41d4-a716-446655440000)"));
        assert!(out.contains("[[Missing]]"));
    }

    #[test]
    fn import_keeps_existing_doc_id_on_reimport() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("note.md");
        let output = dir.path().join("note.tes");
        std::fs::write(&input, "---\nid: Stable\n---\n# Hello\n\nBody.\n").unwrap();
        let first = import_markdown_v0(
            &input,
            &output,
            &MarkdownImportOptions {
                doc_id_seed: Some("Stable".into()),
                ..MarkdownImportOptions::default()
            },
        )
        .unwrap();
        std::fs::write(&input, "---\nid: Stable\n---\n# Hello\n\nChanged.\n").unwrap();
        let second = import_markdown_v0(
            &input,
            &output,
            &MarkdownImportOptions {
                doc_id_seed: Some("other-seed".into()),
                ..MarkdownImportOptions::default()
            },
        )
        .unwrap();
        assert_eq!(first.doc_id, second.doc_id);
        assert_eq!(first.slug.as_deref(), Some("Stable"));
    }

    #[test]
    fn rejects_invalid_explicit_doc_id() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("note.md");
        let output = dir.path().join("note.tes");
        std::fs::write(&input, "# Note\n").unwrap();
        let options = MarkdownImportOptions {
            doc_id: Some("not-a-uuid".to_owned()),
            ..Default::default()
        };
        let err = import_markdown_v0(input, output, &options).unwrap_err();
        assert!(matches!(err, TesError::InvalidDocId { .. }));
    }
}
