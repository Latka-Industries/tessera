//! Decoded export views (`docs/exports.md`).
//!
//! Exports are **projections** of a sealed `.tes` file — never the canonical
//! source. Models and pipelines should call these views rather than hex-dumping
//! the wire format.

use std::fmt::Write as _;
use std::path::Path;

use serde::Serialize;

use crate::bib::{
    BibEntry, format_numeric_marker, format_numeric_reference, format_pandoc_cite,
    format_reference_body,
};
use crate::catalog::chunk::{CitePayload, ListKind, TextHeader, TextRole, decode_text_payload};
use crate::catalog::file::TesFile;
use crate::catalog::index::{ChunkIndexEntry, ChunkType};
use crate::catalog::media::{FigureRef, ImagePayload, base64_encode};
use crate::error::{Result, TesError};

/// Which decoded view to emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportView {
    /// Concatenate text chunk bodies (`--raw`).
    Raw,
    /// Reading-order prose with light structure markers (`--linear`).
    Linear,
    /// LLM-oriented plain text, no exporter-introduced markup (`--ai-text`).
    AiText,
    /// One JSON object per reading-order chunk (`--chunks-jsonl`).
    ChunksJsonl,
    /// Lossy GFM-ish Markdown projection (`--markdown`).
    Markdown,
    /// Semantic HTML5 fragment or standalone document (`--html`).
    Html,
}

/// Options that refine an [`ExportView`].
#[derive(Debug, Clone, Default)]
pub struct ExportOptions {
    /// Restrict output to a single chunk id (where applicable).
    pub chunk_id: Option<u64>,
    /// Prefix each `--raw` chunk with a debug header line.
    pub include_headers: bool,
    /// Prefix each `--ai-text` chunk with `<!-- chunk:N -->`.
    pub annotate: bool,
    /// Include non-reading-order / non-text rows in `--chunks-jsonl`.
    pub all_types: bool,
    /// Omit cite chunk expansion from `--ai-text`.
    pub no_cites: bool,
    /// Stylesheet href for HTML export.
    pub theme_href: Option<String>,
    /// Wrap HTML output in a complete document.
    pub standalone: bool,
    /// CSS embedded in a `<style>` element.
    pub embedded_css: Option<String>,
    /// When set, figure `<img src>` uses `{prefix}{image_chunk_id}` instead of data URIs.
    pub media_url_prefix: Option<String>,
}

/// Export `path` as the selected view.
///
/// # Errors
///
/// Returns open/parse errors from [`TesFile::open`], or view-specific errors from
/// [`export_file`].
pub fn export_view(
    path: impl AsRef<Path>,
    view: ExportView,
    options: &ExportOptions,
) -> Result<String> {
    let file = TesFile::open(path.as_ref())?;
    export_file(&file, view, options)
}

/// Export an already-open file.
///
/// # Errors
///
/// Returns [`TesError::ChunkNotFound`] if a requested chunk is missing,
/// [`TesError::Decode`] / [`TesError::InvalidFigure`] when a payload cannot be decoded,
/// or other payload errors from [`TesFile::decode_payload`].
pub fn export_file(file: &TesFile, view: ExportView, options: &ExportOptions) -> Result<String> {
    match view {
        ExportView::Raw => export_raw(file, options),
        ExportView::Linear => export_linear(file, options),
        ExportView::AiText => export_ai_text(file, options),
        ExportView::ChunksJsonl => export_chunks_jsonl(file, options),
        ExportView::Markdown => export_markdown(file, options),
        ExportView::Html => export_html(file, options),
    }
}

