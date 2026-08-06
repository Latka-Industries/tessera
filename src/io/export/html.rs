//! Semantic HTML5 / deck export (`--html`).

use std::fmt::Write as _;

use crate::catalog::chunk::{ListKind, TextHeader, TextRole};
use crate::catalog::file::TesFile;
use crate::catalog::index::{ChunkIndexEntry, ChunkType};
use crate::catalog::media::{ImagePayload, ImagePlacement};
use crate::catalog::{InlineKind, InlineSpan, LinkEntry};
use crate::error::{Result, TesError};
use crate::io::bib::{BibEntry, format_numeric_marker, format_reference_body};
use crate::io::cite::{self, CiteProj, CiteStyle, format_inline_cite};

use super::ExportOptions;
use super::common::{
    cite_number_map, decode_attachment_entry, decode_cite_entry, decode_figure_entry,
    decode_numbered_cite, decode_slide_entry, decode_text_entry, escape_html, html_class_attr,
    image_src, selected_content_entries,
};

pub(super) fn export_html(file: &TesFile, options: &ExportOptions) -> Result<String> {
    if options.chunk_id.is_none() && options.chapter.is_none() && file_has_slides(file) {
        return export_deck_html(file, options);
    }

    let entries = selected_content_entries(file, options)?;
    let cite_numbers = cite_number_map(file, &entries)?;
    let (cite_keys, style) = cite::projection_maps(file);
    let cite = CiteProj {
        numbers: &cite_numbers,
        keys: &cite_keys,
        style,
    };
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
                        Some(cite),
                    );
                } else {
                    close_all_lists(&mut article, &mut list_stack);
                    article.push_str(&render_text_chunk_html(
                        entry.chunk_id,
                        &header,
                        &body,
                        file.links(),
                        Some(cite),
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
                None,
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
            let image = ImagePayload::from_bytes(raw.as_ref())?;
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
    links: &[LinkEntry],
    cite: Option<CiteProj<'_>>,
) -> String {
    let inner = apply_spans_html(body, &header.spans, links, cite);
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
            append_list_item_html(&mut out, &mut stack, chunk_id, header, body, links, cite);
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
            let mut html = text_title_html(header.title.as_deref());
            let _ = writeln!(
                html,
                "  <pre data-chunk-id=\"{chunk_id}\"{class}><code{lang}>{escaped}</code></pre>"
            );
            html.push_str(&text_caption_html(header.caption.as_deref()));
            html
        }
        TextRole::Table => {
            let mut html = text_title_html(header.title.as_deref());
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
                let _ = writeln!(
                    html,
                    "  <table data-chunk-id=\"{chunk_id}\"{class}><tbody>{rows}</tbody></table>"
                );
            } else {
                let rows = body.lines().fold(String::new(), |mut acc, line| {
                    let cells = line.split('\t').fold(String::new(), |mut acc, cell| {
                        let _ = write!(acc, "<td>{}</td>", escape_html(cell));
                        acc
                    });
                    let _ = write!(acc, "<tr>{cells}</tr>");
                    acc
                });
                let _ = writeln!(
                    html,
                    "  <table data-chunk-id=\"{chunk_id}\"{class}><tbody>{rows}</tbody></table>"
                );
            }
            html.push_str(&text_caption_html(header.caption.as_deref()));
            html
        }
        TextRole::Math => {
            let mut html = text_title_html(header.title.as_deref());
            html.push_str(&render_math_html(chunk_id, body, &class, true));
            html.push_str(&text_caption_html(header.caption.as_deref()));
            html
        }
    }
}

fn text_title_html(title: Option<&str>) -> String {
    title
        .filter(|s| !s.is_empty())
        .map(|t| format!("  <p class=\"tes-title\">{}</p>\n", escape_html(t)))
        .unwrap_or_default()
}

