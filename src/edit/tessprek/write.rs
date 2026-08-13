use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::catalog::chunk::{OrderedListNumbering, TextHeader, TextRole};
use crate::catalog::layout::LayoutPayload;
use crate::catalog::media::{AttachmentPayload, FigureRef, ImagePlacement};
use crate::catalog::slide::SlidePayload;
use crate::catalog::{CitePayload, InlineKind, InlineSpan, LinkEntry, LinkKind, OutboundLink};
use crate::io::cite::{
    CiteTessprekKind, PendingCite, cite_key_from_payload, cite_key_or_fallback, classify_cite,
};
use crate::io::font::PendingFont;

use super::super::ContentBlock;
use super::layout_ops::layout_op_parts;
use super::markers::{
    ATTACH_PREFIX, BLOCK_PREFIX, BRACE_SUFFIX, CITE_PREFIX, COLUMNS_PREFIX, FIGURE_PREFIX, FORMAT,
    IDS_PREFIX, LAYOUT_PREFIX, MEDIA_PREFIX, QUOTE_PREFIX, REF_PREFIX, SLIDE_PREFIX,
    TESSERA_PREFIX, TOC_PREFIX, LOF_PREFIX, LOT_PREFIX, VERSION,
};
use super::types::{TessprekDocMeta, TessprekMediaEntry};
use super::util::{kv_attr, quoted_attr};

/// Encode typed content blocks as Tessprek v2.
///
/// `meta` supplies `source-hash` and optional catalog identity fields for the
/// `\tessera{…}` header (encode prefers a multiline block). `links` resolves
/// `InlineKind::Link` spans on blocks whose `pending_links` is empty (e.g.
/// blocks freshly decoded from a `.tes` file); pass `&[]` when blocks already
/// carry `pending_links` (normalize / typed ops).
///
/// Used by [`super::format::normalize_tessprek`], [`super::encode_tessprek`], and tests.
///
/// `media` supplies `\media{…}` rows (mime / sha256 / dimensions). When empty,
/// figure-referenced ids are still emitted as bare `id=N` rows.
#[must_use]
pub fn encode_content_blocks(
    meta: &TessprekDocMeta,
    blocks: &[ContentBlock],
    links: &[LinkEntry],
    media: &[TessprekMediaEntry],
) -> String {
    let cite_keys = cite_keys_from_blocks(blocks);
    let mut out = String::new();
    write_header(&mut out, meta);
    write_ids(&mut out, blocks);
    write_media(&mut out, blocks, media);
    out.push('\n');

    let mut ordered = OrderedListNumbering::default();
    for (i, block) in blocks.iter().enumerate() {
        let next = blocks.get(i + 1);
        match block {
            ContentBlock::Text {
                header,
                body,
                pending_links,
                pending_cites,
                pending_fonts,
                ..
            } => {
                if header.role == TextRole::Toc {
                    write_toc_directive(&mut out, header);
                    out.push('\n');
                    continue;
                }
                if header.role == TextRole::Lof {
                    write_float_list_directive(&mut out, header, "lof", LOF_PREFIX);
                    out.push('\n');
                    continue;
                }
                if header.role == TextRole::Lot {
                    write_float_list_directive(&mut out, header, "lot", LOT_PREFIX);
                    out.push('\n');
                    continue;
                }
                if header.role == TextRole::Columns {
                    write_columns_directive(&mut out, header);
                    out.push('\n');
                    continue;
                }
                if header.role == TextRole::ColumnsEnd {
                    let _ = writeln!(out, "\\endcolumns");
                    out.push('\n');
                    continue;
                }
                let ordered_index = ordered.take_for_text(header);
                // One `\block{indent=N}` per list run — not before every item.
                let mut attr_header = header.clone();
                if header.role == TextRole::ListItem && i > 0 && blocks[i - 1].is_list_item() {
                    attr_header.indent = None;
                }
                write_block_directive(&mut out, &attr_header);
                out.push_str(
                    render_text_body(
                        header,
                        body,
                        pending_links,
                        pending_cites,
                        pending_fonts,
                        links,
                        &cite_keys,
                        ordered_index,
                    )
                    .trim_end(),
                );
                // Tight lists: consecutive list items share a single newline
                // (CommonMark). Blank line separates other blocks / list runs.
                out.push_str(
                    if block.is_list_item() && next.is_some_and(ContentBlock::is_list_item) {
                        "\n"
                    } else {
                        "\n\n"
                    },
                );
            }
            other => {
                ordered.clear();
                match other {
                    ContentBlock::Figure { figure, .. } => {
                        write_figure_directive(&mut out, figure);
                        out.push('\n');
                    }
                    ContentBlock::Cite { cite, .. } => {
                        write_cite_family(&mut out, cite);
                        out.push('\n');
                    }
                    ContentBlock::Slide { slide, .. } => {
                        write_slide_directive(&mut out, slide);
                        out.push('\n');
                    }
                    ContentBlock::Layout { layout, .. } => {
                        write_layout_directive(&mut out, layout);
                        out.push('\n');
                    }
                    ContentBlock::Attachment {
                        chunk_id,
                        filename,
                        media_type,
                        caption,
                        sha256,
                    } => {
                        write_attachment_directive(
                            &mut out,
                            *chunk_id,
                            &AttachmentPayload {
                                filename: filename.clone(),
                                media_type: media_type.clone(),
                                caption: caption.clone(),
                                sha256: sha256.clone(),
                                // Bytes are not projected in Tessprek.
                                data: Vec::new(),
                            },
                        );
                        out.push('\n');
                    }
                    ContentBlock::Text { .. } => unreachable!("text handled above"),
                }
            }
        }
    }
    out
}