fn export_raw(file: &TesFile, options: &ExportOptions) -> Result<String> {
    let entries = selected_text_entries(file, options)?;
    let mut out = String::new();
    for (i, entry) in entries.iter().enumerate() {
        let (header, body) = decode_text_entry(file, entry)?;
        if options.include_headers {
            let _ = writeln!(
                out,
                "[chunk_id={} role={}{}]",
                entry.chunk_id,
                header.role.as_str(),
                header
                    .level
                    .map(|l| format!(" level={l}"))
                    .unwrap_or_default()
            );
        }
        out.push_str(&body);
        if i + 1 < entries.len() {
            out.push_str("\n\n");
        }
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

fn export_linear(file: &TesFile, options: &ExportOptions) -> Result<String> {
    let entries = selected_content_entries(file, options)?;
    let cite_numbers = cite_number_map(file, &entries)?;
    let mut out = String::new();
    let mut bib_items: Vec<(usize, BibEntry)> = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        match entry.chunk_type {
            ChunkType::Text => {
                let (header, body) = decode_text_entry(file, entry)?;
                match header.role {
                    TextRole::Heading => {
                        let level = header.level.unwrap_or(1).clamp(1, 6) as usize;
                        out.push_str(&"#".repeat(level));
                        out.push(' ');
                        out.push_str(body.trim_end());
                        out.push('\n');
                    }
                    TextRole::ListItem => {
                        let marker = match header.list_kind.unwrap_or(ListKind::Bullet) {
                            ListKind::Bullet => "- ".to_owned(),
                            ListKind::Ordered => "1. ".to_owned(),
                        };
                        out.push_str(&marker);
                        out.push_str(body.trim_end());
                        out.push('\n');
                    }
                    TextRole::Blockquote => {
                        for line in body.lines() {
                            out.push_str("> ");
                            out.push_str(line);
                            out.push('\n');
                        }
                    }
                    TextRole::CodeBlock => {
                        out.push_str("```\n");
                        out.push_str(body.trim_end());
                        out.push_str("\n```\n");
                    }
                    TextRole::Paragraph | TextRole::Table => {
                        out.push_str(body.trim_end());
                        out.push('\n');
                    }
                }
            }
            ChunkType::Figure => {
                let figure = decode_figure_entry(file, entry)?;
                let _ = writeln!(
                    out,
                    "[figure image={} placement={}]\n{}",
                    figure.image_chunk_id,
                    figure.placement.as_str(),
                    figure.alt_text.trim_end()
                );
                if let Some(caption) = figure.caption.as_deref() {
                    let _ = writeln!(out, "{caption}");
                }
            }
            ChunkType::Cite if !options.no_cites => {
                let (n, cite, bib) = decode_numbered_cite(file, entry, &cite_numbers)?;
                let marker = format_numeric_marker(n);
                if cite.quote.trim().is_empty() {
                    let _ = writeln!(out, "{marker}");
                } else {
                    let _ = writeln!(out, "{marker} {}", cite.quote.trim());
                }
                bib_items.push((n, bib));
            }
            _ => {}
        }
        if i + 1 < entries.len() {
            out.push('\n');
        }
    }
    if !bib_items.is_empty() {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("\nReferences\n");
        bib_items.sort_by_key(|(n, _)| *n);
        for (n, entry) in bib_items {
            let _ = writeln!(out, "{}", format_numeric_reference(n, &entry));
        }
    }
    Ok(out)
}

fn export_markdown(file: &TesFile, options: &ExportOptions) -> Result<String> {
    let entries = selected_content_entries(file, options)?;
    let cite_numbers = cite_number_map(file, &entries)?;
    let mut parts = Vec::with_capacity(entries.len());
    let mut bib_items: Vec<(usize, BibEntry)> = Vec::new();
    for entry in entries {
        match entry.chunk_type {
            ChunkType::Text => {
                let (header, body) = decode_text_entry(file, entry)?;
                let body = body.trim_end();
                let rendered = match header.role {
                    TextRole::Heading => {
                        let level = header.level.unwrap_or(1).clamp(1, 6) as usize;
                        format!("{} {body}", "#".repeat(level))
                    }
                    TextRole::ListItem => match header.list_kind.unwrap_or(ListKind::Bullet) {
                        ListKind::Bullet => format!("- {body}"),
                        ListKind::Ordered => format!("1. {body}"),
                    },
                    TextRole::Blockquote => body
                        .lines()
                        .map(|line| format!("> {line}"))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    TextRole::CodeBlock => format!("```\n{body}\n```"),
                    TextRole::Table => format!("```tsv\n{body}\n```"),
                    TextRole::Paragraph => body.to_owned(),
                };
                parts.push(rendered);
            }
            ChunkType::Figure => {
                let figure = decode_figure_entry(file, entry)?;
                let mut block = format!(
                    "![{}](media:chunk-{})",
                    markdown_escape_alt(&figure.alt_text),
                    figure.image_chunk_id
                );
                if let Some(caption) = figure.caption.as_deref() {
                    block.push_str("\n\n*");
                    block.push_str(caption.trim());
                    block.push('*');
                }
                parts.push(block);
            }
            ChunkType::Cite if !options.no_cites => {
                let (n, cite, bib) = decode_numbered_cite(file, entry, &cite_numbers)?;
                let label = cite
                    .label
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(bib.cite_key.as_str());
                let label = if label.is_empty() { "unknown" } else { label };
                let mut block = format_pandoc_cite(label);
                if !cite.quote.trim().is_empty() {
                    block.push(' ');
                    block.push('"');
                    block.push_str(cite.quote.trim());
                    block.push('"');
                }
                parts.push(block);
                bib_items.push((n, bib));
            }
            _ => {}
        }
    }
    if !bib_items.is_empty() {
        bib_items.sort_by_key(|(n, _)| *n);
        let mut refs = String::from("## References\n");
        for (n, entry) in &bib_items {
            let _ = writeln!(refs, "{}", format_numeric_reference(*n, entry));
        }
        parts.push(refs.trim_end().to_owned());
    }
    let mut out = parts.join("\n\n");
    if !out.is_empty() {
        out.push('\n');
    }
    Ok(out)
}

fn export_html(file: &TesFile, options: &ExportOptions) -> Result<String> {
    let entries = selected_content_entries(file, options)?;
    let cite_numbers = cite_number_map(file, &entries)?;
    let doc_id = file.catalog().map_or("", |catalog| catalog.doc_id.as_str());
    let title = file
        .catalog()
        .map_or("Untitled", |catalog| catalog.title.as_str());
    let mut article = format!("<article data-doc-id=\"{}\">\n", escape_html(doc_id));
    let mut bib_items: Vec<(usize, BibEntry)> = Vec::new();

    for entry in entries {
        match entry.chunk_type {
            ChunkType::Text => {
                let (header, body) = decode_text_entry(file, entry)?;
                article.push_str(&render_text_chunk_html(entry.chunk_id, &header, &body));
            }
            ChunkType::Figure => {
                article.push_str(&render_figure_html(file, entry, options)?);
            }
            ChunkType::Cite if !options.no_cites => {
                append_cite_html(file, entry, &cite_numbers, &mut article, &mut bib_items)?;
            }
            _ => {}
        }
    }
    append_html_bibliography(&mut article, &mut bib_items);
    article.push_str("</article>\n");

    let styles = html_theme_styles(options);
    if options.standalone {
        Ok(format!(
            "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n{styles}</head>\n<body>\n{article}</body>\n</html>\n",
            escape_html(title)
        ))
    } else {
        Ok(format!("{styles}{article}"))
    }
}

fn render_text_chunk_html(chunk_id: u64, header: &TextHeader, body: &str) -> String {
    let escaped = escape_html(body);
    let class = html_class_attr(&header.classes);
    match header.role {
        TextRole::Heading => {
            let level = header.level.unwrap_or(1).clamp(1, 6);
            format!("  <h{level} id=\"chunk-{chunk_id}\"{class}>{escaped}</h{level}>\n")
        }
        TextRole::Paragraph => {
            format!("  <p data-chunk-id=\"{chunk_id}\"{class}>{escaped}</p>\n")
        }
        TextRole::ListItem => {
            let (open, close) = match header.list_kind.unwrap_or(ListKind::Bullet) {
                ListKind::Bullet => ("ul", "ul"),
                ListKind::Ordered => ("ol", "ol"),
            };
            format!("  <{open}><li data-chunk-id=\"{chunk_id}\"{class}>{escaped}</li></{close}>\n")
        }
        TextRole::Blockquote => {
            format!("  <blockquote data-chunk-id=\"{chunk_id}\"{class}>{escaped}</blockquote>\n")
        }
        TextRole::CodeBlock => {
            format!("  <pre data-chunk-id=\"{chunk_id}\"{class}><code>{escaped}</code></pre>\n")
        }
        TextRole::Table => {
            let rows = body.lines().fold(String::new(), |mut acc, line| {
                let cells = line.split('\t').fold(String::new(), |mut acc, cell| {
                    let _ = write!(acc, "<td>{}</td>", escape_html(cell));
                    acc
                });
                let _ = write!(acc, "<tr>{cells}</tr>");
                acc
            });
            format!("  <table data-chunk-id=\"{chunk_id}\"{class}><tbody>{rows}</tbody></table>\n")
        }
    }
}

fn append_cite_html(
    file: &TesFile,
    entry: &ChunkIndexEntry,
    cite_numbers: &std::collections::HashMap<u64, usize>,
    article: &mut String,
    bib_items: &mut Vec<(usize, BibEntry)>,
) -> Result<()> {
    let (n, cite, bib) = decode_numbered_cite(file, entry, cite_numbers)?;
    let label = cite
        .label
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(bib.cite_key.as_str());
    let label = if label.is_empty() { "unknown" } else { label };
    let marker = format_numeric_marker(n);
    if cite.quote.trim().is_empty() {
        let _ = writeln!(
            article,
            "  <p data-chunk-id=\"{}\" class=\"citation\"><a href=\"#ref-{n}\"><cite>{marker}</cite></a> <span class=\"cite-label\">{}</span></p>",
            entry.chunk_id,
            escape_html(label)
        );
    } else {
        let _ = writeln!(
            article,
            "  <p data-chunk-id=\"{}\" class=\"citation\"><a href=\"#ref-{n}\"><cite>{marker}</cite></a> {}</p>",
            entry.chunk_id,
            escape_html(cite.quote.trim())
        );
    }
    bib_items.push((n, bib));
    Ok(())
}

fn append_html_bibliography(article: &mut String, bib_items: &mut [(usize, BibEntry)]) {
    if bib_items.is_empty() {
        return;
    }
    bib_items.sort_by_key(|(n, _)| *n);
    article.push_str("  <section class=\"bibliography\">\n    <h2>References</h2>\n    <ol>\n");
    for (n, entry) in bib_items.iter() {
        let _ = writeln!(
            article,
            "      <li id=\"ref-{n}\">{}</li>",
            escape_html(&format_reference_body(entry))
        );
    }
    article.push_str("    </ol>\n  </section>\n");
}

fn html_theme_styles(options: &ExportOptions) -> String {
    let mut styles = String::new();
    if let Some(css) = &options.embedded_css {
        styles.push_str("<style>\n");
        styles.push_str(css);
        if !css.ends_with('\n') {
            styles.push('\n');
        }
        styles.push_str("</style>\n");
    } else if let Some(href) = &options.theme_href {
        let _ = writeln!(
            styles,
            "<link rel=\"stylesheet\" href=\"{}\">",
            escape_html(href)
        );
    }
    styles
}

fn render_figure_html(
    file: &TesFile,
    entry: &ChunkIndexEntry,
    options: &ExportOptions,
) -> Result<String> {
    let figure = decode_figure_entry(file, entry)?;
    let image_entry = file.chunk_by_id(figure.image_chunk_id)?;
    if image_entry.chunk_type != ChunkType::Image {
        return Err(TesError::InvalidFigure {
            message: format!(
                "figure {} points at chunk {} of type '{}'",
                entry.chunk_id,
                figure.image_chunk_id,
                image_entry.chunk_type.as_str()
            ),
        });
    }
    let image = {
        let raw = file.decode_payload(image_entry)?;
        ImagePayload::from_bytes(&raw).map_err(|e| TesError::Decode {
            chunk_id: image_entry.chunk_id,
            message: e.to_string(),
        })?
    };

    let src = if let Some(prefix) = &options.media_url_prefix {
        format!("{prefix}{}", figure.image_chunk_id)
    } else {
        format!(
            "data:{};base64,{}",
            image.media_type,
            base64_encode(&image.data)
        )
    };

    let mut dims = String::new();
    if image.width_px > 0 {
        let _ = write!(dims, " width=\"{}\"", image.width_px);
    }
    if image.height_px > 0 {
        let _ = write!(dims, " height=\"{}\"", image.height_px);
    }

    let region = match &figure.placement {
        crate::catalog::media::ImagePlacement::Region { name } => {
            format!(" data-region=\"{}\"", escape_html(name))
        }
        _ => String::new(),
    };

    let mut html = format!(
        "  <figure data-chunk-id=\"{}\" data-image-chunk=\"{}\" data-placement=\"{}\"{region}>\n    <img src=\"{}\" alt=\"{}\"{dims}>\n",
        entry.chunk_id,
        figure.image_chunk_id,
        figure.placement.as_str(),
        escape_html(&src),
        escape_html(&figure.alt_text),
    );
    if let Some(caption) = figure.caption.as_deref() {
        let _ = writeln!(
            html,
            "    <figcaption>{}</figcaption>",
            escape_html(caption)
        );
    }
    html.push_str("  </figure>\n");
    Ok(html)
}

fn markdown_escape_alt(value: &str) -> String {
    value.replace('[', "\\[").replace(']', "\\]")
}

fn escape_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn html_class_attr(classes: &[String]) -> String {
    if classes.is_empty() {
        String::new()
    } else {
        format!(" class=\"{}\"", escape_html(&classes.join(" ")))
    }
}

fn export_ai_text(file: &TesFile, options: &ExportOptions) -> Result<String> {
    let mut parts: Vec<String> = Vec::new();

    let entries: Vec<&ChunkIndexEntry> = if let Some(id) = options.chunk_id {
        vec![file.chunk_by_id(id)?]
    } else {
        file.reading_order_chunks()
    };

    for entry in entries {
        match entry.chunk_type {
            ChunkType::Text => {
                let (_header, body) = decode_text_entry(file, entry)?;
                let body = body.trim_end().to_owned();
                if body.is_empty() {
                    continue;
                }
                if options.annotate {
                    parts.push(format!("<!-- chunk:{} -->\n{body}", entry.chunk_id));
                } else {
                    parts.push(body);
                }
            }
            ChunkType::Cite if !options.no_cites => {
                let raw = file.decode_payload(entry)?;
                match CitePayload::from_bytes(&raw) {
                    Ok(cite) => {
                        let text = if cite.quote.trim().is_empty() {
                            format!(
                                "[citation unresolved: {}]",
                                cite.label.as_deref().unwrap_or("unknown")
                            )
                        } else if let Some(label) = cite.label.as_deref() {
                            // Plain sentence form; no markdown/HTML introduced.
                            format!("{label} reported that {}", cite.quote.trim())
                        } else {
                            cite.quote.trim().to_owned()
                        };
                        if options.annotate {
                            parts.push(format!("<!-- chunk:{} -->\n{text}", entry.chunk_id));
                        } else {
                            parts.push(text);
                        }
                    }
                    Err(_) => {
                        parts.push(format!("[citation unresolved: chunk {}]", entry.chunk_id));
                    }
                }
            }
            ChunkType::Figure => {
                let figure = decode_figure_entry(file, entry)?;
                let mut text = format!("[image: {}]", figure.alt_text.trim());
                if let Some(caption) = figure.caption.as_deref() {
                    text.push(' ');
                    text.push_str(caption.trim());
                }
                if options.annotate {
                    parts.push(format!("<!-- chunk:{} -->\n{text}", entry.chunk_id));
                } else {
                    parts.push(text);
                }
            }
            _ => {}
        }
    }

    let mut out = parts.join("\n\n");
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

#[derive(Serialize)]
struct ChunkJsonlRow<'a> {
    doc_id: &'a str,
    doc_title: &'a str,
    chunk_id: u64,
    chunk_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    level: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    list_kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    byte_len: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quote: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_doc_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_chunk_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolved_text: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_chunk_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    alt_text: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    caption: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    placement: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    media_type: Option<&'a str>,
}

fn export_chunks_jsonl(file: &TesFile, options: &ExportOptions) -> Result<String> {
    let doc_id = file.catalog().map_or("", |c| c.doc_id.as_str());
    let doc_title = file.catalog().map_or("", |c| c.title.as_str());
    let entries = jsonl_entries(file, options)?;

    let mut out = String::new();
    for entry in entries {
        match entry.chunk_type {
            ChunkType::Text => {
                append_jsonl_text(&mut out, file, entry, doc_id, doc_title)?;
            }
            ChunkType::Cite => {
                append_jsonl_cite(&mut out, file, entry, doc_id, doc_title)?;
            }
            ChunkType::Figure => {
                append_jsonl_figure(&mut out, file, entry, doc_id, doc_title)?;
            }
            other if options.all_types => {
                push_jsonl_row(
                    &mut out,
                    &ChunkJsonlRow::bare(doc_id, doc_title, entry.chunk_id, other.as_str()),
                )?;
            }
            _ => {}
        }
    }
    Ok(out)
}

fn jsonl_entries<'a>(
    file: &'a TesFile,
    options: &ExportOptions,
) -> Result<Vec<&'a ChunkIndexEntry>> {
    if let Some(id) = options.chunk_id {
        return Ok(vec![file.chunk_by_id(id)?]);
    }
    if options.all_types {
        return Ok(file.chunks().iter().collect());
    }
    Ok(file
        .reading_order_chunks()
        .into_iter()
        .filter(|c| {
            c.chunk_type == ChunkType::Text
                || c.chunk_type == ChunkType::Cite
                || c.chunk_type == ChunkType::Figure
        })
        .collect())
}

