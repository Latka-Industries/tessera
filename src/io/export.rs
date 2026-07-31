//! Decoded export views under [`crate::io`] (`docs/exports.md`).
//!
//! Exports are **projections** of a sealed `.tes` file — never the canonical
//! source. Models and pipelines should call these views rather than hex-dumping
//! the wire format.

use std::fmt::Write as _;
use std::path::Path;

use serde::Serialize;

use super::bib::{
    BibEntry, format_numeric_marker, format_numeric_reference, format_pandoc_cite,
    format_reference_body,
};
use crate::catalog::chunk::{CitePayload, ListKind, TextHeader, TextRole, decode_text_payload};
use crate::catalog::file::TesFile;
use crate::catalog::index::{ChunkIndexEntry, ChunkType};
use crate::catalog::media::{AttachmentPayload, FigureRef, ImagePayload, base64_encode};
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
#[allow(clippy::struct_excessive_bools)]
pub struct ExportOptions {
    /// Restrict output to a single chunk id (where applicable).
    pub chunk_id: Option<u64>,
    /// Restrict output to the Nth chapter (1-based), bounded by level-1 headings.
    ///
    /// Mutually exclusive with [`Self::chunk_id`]. See manuscript conventions in
    /// `docs/decisions.md`.
    pub chapter: Option<u32>,
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
    /// When set, attachment download links use `{prefix}{attachment_chunk_id}`.
    ///
    /// Attachments are never inlined as data URIs.
    pub attachment_url_prefix: Option<String>,
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
                append_linear_text(&mut out, &header, &body);
            }
            ChunkType::Figure => {
                let figure = decode_figure_entry(file, entry)?;
                append_linear_figure(&mut out, &figure);
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
            ChunkType::Slide => {
                let slide = decode_slide_entry(file, entry)?;
                let _ = writeln!(out, "[slide layout={}]", slide.layout_id);
                for region in &slide.regions {
                    let _ = writeln!(out, "  {}: chunk-{}", region.name, region.chunk_id);
                }
            }
            ChunkType::Attachment => {
                let att = decode_attachment_entry(file, entry)?;
                append_linear_attachment(&mut out, &att);
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

fn append_linear_text(out: &mut String, header: &TextHeader, body: &str) {
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
                ListKind::Bullet => "- ",
                ListKind::Ordered => "1. ",
            };
            let indent = "  ".repeat(header.list_depth_or_default().saturating_sub(1) as usize);
            out.push_str(&indent);
            out.push_str(marker);
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
            out.push_str("```");
            if let Some(lang) = header.code_lang.as_deref() {
                out.push_str(lang);
            }
            out.push('\n');
            out.push_str(body.trim_end());
            out.push_str("\n```\n");
        }
        TextRole::Math => {
            out.push_str("$$\n");
            out.push_str(body.trim_end());
            out.push_str("\n$$\n");
        }
        TextRole::Paragraph | TextRole::Table => {
            if header.role == TextRole::Table && header.table.is_some() {
                out.push_str(&header.render_markdown(""));
            } else {
                out.push_str(body.trim_end());
            }
            out.push('\n');
        }
    }
}