fn cite_keys_from_blocks(blocks: &[ContentBlock]) -> std::collections::BTreeMap<u64, String> {
    let mut map = std::collections::BTreeMap::new();
    for block in blocks {
        if let ContentBlock::Cite {
            chunk_id: Some(id),
            cite,
        } = block
            && let Some(key) = cite_key_from_payload(cite)
        {
            map.insert(*id, key);
        }
    }
    map
}

fn write_brace_line(out: &mut String, prefix: &str, parts: &[String]) {
    let _ = writeln!(out, "{prefix}{}{BRACE_SUFFIX}", parts.join(" "));
}

/// Prefer multiline brace blocks (same shape as `\tessera{…}`) for readability.
fn write_brace_block(out: &mut String, prefix: &str, parts: &[String]) {
    if parts.is_empty() {
        let _ = writeln!(out, "{prefix}{BRACE_SUFFIX}");
        return;
    }
    let _ = writeln!(out, "{prefix}");
    for part in parts {
        let _ = writeln!(out, "  {part}");
    }
    let _ = writeln!(out, "{BRACE_SUFFIX}");
}

fn write_header(out: &mut String, meta: &TessprekDocMeta) {
    let mut parts = vec![format!("format={FORMAT}"), format!("version={VERSION}")];
    meta.push_parts(&mut parts);
    // Multiline so long identity keys stay readable (single-line still accepted).
    let _ = writeln!(out, "{TESSERA_PREFIX}");
    for part in &parts {
        let _ = writeln!(out, "  {part}");
    }
    let _ = writeln!(out, "{BRACE_SUFFIX}");
}

fn write_ids(out: &mut String, blocks: &[ContentBlock]) {
    let ids = blocks
        .iter()
        .map(|b| b.chunk_id().unwrap_or(0).to_string())
        .collect::<Vec<_>>()
        .join(",");
    write_brace_line(out, IDS_PREFIX, &[ids]);
}

/// Emit `\media{…}` for image payloads referenced by figures (sorted by id).
///
/// One attr per line; blank line between payloads when there are several.
/// Omitted when the document has no figures. Prefer rows from `media`; fill
/// missing figure targets as bare `id=N`. Regenerated on every encode.
fn write_media(out: &mut String, blocks: &[ContentBlock], media: &[TessprekMediaEntry]) {
    let mut by_id: BTreeMap<u64, TessprekMediaEntry> = BTreeMap::new();
    for entry in media {
        if entry.chunk_id != 0 {
            by_id.insert(entry.chunk_id, entry.clone());
        }
    }
    for block in blocks {
        if let ContentBlock::Figure { figure, .. } = block {
            let id = figure.image_chunk_id;
            if id != 0 {
                by_id.entry(id).or_insert(TessprekMediaEntry {
                    chunk_id: id,
                    ..TessprekMediaEntry::default()
                });
            }
        }
    }
    if by_id.is_empty() {
        return;
    }
    let _ = writeln!(out, "{MEDIA_PREFIX}");
    let mut first = true;
    for entry in by_id.values() {
        if !first {
            out.push('\n');
        }
        first = false;
        for part in entry.attr_parts() {
            let _ = writeln!(out, "  {part}");
        }
    }
    let _ = writeln!(out, "{BRACE_SUFFIX}");
}