impl<'a> ChunkJsonlRow<'a> {
    fn bare(doc_id: &'a str, doc_title: &'a str, chunk_id: u64, chunk_type: &'static str) -> Self {
        Self {
            doc_id,
            doc_title,
            chunk_id,
            chunk_type,
            role: None,
            level: None,
            list_kind: None,
            byte_len: None,
            text: None,
            quote: None,
            target_doc_id: None,
            target_chunk_id: None,
            label: None,
            resolved_text: None,
            image_chunk_id: None,
            alt_text: None,
            caption: None,
            placement: None,
            media_type: None,
        }
    }
}

fn push_jsonl_row(out: &mut String, row: &ChunkJsonlRow<'_>) -> Result<()> {
    out.push_str(&serde_json::to_string(row)?);
    out.push('\n');
    Ok(())
}

fn append_jsonl_text(
    out: &mut String,
    file: &TesFile,
    entry: &ChunkIndexEntry,
    doc_id: &str,
    doc_title: &str,
) -> Result<()> {
    let (header, body) = decode_text_entry(file, entry)?;
    let list_kind = header.list_kind.map(|k| match k {
        ListKind::Bullet => "bullet",
        ListKind::Ordered => "ordered",
    });
    let mut row = ChunkJsonlRow::bare(doc_id, doc_title, entry.chunk_id, "text");
    row.role = Some(header.role.as_str());
    row.level = header.level;
    row.list_kind = list_kind;
    row.byte_len = Some(body.len());
    row.text = Some(&body);
    push_jsonl_row(out, &row)
}