fn text_caption_html(caption: Option<&str>) -> String {
    caption
        .filter(|s| !s.is_empty())
        .map(|c| format!("  <p class=\"tes-caption\">{}</p>\n", escape_html(c)))
        .unwrap_or_default()
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
    links: &[LinkEntry],
    cite: Option<CiteProj<'_>>,
) {
    let kind = header.list_kind.unwrap_or(ListKind::Bullet);
    let depth = header.list_depth_or_default();
    let inner = apply_spans_html(body, &header.spans, links, cite);
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
    spans: &[InlineSpan],
    links: &[LinkEntry],
    cite: Option<CiteProj<'_>>,
) -> String {
    if spans.is_empty() {
        return escape_html(body);
    }

    // Replace from the end so earlier byte offsets stay valid (non-overlapping spans).
    let mut ordered: Vec<&InlineSpan> = spans.iter().collect();
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
            InlineKind::Citation { cite_chunk_id } => {
                let n = cite.map_or(0, |c| c.number(*cite_chunk_id));
                let marker = cite.map_or_else(
                    || format_inline_cite(CiteStyle::Numeric, 0, &format!("chunk-{cite_chunk_id}")),
                    |c| c.marker(*cite_chunk_id),
                );
                if n > 0 {
                    format!(
                        "<a href=\"#ref-{n}\"><cite>{}</cite></a>",
                        escape_html(&marker)
                    )
                } else {
                    format!("<cite>{}</cite>", escape_html(&marker))
                }
            }
            InlineKind::Font { font_id } => format!(
                "<span class=\"font\" data-font=\"{}\">{}</span>",
                escape_html(font_id),
                escape_html(&inner)
            ),
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
    use crate::io::cite::{CiteTessprekKind, classify_cite};

    let cite = decode_cite_entry(file, entry)?;
    match classify_cite(&cite) {
        CiteTessprekKind::Biblio => {
            let (n, cite, bib) = decode_numbered_cite(file, entry, cite_numbers)?;
            let label = cite
                .label
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or(bib.cite_key.as_str());
            let label = if label.is_empty() { "unknown" } else { label };
            let marker = format_numeric_marker(n);
            let attrs = format!("data-chunk-id=\"{}\" class=\"citation\"", entry.chunk_id);
            let _ = writeln!(
                article,
                "  <p {attrs}><a href=\"#ref-{n}\"><cite>{marker}</cite></a> <span class=\"cite-label\">{}</span></p>",
                escape_html(label)
            );
            bib_items.push((n, bib));
        }
        CiteTessprekKind::Quote => {
            let mut attrs = format!("data-chunk-id=\"{}\" class=\"quote\"", entry.chunk_id);
            write_target_data_attrs(&mut attrs, &cite);
            let _ = writeln!(
                article,
                "  <blockquote {attrs}>{}</blockquote>",
                escape_html(cite.quote.trim())
            );
        }
        CiteTessprekKind::Ref => {
            let mut attrs = format!("data-chunk-id=\"{}\" class=\"ref\"", entry.chunk_id);
            write_target_data_attrs(&mut attrs, &cite);
            let label = cite
                .label
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or("ref");
            let _ = writeln!(
                article,
                "  <p {attrs}><span class=\"ref-label\">{}</span></p>",
                escape_html(label)
            );
        }
    }
    Ok(())
}

fn write_target_data_attrs(attrs: &mut String, cite: &crate::catalog::chunk::CitePayload) {
    if let Some(doc) = cite.target_doc_id.as_deref() {
        let _ = write!(attrs, " data-target-doc=\"{}\"", escape_html(doc));
    }
    if let Some(chunk) = cite.target_chunk_id {
        let _ = write!(attrs, " data-target-chunk=\"{chunk}\"");
    }
    if let Some(start) = cite.target_byte_start {
        let _ = write!(attrs, " data-byte-start=\"{start}\"");
    }
    if let Some(end) = cite.target_byte_end {
        let _ = write!(attrs, " data-byte-end=\"{end}\"");
    }
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
        ImagePlacement::Region { name } => {
            format!(" data-region=\"{}\"", escape_html(name))
        }
        _ => String::new(),
    };

    let mut html = format!(
        "  <figure data-chunk-id=\"{}\" data-image-chunk=\"{}\" data-placement=\"{}\"{region}>\n",
        entry.chunk_id,
        figure.image_chunk_id,
        figure.placement.as_str(),
    );
    if let Some(title) = figure.title.as_deref().filter(|s| !s.is_empty()) {
        let _ = writeln!(
            html,
            "    <p class=\"tes-title\">{}</p>",
            escape_html(title)
        );
    }
    let _ = writeln!(
        html,
        "    <img src=\"{}\" alt=\"{}\"{dims}>",
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