#[allow(clippy::too_many_arguments)]
fn render_text_body(
    header: &TextHeader,
    body: &str,
    pending_links: &[OutboundLink],
    pending_cites: &[PendingCite],
    pending_fonts: &[PendingFont],
    links: &[LinkEntry],
    cite_keys: &std::collections::BTreeMap<u64, String>,
    ordered_index: Option<u32>,
) -> String {
    let mut header = header.clone();
    let mut body = body.to_owned();

    // Prefer sealed Citation spans; else project pending_cites from Tessprek parse.
    if header
        .spans
        .iter()
        .any(|s| matches!(s.kind, InlineKind::Citation { .. }))
    {
        let replacements: Vec<_> = header
            .spans
            .iter()
            .filter_map(|s| match &s.kind {
                InlineKind::Citation { cite_chunk_id } => {
                    let key = cite_key_or_fallback(cite_keys, *cite_chunk_id);
                    Some((s.start, s.end, format!("\\cite{{{key}}}")))
                }
                _ => None,
            })
            .collect();
        rewrite_ranges_rev(&mut body, replacements);
        header
            .spans
            .retain(|s| !matches!(s.kind, InlineKind::Citation { .. }));
    } else if !pending_cites.is_empty() {
        let replacements = pending_cites
            .iter()
            .map(|c| (c.start, c.end, format!("\\cite{{{}}}", c.key)))
            .collect();
        rewrite_ranges_rev(&mut body, replacements);
    }

    // Pending fonts (pre-seal): project macros. Sealed Font spans go through
    // `apply_spans_markdown` as `\font{id}{…}`.
    if !header
        .spans
        .iter()
        .any(|s| matches!(s.kind, InlineKind::Font { .. }))
        && !pending_fonts.is_empty()
    {
        let mut fonts: Vec<_> = pending_fonts.iter().collect();
        fonts.sort_by_key(|f| std::cmp::Reverse(f.start));
        for font in fonts {
            let Some((start, end)) = byte_range(&body, font.start, font.end) else {
                continue;
            };
            let inner = body[start..end].to_owned();
            let replacement = if let Some(name) =
                crate::catalog::icon_name_for_face_glyph(&font.font_id, &inner)
            {
                format!("\\icon{{{name}}}")
            } else {
                format!("\\font{{{}}}{{{inner}}}", font.font_id)
            };
            body.replace_range(start..end, &replacement);
        }
    }

    if pending_links.is_empty() {
        return header.render_markdown_with_links_indexed(&body, links, ordered_index);
    }

    let mut synthetic_links = Vec::new();
    for link in pending_links {
        let link_id = u64::try_from(synthetic_links.len()).unwrap_or(u64::MAX);
        if let Ok(entry) = link.clone().into_entry(0, LinkKind::Wiki) {
            synthetic_links.push(entry);
            header.spans.push(InlineSpan {
                start: link.start,
                end: link.end,
                kind: InlineKind::Link { link_id },
            });
        }
    }
    header.render_markdown_with_links_indexed(&body, &synthetic_links, ordered_index)
}

/// Write `\block{title=… caption=… class=… …}` when the header carries attrs
/// that cannot live in plain Markdown. Emits nothing otherwise.
fn write_block_directive(out: &mut String, header: &TextHeader) {
    if header.classes.is_empty()
        && header.lang.is_none()
        && header.align.is_none()
        && header.indent.is_none()
        && header.title.is_none()
        && header.caption.is_none()
    {
        return;
    }
    let mut parts = Vec::new();
    if let Some(title) = header.title.as_deref() {
        parts.push(quoted_attr("title", title));
    }
    if let Some(caption) = header.caption.as_deref() {
        parts.push(quoted_attr("caption", caption));
    }
    if !header.classes.is_empty() {
        parts.push(format!("class=\"{}\"", header.classes.join(" ")));
    }
    if let Some(lang) = header.lang.as_deref() {
        parts.push(kv_attr("lang", lang));
    }
    if let Some(align) = header.align {
        parts.push(format!("align={}", align.as_str()));
    }
    if let Some(indent) = header.indent.filter(|&n| n > 0) {
        parts.push(format!("indent={indent}"));
    }
    write_brace_block(out, BLOCK_PREFIX, &parts);
}