fn append_jsonl_cite(
    out: &mut String,
    file: &TesFile,
    entry: &ChunkIndexEntry,
    doc_id: &str,
    doc_title: &str,
) -> Result<()> {
    let raw = file.decode_payload(entry)?;
    let cite = CitePayload::from_bytes(&raw).unwrap_or(CitePayload {
        quote: String::new(),
        target_doc_id: None,
        target_chunk_id: None,
        target_byte_start: None,
        target_byte_end: None,
        label: None,
        page: None,
        source: None,
    });
    let resolved = if cite.quote.is_empty() {
        None
    } else {
        Some(cite.quote.as_str())
    };
    let mut row = ChunkJsonlRow::bare(doc_id, doc_title, entry.chunk_id, "cite");
    row.quote = Some(cite.quote.as_str());
    row.target_doc_id = cite.target_doc_id.as_deref();
    row.target_chunk_id = cite.target_chunk_id;
    row.label = cite.label.as_deref();
    row.resolved_text = resolved;
    push_jsonl_row(out, &row)
}

fn append_jsonl_figure(
    out: &mut String,
    file: &TesFile,
    entry: &ChunkIndexEntry,
    doc_id: &str,
    doc_title: &str,
) -> Result<()> {
    let figure = decode_figure_entry(file, entry)?;
    let media_type = file
        .chunk_by_id(figure.image_chunk_id)
        .ok()
        .filter(|e| e.chunk_type == ChunkType::Image)
        .and_then(|e| file.decode_payload(e).ok())
        .and_then(|raw| ImagePayload::from_bytes(&raw).ok())
        .map(|img| img.media_type);
    let mut row = ChunkJsonlRow::bare(doc_id, doc_title, entry.chunk_id, "figure");
    row.image_chunk_id = Some(figure.image_chunk_id);
    row.alt_text = Some(figure.alt_text.as_str());
    row.caption = figure.caption.as_deref();
    row.placement = Some(figure.placement.as_str());
    row.media_type = media_type.as_deref();
    push_jsonl_row(out, &row)
}

