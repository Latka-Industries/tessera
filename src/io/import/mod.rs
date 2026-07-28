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

use crate::catalog::{DocumentCatalog, ListKind, TesWriterSession, TextHeader, TextRole};
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
    for block in &blocks {
        session.add_text_chunk(&block.header, &block.body)?;
    }
    session.commit()?;

    Ok(MarkdownImportReport {
        input: input.to_path_buf(),
        output: output.to_path_buf(),
        doc_id,
        title,
        chunk_count: blocks.len(),
    })
}

/// Parse the supported `CommonMark` subset into semantic text blocks.
#[must_use]
pub fn parse_markdown_blocks(markdown: &str) -> Vec<MarkdownBlock> {
    let parser = Parser::new_ext(markdown, Options::empty());
    let mut state = ParseState::default();

    for event in parser {
        match event {
            Event::Start(tag) => state.start(&tag),
            Event::End(tag) => state.end(tag),
            Event::Text(text)
            | Event::Code(text)
            | Event::InlineMath(text)
            | Event::DisplayMath(text) => state.push_text(&text),
            Event::SoftBreak => state.push_break(false),
            Event::HardBreak => state.push_break(true),
            Event::TaskListMarker(done) => {
                state.push_text(if done { "[x] " } else { "[ ] " });
            }
            // Raw HTML, footnotes, and rules are explicitly deferred in v0.
            Event::Html(_) | Event::InlineHtml(_) | Event::FootnoteReference(_) | Event::Rule => {}
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
            Tag::CodeBlock(_) => self.begin(header_for_role(TextRole::CodeBlock)),
            // Inline tags are intentionally flattened; unsupported block tags
            // contribute text to their enclosing supported block when present.
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
            _ => {}
        }
    }

    fn begin(&mut self, header: TextHeader) {
        self.finish_active();
        self.active = Some(ActiveBlock {
            header,
            body: String::new(),
        });
    }

    fn push_text(&mut self, text: &str) {
        if self.active.is_none() && !text.trim().is_empty() {
            self.begin(TextHeader::paragraph());
        }
        if let Some(active) = &mut self.active {
            active.body.push_str(text);
        }
    }

    fn push_break(&mut self, hard: bool) {
        if let Some(active) = &mut self.active {
            active.body.push(if hard { '\n' } else { ' ' });
        }
    }

    fn finish_active(&mut self) {
        if let Some(active) = self.active.take() {
            let body = active.body.trim().to_owned();
            if !body.is_empty() {
                self.blocks.push(MarkdownBlock {
                    header: active.header,
                    body,
                });
            }
        }
    }
}

fn header_for_role(role: TextRole) -> TextHeader {
    TextHeader {
        role,
        level: None,
        list_kind: None,
        emphasis: Vec::new(),
        classes: Vec::new(),
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