fn write_toc_directive(out: &mut String, header: &TextHeader) {
    let mut parts = Vec::new();
    if let Some(title) = header.title.as_deref().filter(|s| !s.is_empty()) {
        parts.push(quoted_attr("title", title));
    }
    if let Some(depth) = header.toc_depth {
        parts.push(format!("depth={depth}"));
    }
    match header.toc_pages {
        Some(false) => parts.push("page_numbers=false".into()),
        Some(true) => parts.push("page_numbers=true".into()),
        None => {}
    }
    match header.toc_sections {
        Some(false) => parts.push("section_numbers=false".into()),
        Some(true) => parts.push("section_numbers=true".into()),
        None => {}
    }
    match header.toc_leaders {
        Some(false) => parts.push("leaders=false".into()),
        Some(true) => parts.push("leaders=true".into()),
        None => {}
    }
    if parts.is_empty() {
        let _ = writeln!(out, "\\toc");
    } else {
        write_brace_block(out, TOC_PREFIX, &parts);
    }
}

fn write_float_list_directive(
    out: &mut String,
    header: &TextHeader,
    bare: &str,
    prefix: &str,
) {
    let mut parts = Vec::new();
    if let Some(title) = header.title.as_deref().filter(|s| !s.is_empty()) {
        parts.push(quoted_attr("title", title));
    }
    match header.toc_pages {
        Some(false) => parts.push("page_numbers=false".into()),
        Some(true) => parts.push("page_numbers=true".into()),
        None => {}
    }
    match header.toc_leaders {
        Some(false) => parts.push("leaders=false".into()),
        Some(true) => parts.push("leaders=true".into()),
        None => {}
    }
    if let Some(source) = header.float_list_source {
        parts.push(format!("source={}", source.as_str()));
    }
    if parts.is_empty() {
        let _ = writeln!(out, "\\{bare}");
    } else {
        write_brace_block(out, prefix, &parts);
    }
}

fn write_columns_directive(out: &mut String, header: &TextHeader) {
    let mut parts = Vec::new();
    if let Some(n) = header.columns_count {
        parts.push(format!("n={n}"));
    }
    if let Some(gap) = header.columns_gap {
        parts.push(format!("gap={gap}"));
    }
    if parts.is_empty() {
        let _ = writeln!(out, "\\columns");
    } else {
        write_brace_block(out, COLUMNS_PREFIX, &parts);
    }
}

fn write_figure_directive(out: &mut String, figure: &FigureRef) {
    let mut parts = vec![
        format!("image={}", figure.image_chunk_id),
        format!("placement={}", figure.placement.as_str()),
        quoted_attr("alt", &figure.alt_text),
    ];
    if let ImagePlacement::Region { name } = &figure.placement {
        parts.push(quoted_attr("region", name));
    }
    if let Some(title) = figure.title.as_deref() {
        parts.push(quoted_attr("title", title));
    }
    if let Some(caption) = figure.caption.as_deref() {
        parts.push(quoted_attr("caption", caption));
    }
    write_brace_block(out, FIGURE_PREFIX, &parts);
}

fn write_cite_family(out: &mut String, cite: &CitePayload) {
    match classify_cite(cite) {
        CiteTessprekKind::Biblio => write_brace_block(out, CITE_PREFIX, &biblio_attr_parts(cite)),
        CiteTessprekKind::Quote => write_brace_block(out, QUOTE_PREFIX, &quote_attr_parts(cite)),
        CiteTessprekKind::Ref => write_brace_block(out, REF_PREFIX, &target_attr_parts(cite)),
    }
}