/// Typed multimodal export parts for API adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiPart {
    /// Plain UTF-8 prose.
    Text(String),
    /// Image bytes referenced from a figure (or direct image chunk).
    Image {
        /// Figure chunk id when exported from a figure; else the image chunk id.
        chunk_id: u64,
        /// Target image chunk id holding the bytes.
        image_chunk_id: u64,
        /// IANA media type.
        media_type: String,
        /// Intrinsic width (0 = unknown).
        width_px: u32,
        /// Intrinsic height (0 = unknown).
        height_px: u32,
        /// Raw image bytes.
        data: Vec<u8>,
        /// Alt text from the figure (or empty for bare image).
        alt_text: String,
        /// Optional caption.
        caption: Option<String>,
    },
}

/// Export reading-order content as typed [`AiPart`]s (text + image bytes).
///
/// # Errors
///
/// Returns [`TesError::ChunkNotFound`] if a requested chunk is missing, or
/// [`TesError::Decode`] when a text/figure/image payload cannot be decoded.
pub fn export_ai_parts(file: &TesFile, options: &ExportOptions) -> Result<Vec<AiPart>> {
    let entries: Vec<&ChunkIndexEntry> = if let Some(id) = options.chunk_id {
        vec![file.chunk_by_id(id)?]
    } else {
        file.reading_order_chunks()
    };

    let mut parts = Vec::new();
    for entry in entries {
        match entry.chunk_type {
            ChunkType::Text => {
                let (_header, body) = decode_text_entry(file, entry)?;
                let body = body.trim_end().to_owned();
                if !body.is_empty() {
                    parts.push(AiPart::Text(body));
                }
            }
            ChunkType::Figure => {
                let figure = decode_figure_entry(file, entry)?;
                let image_entry = file.chunk_by_id(figure.image_chunk_id)?;
                let raw = file.decode_payload(image_entry)?;
                let image = ImagePayload::from_bytes(&raw).map_err(|e| TesError::Decode {
                    chunk_id: image_entry.chunk_id,
                    message: e.to_string(),
                })?;
                parts.push(AiPart::Image {
                    chunk_id: entry.chunk_id,
                    image_chunk_id: figure.image_chunk_id,
                    media_type: image.media_type,
                    width_px: image.width_px,
                    height_px: image.height_px,
                    data: image.data,
                    alt_text: figure.alt_text,
                    caption: figure.caption,
                });
            }
            ChunkType::Cite if !options.no_cites => {
                let raw = file.decode_payload(entry)?;
                if let Ok(cite) = CitePayload::from_bytes(&raw) {
                    let text = if cite.quote.trim().is_empty() {
                        format!(
                            "[citation unresolved: {}]",
                            cite.label.as_deref().unwrap_or("unknown")
                        )
                    } else if let Some(label) = cite.label.as_deref() {
                        format!("{label} reported that {}", cite.quote.trim())
                    } else {
                        cite.quote.trim().to_owned()
                    };
                    parts.push(AiPart::Text(text));
                }
            }
            _ => {}
        }
    }
    Ok(parts)
}

