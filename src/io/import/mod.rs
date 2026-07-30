//! Foreign-format importers under [`crate::io`].
//!
//! v0 implements the `CommonMark` subset from `docs/decisions.md`: ATX headings,
//! paragraphs, lists, fenced code, and blockquotes. Inline presentation is
//! parsed once and flattened into clean canonical text.
//!
//! HTML import lives in [`html`].

pub mod html;

use std::path::{Path, PathBuf};

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use crate::catalog::{
    DocumentCatalog, InlineKind, InlineSpan, ListKind, OutboundLink, TesWriterSession, TextHeader,
    TextRole,
};
use crate::error::{Result, TesError};
use crate::layout::DocKind;

pub use html::{HtmlImportOptions, HtmlImportReport, import_html_v0, parse_html_blocks};

/// Options for Markdown → `.tes` import.
#[derive(Debug, Clone)]
pub struct MarkdownImportOptions {
    /// Kind stored in the superblock and catalog.
    pub doc_kind: DocKind,
    /// Catalog title. When absent, front matter, first heading, or filename wins.
    pub title: Option<String>,
    /// Stable document UUID string. Generated when absent.
    pub doc_id: Option<String>,
}

impl Default for MarkdownImportOptions {
    fn default() -> Self {
        Self {
            doc_kind: DocKind::Document,
            title: None,
            doc_id: None,
        }
    }
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
    let (front_title, markdown) = strip_front_matter(&source);
    let blocks = parse_markdown_blocks(markdown);

    let title = options
        .title
        .clone()
        .or(front_title)
        .or_else(|| first_heading(&blocks))
        .or_else(|| {
            input
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "Untitled".to_owned());
    let doc_id = match &options.doc_id {
        Some(value) => Uuid::parse_str(value)
            .map_err(|_| TesError::InvalidDocId {
                value: value.clone(),
            })?
            .to_string(),
        None => Uuid::new_v4().to_string(),
    };
    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|err| std::io::Error::other(format!("format import timestamp: {err}")))?;

    let mut session = TesWriterSession::create(output, options.doc_kind);
    session.set_catalog(DocumentCatalog::new(
        &doc_id,
        &title,
        &now,
        &now,
        options.doc_kind,
    ))?;
    seal_text_blocks(&mut session, &blocks)?;
    session.commit()?;

    Ok(MarkdownImportReport {
        input: input.to_path_buf(),
        output: output.to_path_buf(),
        doc_id,
        title,
        chunk_count: blocks.len(),
    })
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

/// Parse the supported `CommonMark` subset into semantic text blocks.
#[must_use]
pub fn parse_markdown_blocks(markdown: &str) -> Vec<MarkdownBlock> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_MATH);
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
                self.begin(TextHeader::list_item(kind));
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
                if self.active.is_none() {
                    self.begin(TextHeader::paragraph());
                }
                if let Some(active) = &mut self.active {
                    let start = u32::try_from(active.body.len()).unwrap_or(u32::MAX);
                    active.link_stack.push((start, dest_url.to_string()));
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
        if self.active.is_none() && !text.trim().is_empty() {
            self.begin(TextHeader::paragraph());
        }
        if let Some(active) = &mut self.active {
            active.body.push_str(text);
        }
    }

    fn push_inline_math(&mut self, tex: &str) {
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
        self.finish_active();
        self.begin(TextHeader::math());
        self.push_text(tex);
        self.finish_active();
    }

    fn push_break(&mut self, hard: bool) {
        if let Some(active) = &mut self.active {
            active.body.push(if hard { '\n' } else { ' ' });
        }
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

fn strip_front_matter(source: &str) -> (Option<String>, &str) {
    let Some(after_open) = source.strip_prefix("---\n") else {
        return (None, source);
    };
    let Some(end) = after_open.find("\n---\n") else {
        return (None, source);
    };
    let front = &after_open[..end];
    let title = front.lines().find_map(|line| {
        line.strip_prefix("title:")
            .map(str::trim)
            .map(|value| value.trim_matches(['"', '\'']).to_owned())
    });
    (title, &after_open[end + 5..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::export::{ExportOptions, ExportView, export_view};
    use tempfile::tempdir;

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
        let (title, body) = strip_front_matter("---\ntitle: \"Front\"\n---\n# Heading\n");
        assert_eq!(title.as_deref(), Some("Front"));
        assert_eq!(body, "# Heading\n");
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