fn biblio_attr_parts(cite: &CitePayload) -> Vec<String> {
    let mut parts = Vec::new();
    if let Some(label) = cite.label.as_deref() {
        parts.push(kv_attr("label", label));
    } else if let Some(key) = cite.source.as_ref().map(|s| s.cite_key.as_str()) {
        parts.push(kv_attr("label", key));
    }
    if let Some(source) = &cite.source {
        if source.entry_type != "misc" && !source.entry_type.is_empty() {
            parts.push(kv_attr("entry_type", &source.entry_type));
        }
        push_opt_quoted(&mut parts, "author", source.author.as_deref());
        push_opt_quoted(&mut parts, "title", source.title.as_deref());
        push_opt_quoted(&mut parts, "journal", source.journal.as_deref());
        push_opt(&mut parts, "year", source.year.as_deref());
        push_opt(&mut parts, "volume", source.volume.as_deref());
        push_opt(&mut parts, "number", source.number.as_deref());
        push_opt_quoted(&mut parts, "pages", source.pages.as_deref());
        push_opt(&mut parts, "doi", source.doi.as_deref());
        push_opt_quoted(&mut parts, "publisher", source.publisher.as_deref());
        push_opt_quoted(&mut parts, "note", source.note.as_deref());
        push_opt_quoted(&mut parts, "howpublished", source.howpublished.as_deref());
        push_opt_quoted(&mut parts, "url", source.url.as_deref());
    }
    if let Some(page) = cite.page {
        parts.push(format!("page={page}"));
    }
    parts
}

fn quote_attr_parts(cite: &CitePayload) -> Vec<String> {
    let mut parts = target_attr_parts(cite);
    if !cite.quote.is_empty() {
        parts.push(quoted_attr("quote", &cite.quote.replace('\n', "\\n")));
    }
    parts
}

fn target_attr_parts(cite: &CitePayload) -> Vec<String> {
    let mut parts = Vec::new();
    if let Some(label) = cite.label.as_deref() {
        parts.push(kv_attr("label", label));
    }
    if let Some(doc) = cite.target_doc_id.as_deref() {
        parts.push(format!("target_doc={doc}"));
    }
    if let Some(chunk) = cite.target_chunk_id {
        parts.push(format!("target_chunk={chunk}"));
    }
    if let Some(start) = cite.target_byte_start {
        parts.push(format!("target_byte_start={start}"));
    }
    if let Some(end) = cite.target_byte_end {
        parts.push(format!("target_byte_end={end}"));
    }
    if let Some(page) = cite.page {
        parts.push(format!("page={page}"));
    }
    parts
}

fn push_opt(parts: &mut Vec<String>, key: &str, value: Option<&str>) {
    if let Some(v) = value.filter(|s| !s.is_empty()) {
        parts.push(kv_attr(key, v));
    }
}

fn push_opt_quoted(parts: &mut Vec<String>, key: &str, value: Option<&str>) {
    if let Some(v) = value.filter(|s| !s.is_empty()) {
        parts.push(quoted_attr(key, v));
    }
}

fn write_slide_directive(out: &mut String, slide: &SlidePayload) {
    let regions = slide
        .regions
        .iter()
        .map(|r| format!("{}:{}", r.name, r.chunk_id))
        .collect::<Vec<_>>()
        .join(",");
    write_brace_block(
        out,
        SLIDE_PREFIX,
        &[
            kv_attr("layout", &slide.layout_id),
            quoted_attr("regions", &regions),
        ],
    );
}

fn write_layout_directive(out: &mut String, layout: &LayoutPayload) {
    let parts = layout_op_parts(layout);
    write_brace_block(out, LAYOUT_PREFIX, &parts);
}

fn write_attachment_directive(out: &mut String, chunk_id: Option<u64>, att: &AttachmentPayload) {
    let mut parts = Vec::new();
    if let Some(id) = chunk_id.filter(|&id| id > 0) {
        parts.push(format!("chunk={id}"));
    }
    parts.push(quoted_attr("filename", &att.filename));
    parts.push(kv_attr("media_type", &att.media_type));
    parts.push(format!("sha256={}", att.sha256));
    if let Some(caption) = att.caption.as_deref() {
        parts.push(quoted_attr("caption", caption));
    }
    write_brace_block(out, ATTACH_PREFIX, &parts);
}

/// Apply `(start, end, replacement)` edits from the end of the string forward.
fn rewrite_ranges_rev(body: &mut String, mut replacements: Vec<(u32, u32, String)>) {
    replacements.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    for (start, end, replacement) in replacements {
        let Some((start, end)) = byte_range(body, start, end) else {
            continue;
        };
        body.replace_range(start..end, &replacement);
    }
}

fn byte_range(body: &str, start: u32, end: u32) -> Option<(usize, usize)> {
    let start = start as usize;
    let end = end as usize;
    if end > body.len() || start > end {
        None
    } else {
        Some((start, end))
    }
}