fn selected_text_entries<'a>(
    file: &'a TesFile,
    options: &ExportOptions,
) -> Result<Vec<&'a ChunkIndexEntry>> {
    if let Some(id) = options.chunk_id {
        let entry = file.chunk_by_id(id)?;
        if entry.chunk_type != ChunkType::Text {
            return Err(TesError::Decode {
                chunk_id: id,
                message: format!(
                    "chunk type is '{}'; --raw/--linear require text",
                    entry.chunk_type.as_str()
                ),
            });
        }
        return Ok(vec![entry]);
    }
    Ok(file
        .reading_order_chunks()
        .into_iter()
        .filter(|c| c.chunk_type == ChunkType::Text)
        .collect())
}

fn selected_content_entries<'a>(
    file: &'a TesFile,
    options: &ExportOptions,
) -> Result<Vec<&'a ChunkIndexEntry>> {
    if let Some(id) = options.chunk_id {
        let entry = file.chunk_by_id(id)?;
        if !matches!(
            entry.chunk_type,
            ChunkType::Text | ChunkType::Figure | ChunkType::Cite
        ) {
            return Err(TesError::Decode {
                chunk_id: id,
                message: format!(
                    "chunk type is '{}'; content exports require text, figure, or cite",
                    entry.chunk_type.as_str()
                ),
            });
        }
        return Ok(vec![entry]);
    }
    Ok(file
        .reading_order_chunks()
        .into_iter()
        .filter(|c| {
            matches!(
                c.chunk_type,
                ChunkType::Text | ChunkType::Figure | ChunkType::Cite
            )
        })
        .collect())
}

fn cite_number_map(
    file: &TesFile,
    entries: &[&ChunkIndexEntry],
) -> Result<std::collections::HashMap<u64, usize>> {
    let mut map = std::collections::HashMap::new();
    let mut n = 0usize;
    for entry in entries {
        if entry.chunk_type != ChunkType::Cite {
            continue;
        }
        // Ensure payload decodes so numbering stays aligned with valid cites.
        let _ = decode_cite_entry(file, entry)?;
        n += 1;
        map.insert(entry.chunk_id, n);
    }
    Ok(map)
}

fn decode_cite_entry(file: &TesFile, entry: &ChunkIndexEntry) -> Result<CitePayload> {
    let raw = file.decode_payload(entry)?;
    CitePayload::from_bytes(&raw).map_err(|e| TesError::Decode {
        chunk_id: entry.chunk_id,
        message: e.to_string(),
    })
}

fn decode_numbered_cite(
    file: &TesFile,
    entry: &ChunkIndexEntry,
    numbers: &std::collections::HashMap<u64, usize>,
) -> Result<(usize, CitePayload, BibEntry)> {
    let cite = decode_cite_entry(file, entry)?;
    let n = *numbers.get(&entry.chunk_id).unwrap_or(&0);
    let bib = if let Some(source) = &cite.source {
        source.clone()
    } else {
        BibEntry {
            cite_key: cite
                .label
                .clone()
                .unwrap_or_else(|| format!("chunk-{}", entry.chunk_id)),
            entry_type: "misc".into(),
            title: if cite.quote.trim().is_empty() {
                None
            } else {
                Some(cite.quote.clone())
            },
            note: cite.page.map(|p| format!("page {p}")),
            ..BibEntry::default()
        }
    };
    Ok((n, cite, bib))
}

fn decode_text_entry(file: &TesFile, entry: &ChunkIndexEntry) -> Result<(TextHeader, String)> {
    let raw = file.decode_payload(entry)?;
    decode_text_payload(&raw).map_err(|e| TesError::Decode {
        chunk_id: entry.chunk_id,
        message: e.to_string(),
    })
}