fn append_linear_figure(out: &mut String, figure: &FigureRef) {
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

fn append_linear_attachment(out: &mut String, att: &AttachmentPayload) {
    let _ = writeln!(
        out,
        "[attachment filename={} media_type={} sha256={}]",
        att.filename, att.media_type, att.sha256
    );
    if let Some(caption) = att.caption.as_deref() {
        let _ = writeln!(out, "{caption}");
    }
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
                parts.push(header.render_markdown_with_links(&body, file.links()));
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
            ChunkType::Slide => {
                let slide = decode_slide_entry(file, entry)?;
                let mut block = format!("<!-- slide layout={} -->", slide.layout_id);
                for region in &slide.regions {
                    let _ = write!(block, "\n[{}]: chunk-{}", region.name, region.chunk_id);
                }
                parts.push(block);
            }
            ChunkType::Attachment => {
                let att = decode_attachment_entry(file, entry)?;
                let mut block = format!(
                    "*Attachment:* `{}` (`{}`)",
                    att.filename.replace('`', "'"),
                    att.media_type
                );
                if let Some(caption) = att.caption.as_deref() {
                    block.push_str(" — ");
                    block.push_str(caption.trim());
                }
                parts.push(block);
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
    if options.chunk_id.is_none() && options.chapter.is_none() && file_has_slides(file) {
        return export_deck_html(file, options);
    }

    let entries = selected_content_entries(file, options)?;
    let cite_numbers = cite_number_map(file, &entries)?;
    let doc_id = file.catalog().map_or("", |catalog| catalog.doc_id.as_str());
    let mut article = format!("<article data-doc-id=\"{}\">\n", escape_html(doc_id));
    let mut bib_items: Vec<(usize, BibEntry)> = Vec::new();
    let mut list_stack: Vec<(ListKind, u32)> = Vec::new();

    for entry in entries {
        match entry.chunk_type {
            ChunkType::Text => {
                let (header, body) = decode_text_entry(file, entry)?;
                if header.role == TextRole::ListItem {
                    append_list_item_html(
                        &mut article,
                        &mut list_stack,
                        entry.chunk_id,
                        &header,
                        &body,
                        file.links(),
                    );
                } else {
                    close_all_lists(&mut article, &mut list_stack);
                    article.push_str(&render_text_chunk_html(
                        entry.chunk_id,
                        &header,
                        &body,
                        file.links(),
                    ));
                }
            }
            ChunkType::Figure => {
                close_all_lists(&mut article, &mut list_stack);
                article.push_str(&render_figure_html(file, entry, options)?);
            }
            ChunkType::Cite if !options.no_cites => {
                close_all_lists(&mut article, &mut list_stack);
                append_cite_html(file, entry, &cite_numbers, &mut article, &mut bib_items)?;
            }
            ChunkType::Slide => {
                close_all_lists(&mut article, &mut list_stack);
                article.push_str(&render_slide_html(file, entry, options)?);
            }
            ChunkType::Attachment => {
                close_all_lists(&mut article, &mut list_stack);
                article.push_str(&render_attachment_html(file, entry, options)?);
            }
            _ => {}
        }
    }
    close_all_lists(&mut article, &mut list_stack);
    append_html_bibliography(&mut article, &mut bib_items);
    article.push_str("</article>\n");

    Ok(wrap_html_document(file, options, &article))
}

fn export_deck_html(file: &TesFile, options: &ExportOptions) -> Result<String> {
    let doc_id = file.catalog().map_or("", |catalog| catalog.doc_id.as_str());
    let mut deck = format!(
        "<main class=\"deck\" data-doc-id=\"{}\">\n",
        escape_html(doc_id)
    );
    for entry in file.reading_order_chunks() {
        if entry.chunk_type != ChunkType::Slide {
            continue;
        }
        deck.push_str(&render_slide_html(file, entry, options)?);
    }
    deck.push_str("</main>\n");
    Ok(wrap_html_document(file, options, &deck))
}

fn wrap_html_document(file: &TesFile, options: &ExportOptions, body: &str) -> String {
    let title = file
        .catalog()
        .map_or("Untitled", |catalog| catalog.title.as_str());
    let styles = html_theme_styles(options);
    if options.standalone {
        format!(
            "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n{styles}</head>\n<body>\n{body}</body>\n</html>\n",
            escape_html(title)
        )
    } else {
        format!("{styles}{body}")
    }
}

fn file_has_slides(file: &TesFile) -> bool {
    file.chunks()
        .iter()
        .any(|c| c.chunk_type == ChunkType::Slide)
}

fn render_slide_html(
    file: &TesFile,
    entry: &ChunkIndexEntry,
    options: &ExportOptions,
) -> Result<String> {
    let slide = decode_slide_entry(file, entry)?;
    let layout = escape_html(&slide.layout_id);
    let mut out = format!(
        "  <section class=\"slide\" data-chunk-id=\"{}\" data-layout=\"{layout}\">\n",
        entry.chunk_id
    );
    for region in &slide.regions {
        let name = escape_html(&region.name);
        let _ = writeln!(
            out,
            "    <div class=\"region region-{name}\" data-region=\"{name}\">"
        );
        out.push_str(&render_region_chunk_html(file, region.chunk_id, options)?);
        out.push_str("    </div>\n");
    }
    out.push_str("  </section>\n");
    Ok(out)
}

fn render_region_chunk_html(
    file: &TesFile,
    chunk_id: u64,
    options: &ExportOptions,
) -> Result<String> {
    let entry = file.chunk_by_id(chunk_id)?;
    match entry.chunk_type {
        ChunkType::Text => {
            let (header, body) = decode_text_entry(file, entry)?;
            Ok(render_text_chunk_html(
                entry.chunk_id,
                &header,
                &body,
                file.links(),
            ))
        }
        ChunkType::Figure => render_figure_html(file, entry, options),
        ChunkType::Cite => {
            let mut buf = String::new();
            let mut bib = Vec::new();
            let cite_numbers = cite_number_map(file, &[entry])?;
            append_cite_html(file, entry, &cite_numbers, &mut buf, &mut bib)?;
            Ok(buf)
        }
        ChunkType::Image => {
            let raw = file.decode_payload(entry)?;
            let image = crate::catalog::ImagePayload::from_bytes(raw.as_ref())?;
            let src = image_src(options, entry.chunk_id, &image.media_type, &image.data);
            Ok(format!(
                "      <img data-chunk-id=\"{}\" src=\"{}\" alt=\"\">\n",
                entry.chunk_id,
                escape_html(&src)
            ))
        }
        other => Err(TesError::Decode {
            chunk_id,
            message: format!(
                "slide region target type '{}' is not renderable",
                other.as_str()
            ),
        }),
    }
}

fn render_text_chunk_html(
    chunk_id: u64,
    header: &TextHeader,
    body: &str,
    links: &[crate::catalog::LinkEntry],
) -> String {
    let inner = apply_spans_html(body, &header.spans, links);
    let class = html_class_attr(&header.classes);
    match header.role {
        TextRole::Heading => {
            let level = header.level.unwrap_or(1).clamp(1, 6);
            format!("  <h{level} id=\"chunk-{chunk_id}\"{class}>{inner}</h{level}>\n")
        }
        TextRole::Paragraph => {
            format!("  <p data-chunk-id=\"{chunk_id}\"{class}>{inner}</p>\n")
        }
        TextRole::ListItem => {
            // Isolated list items (e.g. slide regions) still need a wrapping list.
            let mut out = String::new();
            let mut stack = Vec::new();
            append_list_item_html(&mut out, &mut stack, chunk_id, header, body, links);
            close_all_lists(&mut out, &mut stack);
            out
        }
        TextRole::Blockquote => {
            format!("  <blockquote data-chunk-id=\"{chunk_id}\"{class}>{inner}</blockquote>\n")
        }
        TextRole::CodeBlock => {
            let escaped = escape_html(body);
            let lang = header
                .code_lang
                .as_deref()
                .map(|l| format!(" class=\"language-{}\"", escape_html(l)))
                .unwrap_or_default();
            format!(
                "  <pre data-chunk-id=\"{chunk_id}\"{class}><code{lang}>{escaped}</code></pre>\n"
            )
        }
        TextRole::Table => {
            if let Some(table) = &header.table {
                let mut rows = String::new();
                for (i, row) in table.rows.iter().enumerate() {
                    let cells = row.cells.iter().fold(String::new(), |mut acc, cell| {
                        let tag = if cell.is_header || i == 0 { "th" } else { "td" };
                        let _ = write!(acc, "<{tag}>{}</{tag}>", escape_html(cell.text.as_str()));
                        acc
                    });
                    let _ = write!(rows, "<tr>{cells}</tr>");
                }
                format!(
                    "  <table data-chunk-id=\"{chunk_id}\"{class}><tbody>{rows}</tbody></table>\n"
                )
            } else {
                let rows = body.lines().fold(String::new(), |mut acc, line| {
                    let cells = line.split('\t').fold(String::new(), |mut acc, cell| {
                        let _ = write!(acc, "<td>{}</td>", escape_html(cell));
                        acc
                    });
                    let _ = write!(acc, "<tr>{cells}</tr>");
                    acc
                });
                format!(
                    "  <table data-chunk-id=\"{chunk_id}\"{class}><tbody>{rows}</tbody></table>\n"
                )
            }
        }
        TextRole::Math => render_math_html(chunk_id, body, &class, true),
    }
}

/// Coalesce consecutive list-item chunks into real `<ul>` / `<ol>` trees.
///
/// Each Tessera list item is its own chunk. Emitting one list per chunk made
/// ordered lists restart at `1.` for every item. Nesting follows `list_depth`.
fn append_list_item_html(
    out: &mut String,
    stack: &mut Vec<(ListKind, u32)>,
    chunk_id: u64,
    header: &TextHeader,
    body: &str,
    links: &[crate::catalog::LinkEntry],
) {
    let kind = header.list_kind.unwrap_or(ListKind::Bullet);
    let depth = header.list_depth_or_default();
    let inner = apply_spans_html(body, &header.spans, links);
    let class = html_class_attr(&header.classes);

    // Close deeper nested lists.
    while stack.last().is_some_and(|(_, d)| *d > depth) {
        close_one_list(out, stack);
    }

    // Same depth, different kind → close and reopen.
    if stack.last().is_some_and(|(k, d)| *d == depth && *k != kind) {
        close_one_list(out, stack);
    }

    // Same depth, same kind → close previous `<li>` only.
    if stack.last().is_some_and(|(k, d)| *d == depth && *k == kind) {
        out.push_str("</li>\n");
    } else {
        // Open lists from current depth+1 up to `depth`.
        let mut next_depth = stack.last().map_or(1, |(_, d)| d + 1);
        while next_depth <= depth {
            let (open, _) = list_tags(kind);
            let _ = writeln!(out, "  <{open} data-list-depth=\"{next_depth}\">");
            stack.push((kind, next_depth));
            next_depth += 1;
        }
    }

    let _ = write!(out, "    <li data-chunk-id=\"{chunk_id}\"{class}>{inner}");
}

fn close_all_lists(out: &mut String, stack: &mut Vec<(ListKind, u32)>) {
    while !stack.is_empty() {
        close_one_list(out, stack);
    }
}

fn close_one_list(out: &mut String, stack: &mut Vec<(ListKind, u32)>) {
    let Some((kind, _)) = stack.pop() else {
        return;
    };
    let (_, close) = list_tags(kind);
    let _ = writeln!(out, "</li>\n  </{close}>");
}

fn list_tags(kind: ListKind) -> (&'static str, &'static str) {
    match kind {
        ListKind::Bullet => ("ul", "ul"),
        ListKind::Ordered => ("ol", "ol"),
    }
}

fn render_math_html(chunk_id: u64, body: &str, class: &str, display: bool) -> String {
    if let Ok(mathml) = render_latex_mathml(body.trim(), display) {
        if display {
            format!(
                "  <div data-chunk-id=\"{chunk_id}\" class=\"math-display\"{class}>{mathml}</div>\n"
            )
        } else {
            format!("<span class=\"math-inline\"{class}>{mathml}</span>")
        }
    } else {
        let escaped = escape_html(body.trim());
        if display {
            format!(
                "  <div data-chunk-id=\"{chunk_id}\" class=\"math-display math-fallback\"{class}><code>{escaped}</code></div>\n"
            )
        } else {
            format!("<code class=\"math-inline math-fallback\"{class}>{escaped}</code>")
        }
    }
}

fn render_latex_mathml(tex: &str, display: bool) -> std::result::Result<String, ()> {
    use std::sync::OnceLock;

    use katex::{KatexContext, OutputFormat, Settings, render_to_string};

    static CTX: OnceLock<KatexContext> = OnceLock::new();
    let ctx = CTX.get_or_init(KatexContext::default);

    let settings = Settings::builder()
        .display_mode(display)
        .output(OutputFormat::Mathml)
        .throw_on_error(false)
        .build();
    render_to_string(ctx, tex, &settings).map_err(|_| ())
}

fn apply_spans_html(
    body: &str,
    spans: &[crate::catalog::InlineSpan],
    links: &[crate::catalog::LinkEntry],
) -> String {
    use crate::catalog::InlineKind;
    if spans.is_empty() {
        return escape_html(body);
    }

    // Replace from the end so earlier byte offsets stay valid (non-overlapping spans).
    let mut ordered: Vec<&crate::catalog::InlineSpan> = spans.iter().collect();
    ordered.sort_by_key(|s| std::cmp::Reverse(s.start));
    let mut work = body.to_owned();
    let mut replacements: Vec<(String, String)> = Vec::new();
    for (i, span) in ordered.iter().enumerate() {
        let start = span.start as usize;
        let end = span.end as usize;
        if end > work.len() || start >= end {
            continue;
        }
        if !work.is_char_boundary(start) || !work.is_char_boundary(end) {
            continue;
        }
        let inner = work[start..end].to_owned();
        let token = format!("\u{0001}S{i}\u{0001}");
        let html = match &span.kind {
            InlineKind::Emphasis | InlineKind::Term => {
                format!("<em>{}</em>", escape_html(&inner))
            }
            InlineKind::Strong => format!("<strong>{}</strong>", escape_html(&inner)),
            InlineKind::Underline => format!("<u>{}</u>", escape_html(&inner)),
            InlineKind::Code => format!("<code>{}</code>", escape_html(&inner)),
            InlineKind::Quote => format!("<q>{}</q>", escape_html(&inner)),
            InlineKind::Math { tex } => {
                if let Ok(mathml) = render_latex_mathml(tex, false) {
                    format!("<span class=\"math-inline\">{mathml}</span>")
                } else {
                    format!(
                        "<code class=\"math-inline math-fallback\">{}</code>",
                        escape_html(tex)
                    )
                }
            }
            InlineKind::Link { link_id } => match links.get(*link_id as usize) {
                Some(entry) => format!(
                    "<a href=\"{}\">{}</a>",
                    escape_html(&entry.target.html_href()),
                    escape_html(&inner)
                ),
                None => escape_html(&inner),
            },
            InlineKind::Citation { .. } => escape_html(&inner),
        };
        replacements.push((token.clone(), html));
        work.replace_range(start..end, &token);
    }
    let mut out = escape_html(&work);
    for (token, html) in replacements {
        out = out.replace(&token, &html);
    }
    out
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

    let src = image_src(
        options,
        figure.image_chunk_id,
        &image.media_type,
        &image.data,
    );

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

    let entries = reading_order_scoped(file, options)?;

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
                        let text = format_ai_cite_prose(&cite);
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
            ChunkType::Attachment => {
                let att = decode_attachment_entry(file, entry)?;
                let mut text = format!(
                    "[attachment: {} ({}) sha256={}]",
                    att.filename, att.media_type, att.sha256
                );
                if let Some(caption) = att.caption.as_deref() {
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
    list_depth: Option<u32>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    layout_id: Option<&'a str>,
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
            ChunkType::Slide => {
                append_jsonl_slide(&mut out, file, entry, doc_id, doc_title)?;
            }
            ChunkType::Attachment => {
                append_jsonl_attachment(&mut out, file, entry, doc_id, doc_title)?;
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
    if options.chunk_id.is_some() || options.chapter.is_some() {
        let scoped = reading_order_scoped(file, options)?;
        if options.all_types {
            return Ok(scoped);
        }
        return Ok(scoped
            .into_iter()
            .filter(|c| is_content_export_type(c.chunk_type))
            .collect());
    }
    if options.all_types {
        return Ok(file.chunks().iter().collect());
    }
    Ok(file
        .reading_order_chunks()
        .into_iter()
        .filter(|c| is_content_export_type(c.chunk_type))
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
            list_depth: None,
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
            layout_id: None,
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
    if header.role == TextRole::ListItem {
        let depth = header.list_depth_or_default();
        if depth > 1 {
            row.list_depth = Some(depth);
        }
    }
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

fn append_jsonl_slide(
    out: &mut String,
    file: &TesFile,
    entry: &ChunkIndexEntry,
    doc_id: &str,
    doc_title: &str,
) -> Result<()> {
    let slide = decode_slide_entry(file, entry)?;
    let summary = slide
        .regions
        .iter()
        .map(|r| format!("{}={}", r.name, r.chunk_id))
        .collect::<Vec<_>>()
        .join(",");
    let owned = summary; // keep alive for row
    let mut row = ChunkJsonlRow::bare(doc_id, doc_title, entry.chunk_id, "slide");
    row.layout_id = Some(slide.layout_id.as_str());
    row.text = Some(owned.as_str());
    push_jsonl_row(out, &row)
}

fn append_jsonl_attachment(
    out: &mut String,
    file: &TesFile,
    entry: &ChunkIndexEntry,
    doc_id: &str,
    doc_title: &str,
) -> Result<()> {
    let att = decode_attachment_entry(file, entry)?;
    let mut row = ChunkJsonlRow::bare(doc_id, doc_title, entry.chunk_id, "attachment");
    row.text = Some(att.filename.as_str());
    row.media_type = Some(att.media_type.as_str());
    row.caption = att.caption.as_deref();
    row.label = Some(att.sha256.as_str());
    row.byte_len = Some(att.data.len());
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
    let entries = reading_order_scoped(file, options)?;

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
                    parts.push(AiPart::Text(format_ai_cite_prose(&cite)));
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
    let entries = reading_order_scoped(file, options)?;
    if let Some(id) = options.chunk_id {
        let entry = entries[0];
        if entry.chunk_type != ChunkType::Text {
            return Err(TesError::Decode {
                chunk_id: id,
                message: format!(
                    "chunk type is '{}'; --raw/--linear require text",
                    entry.chunk_type.as_str()
                ),
            });
        }
        return Ok(entries);
    }
    Ok(entries
        .into_iter()
        .filter(|c| c.chunk_type == ChunkType::Text)
        .collect())
}

fn selected_content_entries<'a>(
    file: &'a TesFile,
    options: &ExportOptions,
) -> Result<Vec<&'a ChunkIndexEntry>> {
    let entries = reading_order_scoped(file, options)?;
    if let Some(id) = options.chunk_id {
        let entry = entries[0];
        if !is_content_export_type(entry.chunk_type) {
            return Err(TesError::Decode {
                chunk_id: id,
                message: format!(
                    "chunk type is '{}'; content exports require text, figure, cite, slide, or attachment",
                    entry.chunk_type.as_str()
                ),
            });
        }
        return Ok(entries);
    }
    Ok(entries
        .into_iter()
        .filter(|c| is_content_export_type(c.chunk_type))
        .collect())
}

/// Reading-order chunks, optionally scoped to `--chunk` or `--chapter`.
fn reading_order_scoped<'a>(
    file: &'a TesFile,
    options: &ExportOptions,
) -> Result<Vec<&'a ChunkIndexEntry>> {
    if options.chunk_id.is_some() && options.chapter.is_some() {
        return Err(TesError::ExportScope {
            message: "--chunk and --chapter are mutually exclusive".into(),
        });
    }
    if let Some(id) = options.chunk_id {
        return Ok(vec![file.chunk_by_id(id)?]);
    }
    let entries = file.reading_order_chunks();
    if let Some(chapter) = options.chapter {
        return chapter_slice(file, &entries, chapter);
    }
    Ok(entries)
}

/// Default heading level that opens a manuscript chapter (H1).
const CHAPTER_HEADING_LEVEL: u32 = 1;

/// Slice reading-order entries to the Nth chapter (1-based).
///
/// A chapter starts at a text heading with level [`CHAPTER_HEADING_LEVEL`] and
/// ends just before the next such heading. Front matter before the first H1 is
/// excluded. Scene headings (H2+) stay inside their parent chapter.
fn chapter_slice<'a>(
    file: &'a TesFile,
    entries: &[&'a ChunkIndexEntry],
    chapter: u32,
) -> Result<Vec<&'a ChunkIndexEntry>> {
    if chapter == 0 {
        return Err(TesError::ExportScope {
            message: "--chapter must be >= 1 (1-based)".into(),
        });
    }
    let mut starts: Vec<usize> = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        if is_chapter_heading(file, entry)? {
            starts.push(i);
        }
    }
    if starts.is_empty() {
        return Err(TesError::ExportScope {
            message: format!(
                "no chapter headings (H{CHAPTER_HEADING_LEVEL}) found; cannot select --chapter {chapter}"
            ),
        });
    }
    let idx = (chapter as usize).saturating_sub(1);
    let Some(&start) = starts.get(idx) else {
        return Err(TesError::ExportScope {
            message: format!(
                "chapter {chapter} not found (document has {} chapter{})",
                starts.len(),
                if starts.len() == 1 { "" } else { "s" }
            ),
        });
    };
    let end = starts.get(idx + 1).copied().unwrap_or(entries.len());
    Ok(entries[start..end].to_vec())
}

fn is_chapter_heading(file: &TesFile, entry: &ChunkIndexEntry) -> Result<bool> {
    if entry.chunk_type != ChunkType::Text {
        return Ok(false);
    }
    let (header, _) = decode_text_entry(file, entry)?;
    Ok(header.role == TextRole::Heading
        && header.level.unwrap_or(CHAPTER_HEADING_LEVEL) == CHAPTER_HEADING_LEVEL)
}

fn is_content_export_type(chunk_type: ChunkType) -> bool {
    matches!(
        chunk_type,
        ChunkType::Text
            | ChunkType::Figure
            | ChunkType::Cite
            | ChunkType::Slide
            | ChunkType::Attachment
    )
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

fn decode_attachment_entry(file: &TesFile, entry: &ChunkIndexEntry) -> Result<AttachmentPayload> {
    let raw = file.decode_payload(entry)?;
    AttachmentPayload::from_bytes(&raw).map_err(|e| TesError::Decode {
        chunk_id: entry.chunk_id,
        message: e.to_string(),
    })
}

fn render_attachment_html(
    file: &TesFile,
    entry: &ChunkIndexEntry,
    options: &ExportOptions,
) -> Result<String> {
    let att = decode_attachment_entry(file, entry)?;
    let href = options
        .attachment_url_prefix
        .as_ref()
        .map(|prefix| format!("{prefix}{}", entry.chunk_id));
    let caption = att
        .caption
        .as_deref()
        .map(|c| format!("\n    <p class=\"caption\">{}</p>", escape_html(c)))
        .unwrap_or_default();
    let link = if let Some(href) = href {
        format!(
            "<a href=\"{}\" download=\"{}\">{}</a>",
            escape_html(&href),
            escape_html(&att.filename),
            escape_html(&att.filename)
        )
    } else {
        format!(
            "<span class=\"filename\">{}</span>",
            escape_html(&att.filename)
        )
    };
    Ok(format!(
        "  <aside class=\"tes-attachment\" data-chunk-id=\"{}\" data-media-type=\"{}\" data-sha256=\"{}\">\n    {link}\n    <span class=\"media-type\">{}</span>{caption}\n  </aside>\n",
        entry.chunk_id,
        escape_html(&att.media_type),
        escape_html(&att.sha256),
        escape_html(&att.media_type),
    ))
}

/// Decode a single attachment chunk's opaque bytes for explicit download/export.
///
/// # Errors
///
/// Returns [`TesError::ChunkNotFound`], [`TesError::Decode`], or
/// [`TesError::InvalidAttachment`] when the chunk is missing or not an attachment.
pub fn export_attachment_bytes(file: &TesFile, chunk_id: u64) -> Result<AttachmentPayload> {
    let entry = file.chunk_by_id(chunk_id)?;
    if entry.chunk_type != ChunkType::Attachment {
        return Err(TesError::InvalidAttachment {
            message: format!(
                "chunk {chunk_id} is type '{}', expected attachment",
                entry.chunk_type.as_str()
            ),
        });
    }
    decode_attachment_entry(file, entry)
}

fn decode_slide_entry(
    file: &TesFile,
    entry: &ChunkIndexEntry,
) -> Result<crate::catalog::SlidePayload> {
    let raw = file.decode_payload(entry)?;
    crate::catalog::SlidePayload::from_bytes(raw.as_ref())
}

fn image_src(options: &ExportOptions, chunk_id: u64, media_type: &str, data: &[u8]) -> String {
    if let Some(prefix) = options.media_url_prefix.as_deref() {
        format!("{prefix}{chunk_id}")
    } else {
        format!("data:{media_type};base64,{}", base64_encode(data))
    }
}

/// Plain-prose citation line for AI exports (no markdown/HTML).
fn format_ai_cite_prose(cite: &CitePayload) -> String {
    if cite.quote.trim().is_empty() {
        format!(
            "[citation unresolved: {}]",
            cite.label.as_deref().unwrap_or("unknown")
        )
    } else if let Some(label) = cite.label.as_deref() {
        format!("{label} reported that {}", cite.quote.trim())
    } else {
        cite.quote.trim().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{DocumentCatalog, ListKind, TesWriterSession, TextHeader};
    use crate::layout::DocKind;
    use std::fs;
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
        assert_eq!(
            out,
            "Hello from Tessera — use tes textconv for readable diffs.\n"
        );
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
    fn chapter_filter_excludes_front_matter_and_other_chapters() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ms.tes");
        fs::write(
            &path,
            crate::fixtures::samples::encode_manuscript_chapters(),
        )
        .unwrap();

        let ch2 = export_view(
            &path,
            ExportView::Markdown,
            &ExportOptions {
                chapter: Some(2),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(ch2.contains("Chapter 2 — The Signal"));
        assert!(ch2.contains("lantern blinked"));
        assert!(!ch2.contains("Chapter 1"));
        assert!(!ch2.contains("Chapter 3"));
        assert!(!ch2.contains("Front matter"));
        assert!(!ch2.contains("beta readers"));

        let ch1 = export_view(
            &path,
            ExportView::Markdown,
            &ExportOptions {
                chapter: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(ch1.contains("Chapter 1 — The Quay"));
        assert!(ch1.contains("Scene: Warehouse"));
        assert!(!ch1.contains("Chapter 2"));

        let err = export_view(
            &path,
            ExportView::Raw,
            &ExportOptions {
                chapter: Some(9),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("chapter 9 not found"));
    }

    #[test]
    fn chapter_and_chunk_conflict() {
        let dir = tempdir().unwrap();
        let path = write_article(dir.path());
        let err = export_view(
            &path,
            ExportView::Raw,
            &ExportOptions {
                chunk_id: Some(1),
                chapter: Some(1),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("mutually exclusive"));
    }

    #[test]
    fn html_coalesces_ordered_lists_and_renders_mathml() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("lists_math.tes");
        let mut s = TesWriterSession::create(&path, DocKind::Document);
        s.set_catalog(DocumentCatalog::new(
            "990e8400-e29b-41d4-a716-446655440099",
            "Lists and math",
            "2026-07-30T00:00:00Z",
            "2026-07-30T00:00:00Z",
            DocKind::Document,
        ))
        .unwrap();
        s.add_text_chunk(&TextHeader::heading(1), "Open questions")
            .unwrap();
        s.add_text_chunk(&TextHeader::list_item(ListKind::Ordered), "First question")
            .unwrap();
        s.add_text_chunk(&TextHeader::list_item(ListKind::Ordered), "Second question")
            .unwrap();
        s.add_text_chunk(&TextHeader::list_item(ListKind::Ordered), "Third question")
            .unwrap();
        s.add_text_chunk(&TextHeader::math(), r"\Delta = \frac{a}{b}")
            .unwrap();
        s.commit().unwrap();

        let html = export_view(&path, ExportView::Html, &ExportOptions::default()).unwrap();
        assert!(
            html.contains("<ol data-list-depth=\"1\">"),
            "expected one ordered list, got:\n{html}"
        );
        assert_eq!(
            html.matches("<ol data-list-depth=\"1\">").count(),
            1,
            "ordered items must share one <ol>, got:\n{html}"
        );
        assert!(html.contains("First question"));
        assert!(html.contains("Second question"));
        assert!(html.contains("Third question"));
        assert!(
            html.contains("<math") || html.contains("math-fallback"),
            "expected MathML or fallback, got:\n{html}"
        );
        assert!(
            !html.contains("<ol data-list-depth=\"1\"><li data-chunk-id=\"2\""),
            "must not wrap each item in its own ol"
        );
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
        use crate::catalog::link::LinkKind;
        use crate::catalog::{CitePayload, TesFile};
        use crate::io::bib::{
            BibEntry, BibFormat, BibImportOptions, export_bibliography, import_bibliography,
        };

        let dir = tempdir().unwrap();
        let sample =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/assets/citations/sample.bib");
        let bib_tes = dir.path().join("from_bib.tes");
        import_bibliography(
            &sample,
            &bib_tes,
            BibFormat::Bibtex,
            &BibImportOptions::default(),
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

    #[test]
    fn deck_slides_export_html_regions() {
        use crate::catalog::SlidePayload;

        let dir = tempdir().unwrap();
        let path = dir.path().join("deck.tes");
        let mut s = TesWriterSession::create(&path, DocKind::Deck);
        s.set_catalog(DocumentCatalog::new(
            "880e8400-e29b-41d4-a716-446655440003",
            "Demo deck",
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Deck,
        ))
        .unwrap();
        s.add_text_chunk(&TextHeader::heading(1), "Hello slides")
            .unwrap();
        s.add_text_chunk(&TextHeader::paragraph(), "Region body copy.")
            .unwrap();
        s.add_slide(&SlidePayload::title_body(1, 2)).unwrap();
        s.commit().unwrap();

        let report = crate::verify::verify_tes_file(&path, true).unwrap();
        assert!(report.ok, "{:?}", report.findings);

        let html = export_view(
            &path,
            ExportView::Html,
            &ExportOptions {
                standalone: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(html.contains("class=\"deck\""));
        assert!(html.contains("data-layout=\"title_body\""));
        assert!(html.contains("data-region=\"title\""));
        assert!(html.contains("Hello slides"));
        assert!(html.contains("Region body copy."));
        assert!(!html.contains("<article"));
    }

    #[test]
    fn attachment_round_trip_verify_and_inert_export() {
        use crate::catalog::AttachmentPayload;
        use crate::edit::{EditWriteOptions, edit_read, edit_write};

        let dir = tempdir().unwrap();
        let path = dir.path().join("with_att.tes");
        let mut s = TesWriterSession::create(&path, DocKind::Note);
        s.set_catalog(DocumentCatalog::new(
            "990e8400-e29b-41d4-a716-446655440099",
            "Attachment specimen",
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Note,
        ))
        .unwrap();
        s.add_text_chunk(&TextHeader::paragraph(), "See the PDF.")
            .unwrap();
        let att = AttachmentPayload::new(
            "application/pdf",
            "notes.pdf",
            b"%PDF-1.4 tessera-fixture".to_vec(),
            Some("Lab notes".into()),
        )
        .unwrap();
        let att_id = s.add_attachment_chunk(&att).unwrap();
        s.commit().unwrap();

        let report = crate::verify::verify_tes_file(&path, true).unwrap();
        assert!(report.ok, "{:?}", report.findings);

        let file = TesFile::open(&path).unwrap();
        let exported = export_attachment_bytes(&file, att_id).unwrap();
        assert_eq!(exported.data, b"%PDF-1.4 tessera-fixture");
        assert_eq!(exported.filename, "notes.pdf");

        let linear = export_view(&path, ExportView::Linear, &ExportOptions::default()).unwrap();
        assert!(linear.contains("[attachment filename=notes.pdf"));
        assert!(!linear.contains("%PDF"));

        let html = export_view(
            &path,
            ExportView::Html,
            &ExportOptions {
                attachment_url_prefix: Some("/attachment/".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(html.contains("tes-attachment"));
        assert!(html.contains(&format!("href=\"/attachment/{att_id}\"")));
        assert!(html.contains("download=\"notes.pdf\""));
        assert!(!html.contains("data:application/pdf"));

        let read = edit_read(&path).unwrap();
        assert!(read.tessprek.contains("type=attachment"));
        assert!(read.tessprek.contains("filename=\"notes.pdf\""));
        edit_write(
            &path,
            &read.tessprek,
            &EditWriteOptions::new(read.source_hash.clone(), false),
        )
        .unwrap();
        let report2 = crate::verify::verify_tes_file(&path, true).unwrap();
        assert!(report2.ok, "{:?}", report2.findings);
    }
}
