//! Semantic HTML → Tessera text chunks.

use std::path::{Path, PathBuf};

use scraper::{ElementRef, Html, Selector};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use super::MarkdownBlock;
use crate::catalog::{DocumentCatalog, ListKind, TesWriterSession, TextHeader, TextRole};
use crate::error::{Result, TesError};
use crate::layout::DocKind;

/// Options for HTML → `.tes` import.
#[derive(Debug, Clone)]
pub struct HtmlImportOptions {
    /// Kind stored in the superblock and catalog.
    pub doc_kind: DocKind,
    /// Catalog title override.
    pub title: Option<String>,
    /// Stable document UUID string. Generated when absent.
    pub doc_id: Option<String>,
}

impl Default for HtmlImportOptions {
    fn default() -> Self {
        Self {
            doc_kind: DocKind::Document,
            title: None,
            doc_id: None,
        }
    }
}

/// Result summary for a completed HTML import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlImportReport {
    /// Source HTML path.
    pub input: PathBuf,
    /// Sealed `.tes` path.
    pub output: PathBuf,
    /// Stable catalog UUID.
    pub doc_id: String,
    /// Catalog title.
    pub title: String,
    /// Number of semantic text chunks.
    pub chunk_count: usize,
}

/// Import semantic HTML and seal a `.tes` document.
///
/// # Errors
///
/// Returns [`TesError::Io`] if the source cannot be read or the `.tes` cannot be written,
/// [`TesError::InvalidDocId`] if `options.doc_id` is not a UUID, or catalog/session
/// errors from [`TesWriterSession`].
pub fn import_html_v0(
    input: impl AsRef<Path>,
    output: impl AsRef<Path>,
    options: &HtmlImportOptions,
) -> Result<HtmlImportReport> {
    let input = input.as_ref();
    let output = output.as_ref();
    let source = std::fs::read_to_string(input)?;
    let document = Html::parse_document(&source);
    let blocks = parse_document_blocks(&document);
    let title = options
        .title
        .clone()
        .or_else(|| document_title(&document))
        .or_else(|| {
            blocks
                .iter()
                .find(|b| b.header.role == TextRole::Heading)
                .map(|b| b.body.clone())
        })
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

    Ok(HtmlImportReport {
        input: input.to_path_buf(),
        output: output.to_path_buf(),
        doc_id,
        title,
        chunk_count: blocks.len(),
    })
}

/// Parse semantic HTML blocks in document order.
#[must_use]
pub fn parse_html_blocks(source: &str) -> Vec<MarkdownBlock> {
    parse_document_blocks(&Html::parse_document(source))
}

fn parse_document_blocks(document: &Html) -> Vec<MarkdownBlock> {
    let selector =
        Selector::parse("h1,h2,h3,h4,h5,h6,p,li,blockquote,pre,table").expect("static selector");
    let row_selector = Selector::parse("tr").expect("static selector");
    let cell_selector = Selector::parse("th,td").expect("static selector");
    let mut blocks = Vec::new();

    for element in document.select(&selector) {
        let name = element.value().name();
        // Parent blocks own their nested paragraphs/list items.
        if name == "p" && has_ancestor(&element, &["blockquote", "li"]) {
            continue;
        }
        if name == "li" && has_ancestor(&element, &["li"]) {
            // Nested items still appear separately through their own selection;
            // this guard only prevents pathological nested wrapping.
        }

        let mut header = match name {
            "h1" => TextHeader::heading(1),
            "h2" => TextHeader::heading(2),
            "h3" => TextHeader::heading(3),
            "h4" => TextHeader::heading(4),
            "h5" => TextHeader::heading(5),
            "h6" => TextHeader::heading(6),
            "p" => TextHeader::paragraph(),
            "li" => {
                let ordered = has_ancestor(&element, &["ol"]);
                TextHeader::list_item(if ordered {
                    ListKind::Ordered
                } else {
                    ListKind::Bullet
                })
            }
            "blockquote" => header_for_role(TextRole::Blockquote),
            "pre" => header_for_role(TextRole::CodeBlock),
            "table" => header_for_role(TextRole::Table),
            _ => continue,
        };
        header.classes = element
            .value()
            .attr("class")
            .map(|value| value.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_default();

        let body = if name == "table" {
            element
                .select(&row_selector)
                .map(|row| {
                    row.select(&cell_selector)
                        .map(|cell| clean_text(&cell))
                        .collect::<Vec<_>>()
                        .join("\t")
                })
                .filter(|row| !row.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        } else if name == "pre" {
            element.text().collect::<String>().trim().to_owned()
        } else {
            clean_text(&element)
        };
        if !body.is_empty() {
            blocks.push(MarkdownBlock { header, body });
        }
    }
    blocks
}

fn clean_text(element: &ElementRef<'_>) -> String {
    element
        .text()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn has_ancestor(element: &ElementRef<'_>, names: &[&str]) -> bool {
    element
        .ancestors()
        .filter_map(ElementRef::wrap)
        .any(|ancestor| names.contains(&ancestor.value().name()))
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

fn document_title(document: &Html) -> Option<String> {
    let selector = Selector::parse("title").expect("static selector");
    document
        .select(&selector)
        .next()
        .map(|element| clean_text(&element))
        .filter(|title| !title.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::export::{ExportOptions, ExportView, export_view};
    use tempfile::tempdir;

    #[test]
    fn parses_semantic_blocks_and_classes() {
        let source = r#"
            <h1>Title</h1>
            <p class="note lead">Hello <strong>world</strong>.</p>
            <ol><li>First</li><li>Second</li></ol>
            <blockquote><p>Quote me.</p></blockquote>
            <pre><code>let x = 1;</code></pre>
            <table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></table>
        "#;
        let blocks = parse_html_blocks(source);
        assert_eq!(blocks.len(), 7);
        assert_eq!(blocks[0].header, TextHeader::heading(1));
        assert_eq!(blocks[1].body, "Hello world.");
        assert_eq!(blocks[1].header.classes, ["note", "lead"]);
        assert_eq!(blocks[2].header.list_kind, Some(ListKind::Ordered));
        assert_eq!(blocks[4].header.role, TextRole::Blockquote);
        assert_eq!(blocks[5].header.role, TextRole::CodeBlock);
        assert_eq!(blocks[6].body, "A\tB\n1\t2");
    }

    #[test]
    fn imports_stress_fixture_and_exports_html() {
        let dir = tempdir().unwrap();
        let input = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/assets/html/rich_document.html");
        let output = dir.path().join("rich.tes");
        let report = import_html_v0(&input, &output, &HtmlImportOptions::default()).unwrap();
        assert_eq!(report.title, "Specimen Document — HTML Import Stress Test");
        assert!(report.chunk_count > 20);
        let exported = export_view(&output, ExportView::Html, &ExportOptions::default()).unwrap();
        assert!(exported.contains("<article"));
        assert!(exported.contains("<h1"));
        assert!(!exported.contains("<script"));
        assert!(!exported.contains("style="));
    }
}