fn decode_figure_entry(file: &TesFile, entry: &ChunkIndexEntry) -> Result<FigureRef> {
    let raw = file.decode_payload(entry)?;
    FigureRef::from_bytes(&raw).map_err(|e| TesError::Decode {
        chunk_id: entry.chunk_id,
        message: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{DocumentCatalog, ListKind, TesWriterSession, TextHeader};
    use crate::layout::DocKind;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn write_note(dir: &std::path::Path) -> PathBuf {
        let path = dir.join("note.tes");
        let mut s = TesWriterSession::create(&path, DocKind::Note);
        s.set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440000",
            "Meeting notes",
            "2026-06-05T12:00:00Z",
            "2026-06-05T12:30:00Z",
            DocKind::Note,
        ))
        .unwrap();
        s.add_text_chunk(&TextHeader::paragraph(), "Hello from Tessera.")
            .unwrap();
        s.commit().unwrap();
        path
    }

    fn write_article(dir: &std::path::Path) -> PathBuf {
        let path = dir.join("article.tes");
        let mut s = TesWriterSession::create(&path, DocKind::Document);
        s.set_catalog(DocumentCatalog::new(
            "660e8400-e29b-41d4-a716-446655440001",
            "Methods",
            "2026-06-05T12:00:00Z",
            "2026-06-05T12:00:00Z",
            DocKind::Document,
        ))
        .unwrap();
        s.add_text_chunk(&TextHeader::heading(1), "Methods")
            .unwrap();
        s.add_text_chunk(
            &TextHeader::paragraph(),
            "We measured temperature at 15 stations.",
        )
        .unwrap();
        s.add_text_chunk(&TextHeader::list_item(ListKind::Bullet), "Calibrate first")
            .unwrap();
        s.commit().unwrap();
        path
    }

    #[test]
    fn raw_note_one_chunk_fixture() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/v0/note_one_chunk.tes");
        let out = export_view(&path, ExportView::Raw, &ExportOptions::default()).unwrap();
        assert_eq!(out, "Hello from Tessera.\n");
    }

    #[test]
    fn ai_text_has_no_exporter_markup() {
        let dir = tempdir().unwrap();
        let path = write_article(dir.path());
        let out = export_view(&path, ExportView::AiText, &ExportOptions::default()).unwrap();
        assert!(!out.contains('#'));
        assert!(!out.contains("<"));
        assert!(!out.contains("**"));
        assert!(out.contains("We measured temperature at 15 stations."));
        assert!(out.contains("Calibrate first"));
        // Heading body is included as plain text, without # markers.
        assert!(out.contains("Methods"));
    }

    #[test]
    fn linear_emits_heading_markers() {
        let dir = tempdir().unwrap();
        let path = write_article(dir.path());
        let out = export_view(&path, ExportView::Linear, &ExportOptions::default()).unwrap();
        assert!(out.starts_with("# Methods\n"));
        assert!(out.contains("\n- Calibrate first\n"));
    }

    #[test]
    fn markdown_preserves_block_structure_lossily() {
        let dir = tempdir().unwrap();
        let path = write_article(dir.path());
        let out = export_view(&path, ExportView::Markdown, &ExportOptions::default()).unwrap();
        assert!(out.starts_with("# Methods\n\n"));
        assert!(out.contains("\n\n- Calibrate first\n"));
    }

    #[test]
    fn chunks_jsonl_line_count_matches_reading_order() {
        let dir = tempdir().unwrap();
        let path = write_article(dir.path());
        let out = export_view(&path, ExportView::ChunksJsonl, &ExportOptions::default()).unwrap();
        let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 3);
        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["role"], "heading");
        assert_eq!(first["text"], "Methods");
        assert_eq!(first["doc_title"], "Methods");
    }

    #[test]
    fn raw_chunk_filter() {
        let dir = tempdir().unwrap();
        let path = write_article(dir.path());
        let opts = ExportOptions {
            chunk_id: Some(2),
            ..Default::default()
        };
        let out = export_view(&path, ExportView::Raw, &opts).unwrap();
        assert_eq!(out, "We measured temperature at 15 stations.\n");
    }

    #[test]
    fn annotate_ai_text() {
        let dir = tempdir().unwrap();
        let path = write_note(dir.path());
        let opts = ExportOptions {
            annotate: true,
            ..Default::default()
        };
        let out = export_view(&path, ExportView::AiText, &opts).unwrap();
        assert!(out.starts_with("<!-- chunk:1 -->\nHello from Tessera."));
    }

    #[test]
    fn reusable_image_two_figures_and_exports() {
        use crate::catalog::{FigureRef, ImagePayload, ImagePlacement};
        use std::fs;

        let dir = tempdir().unwrap();
        let jpeg = fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/assets/images/square.jpg"),
        )
        .unwrap();
        let path = dir.path().join("figures.tes");
        let mut s = TesWriterSession::create(&path, DocKind::Document);
        s.set_catalog(DocumentCatalog::new(
            "770e8400-e29b-41d4-a716-446655440002",
            "Figures",
            "2026-07-25T00:00:00Z",
            "2026-07-25T00:00:00Z",
            DocKind::Document,
        ))
        .unwrap();
        s.add_text_chunk(&TextHeader::heading(1), "Gallery")
            .unwrap();
        let image_id = s
            .add_image_chunk(&ImagePayload {
                media_type: "image/jpeg".into(),
                width_px: 100,
                height_px: 100,
                data: jpeg,
            })
            .unwrap();
        s.add_figure(&FigureRef {
            image_chunk_id: image_id,
            alt_text: "Square crop".into(),
            caption: Some("First use".into()),
            placement: ImagePlacement::Flow,
        })
        .unwrap();
        s.add_figure(&FigureRef {
            image_chunk_id: image_id,
            alt_text: "Square crop again".into(),
            caption: Some("Second use, full width".into()),
            placement: ImagePlacement::FullWidth,
        })
        .unwrap();
        s.commit().unwrap();

        let file = crate::catalog::TesFile::open(&path).unwrap();
        assert_eq!(file.chunks().len(), 4); // heading + image + 2 figures
        let html = export_file(&file, ExportView::Html, &ExportOptions::default()).unwrap();
        assert!(html.contains("<figure data-chunk-id=\"3\""));
        assert!(html.contains("<figure data-chunk-id=\"4\""));
        assert!(html.contains("data-image-chunk=\"2\""));
        assert!(html.contains("data:image/jpeg;base64,"));
        assert_eq!(html.matches("data:image/jpeg;base64,").count(), 2);

        let md = export_file(&file, ExportView::Markdown, &ExportOptions::default()).unwrap();
        assert!(md.contains("![Square crop](media:chunk-2)"));
        assert!(md.contains("![Square crop again](media:chunk-2)"));

        let parts = export_ai_parts(&file, &ExportOptions::default()).unwrap();
        assert!(matches!(parts[0], AiPart::Text(_)));
        assert!(matches!(
            &parts[1],
            AiPart::Image {
                image_chunk_id: 2,
                alt_text,
                ..
            } if alt_text == "Square crop"
        ));
        assert!(matches!(
            &parts[2],
            AiPart::Image {
                image_chunk_id: 2,
                ..
            }
        ));
        // Same underlying bytes reused.
        let AiPart::Image { data: d1, .. } = &parts[1] else {
            panic!("expected image");
        };
        let AiPart::Image { data: d2, .. } = &parts[2] else {
            panic!("expected image");
        };
        assert_eq!(d1, d2);

        let report = crate::verify::verify_tes_file(&path, true).unwrap();
        assert!(report.ok, "{:?}", report.findings);
    }

    #[test]
    fn research_cites_mirror_tlnk_and_export() {
        use crate::bib::{BibEntry, BibFormat, export_bibliography, import_bibliography};
        use crate::catalog::link::LinkKind;
        use crate::catalog::{CitePayload, TesFile};

        let dir = tempdir().unwrap();
        let sample =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/assets/citations/sample.bib");
        let bib_tes = dir.path().join("from_bib.tes");
        import_bibliography(
            &sample,
            &bib_tes,
            BibFormat::Bibtex,
            &crate::bib::BibImportOptions::default(),
        )
        .unwrap();

        let target = "770e8400-e29b-41d4-a716-446655440099";
        let path = dir.path().join("paper.tes");
        let mut catalog = DocumentCatalog::new(
            "880e8400-e29b-41d4-a716-446655440088",
            "Cite specimen",
            "2026-07-25T00:00:00Z",
            "2026-07-25T00:00:00Z",
            DocKind::Research,
        );
        catalog.cite_style_id = Some("numeric".into());
        let mut session = TesWriterSession::create(&path, DocKind::Research);
        session.set_catalog(catalog).unwrap();
        session
            .add_text_chunk(
                &TextHeader::paragraph(),
                "Prior work established the baseline.",
            )
            .unwrap();
        session
            .add_cite_chunk(&CitePayload {
                quote: "Chunk-oriented containers help.".into(),
                target_doc_id: Some(target.into()),
                target_chunk_id: Some(1),
                target_byte_start: Some(0),
                target_byte_end: Some(12),
                label: Some("keller2020chunking".into()),
                page: Some(3),
                source: Some(BibEntry {
                    cite_key: "keller2020chunking".into(),
                    entry_type: "article".into(),
                    author: Some("Keller, Ada and Hurowitz, Alex".into()),
                    title: Some("Chunk-Oriented Document Containers for Local-First Notes".into()),
                    journal: Some("Fixtures Review".into()),
                    year: Some("2020".into()),
                    ..BibEntry::default()
                }),
            })
            .unwrap();
        session.commit().unwrap();

        let file = TesFile::open(&path).unwrap();
        assert_eq!(file.links().len(), 1);
        assert_eq!(file.links()[0].link_kind, LinkKind::Citation);
        assert_eq!(file.links()[0].source_chunk_id, 2);

        let md = export_view(&path, ExportView::Markdown, &ExportOptions::default()).unwrap();
        assert!(md.contains("[@keller2020chunking]"));
        assert!(md.contains("## References"));

        let html = export_view(&path, ExportView::Html, &ExportOptions::default()).unwrap();
        assert!(html.contains("class=\"citation\""));
        assert!(html.contains("class=\"bibliography\""));
        assert!(html.contains("[1]"));

        let bibtex = export_bibliography(&path, BibFormat::Bibtex).unwrap();
        assert!(bibtex.contains("@article{keller2020chunking,"));

        let from_bib = TesFile::open(&bib_tes).unwrap();
        assert_eq!(
            from_bib
                .reading_order_chunks()
                .iter()
                .filter(|c| c.chunk_type == ChunkType::Cite)
                .count(),
            3
        );

        let report = crate::verify::verify_tes_file(&path, true).unwrap();
        assert!(report.ok, "{:?}", report.findings);
        assert!(
            !report.findings.iter().any(|f| f.check == "cite.mirror"),
            "{:?}",
            report.findings
        );
    }
}
