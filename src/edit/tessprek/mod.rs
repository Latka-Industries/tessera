//! Tessera Markdown (Tessprek) encode/decode for virtual editor buffers.
//!
//! Format (v1): HTML-comment directives carry typed fields; bodies are Markdown.

mod format;

use std::fmt::Write as _;

use crate::catalog::TesFile;
use crate::catalog::chunk::{
    CitePayload, ListKind, TableCell, TableData, TableRow, TextAlign, TextHeader, TextRole,
    decode_text_payload,
};
use crate::catalog::index::ChunkType;
use crate::catalog::media::{AttachmentPayload, FigureRef, ImagePlacement};
use crate::catalog::slide::{SlidePayload, SlideRegion};
use crate::error::{Result, TesError};

use super::ContentBlock;

pub use format::{normalize_tessprek, tessprek_needs_format};

/// Tessprek HTML-comment wire markers (v1). Shared by encode/decode and LSP hover.
pub mod markers {
    /// Document header: `<!-- tessera: format=… -->`.
    pub const HEADER_PREFIX: &str = "<!-- tessera:";
    /// Chunk directive: `<!-- tes chunk=… -->`.
    pub const CHUNK_PREFIX: &str = "<!-- tes ";
    /// Closing delimiter for both header and chunk comments.
    pub const COMMENT_SUFFIX: &str = " -->";
    /// `format=` value stamped in the document header.
    pub const FORMAT: &str = "tessprek";
    /// `version=` value stamped in the document header.
    pub const VERSION: &str = "1";
}

use markers::{CHUNK_PREFIX, COMMENT_SUFFIX, FORMAT, HEADER_PREFIX, VERSION};

/// Encode a `.tes` file as Tessera Markdown, embedding `source_hash`.
///
/// # Errors
///
/// Returns decode errors for reading-order text/figure/cite payloads.
pub fn encode_tessprek(file: &TesFile, source_hash: &str) -> Result<String> {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{HEADER_PREFIX} format={FORMAT} version={VERSION} source-hash={source_hash}{COMMENT_SUFFIX}"
    );
    out.push('\n');

    for entry in file.reading_order_chunks() {
        match entry.chunk_type {
            ChunkType::Text => {
                let raw = file.decode_payload(entry)?;
                let (header, body) = decode_text_payload(raw.as_ref())?;
                write_text_directive(&mut out, entry.chunk_id, &header);
                out.push_str(
                    header
                        .render_markdown_with_links(&body, file.links())
                        .trim_end(),
                );
                out.push_str("\n\n");
            }
            ChunkType::Figure => {
                let raw = file.decode_payload(entry)?;
                let figure = FigureRef::from_bytes(raw.as_ref())?;
                write_figure_directive(&mut out, entry.chunk_id, &figure);
                let _ = writeln!(
                    out,
                    "![{}](media:chunk-{})",
                    escape_alt(&figure.alt_text),
                    figure.image_chunk_id
                );
                out.push('\n');
            }
            ChunkType::Cite => {
                let raw = file.decode_payload(entry)?;
                let cite = CitePayload::from_bytes(raw.as_ref())?;
                write_cite_directive(&mut out, entry.chunk_id, &cite);
                out.push_str(cite.quote.trim_end());
                out.push_str("\n\n");
            }
            ChunkType::Slide => {
                let raw = file.decode_payload(entry)?;
                let slide = SlidePayload::from_bytes(raw.as_ref())?;
                write_slide_directive(&mut out, entry.chunk_id, &slide);
                out.push('\n');
            }
            ChunkType::Attachment => {
                let raw = file.decode_payload(entry)?;
                let att = AttachmentPayload::from_bytes(raw.as_ref())?;
                write_attachment_directive(&mut out, entry.chunk_id, &att);
                out.push('\n');
            }
            _ => {}
        }
    }
    Ok(out)
}

/// Parse Tessera Markdown into typed content blocks.
///
/// # Errors
///
/// Returns [`TesError::EditParse`] with line/column on malformed directives or bodies.
pub fn decode_tessprek(input: &str) -> Result<Vec<ContentBlock>> {
    let lines: Vec<&str> = input.lines().collect();
    let mut blocks = Vec::new();
    let mut i = skip_header_and_blanks(&lines, 0);

    while i < lines.len() {
        let line_no = i + 1;
        let trimmed = lines[i].trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }
        if !trimmed.starts_with(CHUNK_PREFIX) || !trimmed.ends_with(COMMENT_SUFFIX) {
            return Err(parse_err(
                line_no,
                1,
                format!("expected `{CHUNK_PREFIX}...{COMMENT_SUFFIX}` directive, found: {trimmed}"),
            ));
        }
        let attrs = &trimmed[CHUNK_PREFIX.len()..trimmed.len() - COMMENT_SUFFIX.len()];
        let map = parse_attrs(attrs, line_no)?;
        i += 1;

        let chunk_id = required_u64(&map, "chunk", line_no)?;
        let kind = map
            .get("type")
            .map(String::as_str)
            .or_else(|| map.get("role").map(|_| "text"))
            .unwrap_or("text");

        let body_start = i;
        i = next_directive_index(&lines, i);
        let body = trim_block_body(&lines[body_start..i]);
        blocks.push(decode_directive_block(
            kind, chunk_id, &map, &body, line_no,
        )?);
    }

    Ok(blocks)
}

fn skip_header_and_blanks(lines: &[&str], mut i: usize) -> usize {
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.is_empty() || trimmed.starts_with(HEADER_PREFIX) {
            i += 1;
            continue;
        }
        break;
    }
    i
}

fn next_directive_index(lines: &[&str], mut i: usize) -> usize {
    while i < lines.len() {
        let t = lines[i].trim();
        if t.starts_with(CHUNK_PREFIX) && t.ends_with(COMMENT_SUFFIX) {
            break;
        }
        i += 1;
    }
    i
}

pub(super) fn decode_directive_block(
    kind: &str,
    chunk_id: u64,
    map: &std::collections::BTreeMap<String, String>,
    body: &str,
    line_no: usize,
) -> Result<ContentBlock> {
    match kind {
        "text" | "paragraph" | "heading" | "list_item" | "blockquote" | "code_block" | "table"
        | "math" => decode_text_block(kind, chunk_id, map, body, line_no),
        "figure" => decode_figure_block(chunk_id, map, body, line_no),
        "cite" => Ok(decode_cite_block(chunk_id, map, body)),
        "slide" => decode_slide_block(chunk_id, map, line_no),
        "attachment" => decode_attachment_block(chunk_id, map, line_no),
        other => Err(parse_err(
            line_no,
            1,
            format!("unknown tes directive type '{other}'"),
        )),
    }
}

fn decode_text_block(
    kind: &str,
    chunk_id: u64,
    map: &std::collections::BTreeMap<String, String>,
    body: &str,
    line_no: usize,
) -> Result<ContentBlock> {
    let role = parse_role(map.get("role").map_or(kind, String::as_str), line_no)?;
    let mut header = TextHeader::with_role(role);
    header.classes = parse_classes(map.get("class").map(String::as_str));
    if let Some(lang) = map.get("lang").filter(|s| !s.is_empty()) {
        header.lang = Some(lang.clone());
    }
    if let Some(align) = map.get("align") {
        header.align =
            Some(TextAlign::from_name(align).map_err(|e| parse_err(line_no, 1, format!("{e}")))?);
    }
    if role == TextRole::Heading {
        header.level = Some(required_u32(map, "level", line_no)?.clamp(1, 6));
    }
    if role == TextRole::ListItem {
        header.list_kind = Some(parse_list_kind(
            map.get("list").map_or("bullet", String::as_str),
            line_no,
        )?);
        if let Some(raw) = map.get("depth") {
            let depth = raw
                .parse::<u32>()
                .map_err(|_| parse_err(line_no, 1, format!("invalid list depth '{raw}'")))?;
            if !(1..=16).contains(&depth) {
                return Err(parse_err(
                    line_no,
                    1,
                    format!("list depth {depth} must be 1..=16"),
                ));
            }
            if depth > 1 {
                header.list_depth = Some(depth);
            }
        } else {
            // Infer from leading indent before the list marker (2 spaces per nest).
            let lead = body.len() - body.trim_start().len();
            let inferred = u32::try_from(lead / 2).unwrap_or(0).saturating_add(1);
            if inferred > 1 {
                header.list_depth = Some(inferred.min(16));
            }
        }
    }
    if role == TextRole::CodeBlock
        && let Some(lang) = map.get("code_lang").or_else(|| map.get("fence"))
        && !lang.is_empty()
    {
        header.code_lang = Some(lang.clone());
    }
    let text_body = strip_markdown_wrapper(&header, body);
    if role == TextRole::CodeBlock
        && header.code_lang.is_none()
        && let Some(lang) = fence_lang(body)
    {
        header.code_lang = Some(lang);
    }
    if role == TextRole::Table
        && let Some(table) = parse_markdown_table(body)
    {
        header.table = Some(table);
    }
    let clear_body = header.table.is_some();
    let (body, pending_links) =
        if clear_body || role == TextRole::CodeBlock || role == TextRole::Math {
            (
                if clear_body { String::new() } else { text_body },
                Vec::new(),
            )
        } else {
            // Re-parse inline markdown so `[text](https://…)` becomes TLNK on write.
            let parsed = crate::io::import::parse_markdown_blocks(&text_body);
            if parsed.len() == 1 {
                let block = &parsed[0];
                // Preserve role/header fields; take body + links + math spans from parse.
                let mut merged = header;
                for span in &block.header.spans {
                    if !merged.spans.iter().any(|s| s == span) {
                        merged.spans.push(span.clone());
                    }
                }
                header = merged;
                (block.body.clone(), block.pending_links.clone())
            } else {
                (text_body, Vec::new())
            }
        };
    Ok(ContentBlock::Text {
        chunk_id: Some(chunk_id),
        header,
        body,
        pending_links,
    })
}

fn decode_figure_block(
    chunk_id: u64,
    map: &std::collections::BTreeMap<String, String>,
    body: &str,
    line_no: usize,
) -> Result<ContentBlock> {
    let image_chunk_id = required_u64(map, "image", line_no)?;
    let placement = parse_placement(
        map.get("placement").map_or("flow", String::as_str),
        map.get("region").map(String::as_str),
        line_no,
    )?;
    let caption = map.get("caption").cloned().filter(|s| !s.is_empty());
    let (alt_text, img_from_md) = parse_figure_markdown(body, line_no)?;
    let image_chunk_id = img_from_md.unwrap_or(image_chunk_id);
    Ok(ContentBlock::Figure {
        chunk_id: Some(chunk_id),
        figure: FigureRef {
            image_chunk_id,
            alt_text,
            caption,
            placement,
        },
    })
}

fn decode_cite_block(
    chunk_id: u64,
    map: &std::collections::BTreeMap<String, String>,
    body: &str,
) -> ContentBlock {
    ContentBlock::Cite {
        chunk_id: Some(chunk_id),
        cite: CitePayload {
            quote: body.to_owned(),
            target_doc_id: map.get("target_doc").cloned().filter(|s| !s.is_empty()),
            target_chunk_id: optional_u64(map, "target_chunk"),
            target_byte_start: None,
            target_byte_end: None,
            label: map.get("label").cloned().filter(|s| !s.is_empty()),
            page: optional_u32(map, "page"),
            source: None,
        },
    }
}

fn decode_slide_block(
    chunk_id: u64,
    map: &std::collections::BTreeMap<String, String>,
    line_no: usize,
) -> Result<ContentBlock> {
    let layout_id = map
        .get("layout")
        .cloned()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| parse_err(line_no, 1, "slide requires layout=…"))?;
    let regions = parse_slide_regions(map.get("regions").map_or("", String::as_str), line_no)?;
    Ok(ContentBlock::Slide {
        chunk_id: Some(chunk_id),
        slide: SlidePayload { layout_id, regions },
    })
}

fn decode_attachment_block(
    chunk_id: u64,
    map: &std::collections::BTreeMap<String, String>,
    line_no: usize,
) -> Result<ContentBlock> {
    let filename = map
        .get("filename")
        .cloned()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| parse_err(line_no, 1, "attachment requires filename=…"))?;
    let media_type = map
        .get("media_type")
        .cloned()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| parse_err(line_no, 1, "attachment requires media_type=…"))?;
    let sha256 = map
        .get("sha256")
        .cloned()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| parse_err(line_no, 1, "attachment requires sha256=…"))?;
    Ok(ContentBlock::Attachment {
        chunk_id: Some(chunk_id),
        filename,
        media_type,
        caption: map.get("caption").cloned().filter(|s| !s.is_empty()),
        sha256,
    })
}

/// Encode typed content blocks as Tessprek (optional `source-hash` in the header).
///
/// Used by [`normalize_tessprek`] and tests; does not require an open [`TesFile`].
#[must_use]
pub fn encode_content_blocks(source_hash: Option<&str>, blocks: &[ContentBlock]) -> String {
    let mut out = String::new();
    match source_hash {
        Some(hash) if !hash.is_empty() => {
            let _ = writeln!(
                out,
                "{HEADER_PREFIX} format={FORMAT} version={VERSION} source-hash={hash}{COMMENT_SUFFIX}"
            );
        }
        _ => {
            let _ = writeln!(
                out,
                "{HEADER_PREFIX} format={FORMAT} version={VERSION}{COMMENT_SUFFIX}"
            );
        }
    }
    out.push('\n');

    for block in blocks {
        match block {
            ContentBlock::Text {
                chunk_id,
                header,
                body,
                pending_links,
            } => {
                let id = chunk_id.unwrap_or(0);
                write_text_directive(&mut out, id, header);
                out.push_str(render_text_with_pending(header, body, pending_links).trim_end());
                out.push_str("\n\n");
            }
            ContentBlock::Figure { chunk_id, figure } => {
                let id = chunk_id.unwrap_or(0);
                write_figure_directive(&mut out, id, figure);
                let _ = writeln!(
                    out,
                    "![{}](media:chunk-{})",
                    escape_alt(&figure.alt_text),
                    figure.image_chunk_id
                );
                out.push('\n');
            }
            ContentBlock::Cite { chunk_id, cite } => {
                let id = chunk_id.unwrap_or(0);
                write_cite_directive(&mut out, id, cite);
                out.push_str(cite.quote.trim_end());
                out.push_str("\n\n");
            }
            ContentBlock::Slide { chunk_id, slide } => {
                let id = chunk_id.unwrap_or(0);
                write_slide_directive(&mut out, id, slide);
                out.push('\n');
            }
            ContentBlock::Attachment {
                chunk_id,
                filename,
                media_type,
                caption,
                sha256,
            } => {
                let id = chunk_id.unwrap_or(0);
                write_attachment_directive(
                    &mut out,
                    id,
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
        }
    }
    out
}

fn render_text_with_pending(
    header: &TextHeader,
    body: &str,
    pending_links: &[crate::catalog::OutboundLink],
) -> String {
    use crate::catalog::{InlineKind, InlineSpan, LinkKind};

    if pending_links.is_empty() {
        return header.render_markdown(body);
    }

    let mut header = header.clone();
    let mut links = Vec::new();
    for link in pending_links {
        let link_id = u64::try_from(links.len()).unwrap_or(u64::MAX);
        if let Ok(entry) = link.clone().into_entry(0, LinkKind::Wiki) {
            links.push(entry);
            header.spans.push(InlineSpan {
                start: link.start,
                end: link.end,
                kind: InlineKind::Link { link_id },
            });
        }
    }
    header.render_markdown_with_links(body, &links)
}

pub(super) fn write_text_directive(out: &mut String, chunk_id: u64, header: &TextHeader) {
    let _ = write!(
        out,
        "{CHUNK_PREFIX}chunk={chunk_id} role={}",
        header.role.as_str()
    );
    if let Some(level) = header.level {
        let _ = write!(out, " level={level}");
    }
    if let Some(list) = header.list_kind {
        let kind = match list {
            ListKind::Bullet => "bullet",
            ListKind::Ordered => "ordered",
        };
        let _ = write!(out, " list={kind}");
    }
    if header.role == TextRole::ListItem {
        let depth = header.list_depth_or_default();
        if depth > 1 {
            let _ = write!(out, " depth={depth}");
        }
    }
    if let Some(lang) = header.lang.as_deref() {
        let _ = write!(out, " lang={}", attr_token(lang));
    }
    if let Some(align) = header.align {
        let _ = write!(out, " align={}", align.as_str());
    }
    if let Some(code_lang) = header.code_lang.as_deref() {
        let _ = write!(out, " code_lang={}", attr_token(code_lang));
    }
    if !header.classes.is_empty() {
        let _ = write!(out, " class=\"{}\"", header.classes.join(" "));
    }
    let _ = writeln!(out, "{COMMENT_SUFFIX}");
}

fn write_figure_directive(out: &mut String, chunk_id: u64, figure: &FigureRef) {
    let _ = write!(
        out,
        "{CHUNK_PREFIX}chunk={chunk_id} type=figure image={} placement={}",
        figure.image_chunk_id,
        figure.placement.as_str()
    );
    if let ImagePlacement::Region { name } = &figure.placement {
        let _ = write!(out, " region=\"{}\"", escape_attr(name));
    }
    if let Some(caption) = figure.caption.as_deref() {
        let _ = write!(out, " caption=\"{}\"", escape_attr(caption));
    }
    let _ = writeln!(out, "{COMMENT_SUFFIX}");
}

fn write_cite_directive(out: &mut String, chunk_id: u64, cite: &CitePayload) {
    let _ = write!(out, "{CHUNK_PREFIX}chunk={chunk_id} type=cite");
    if let Some(label) = cite.label.as_deref() {
        let _ = write!(out, " label={}", attr_token(label));
    }
    if let Some(doc) = cite.target_doc_id.as_deref() {
        let _ = write!(out, " target_doc={doc}");
    }
    if let Some(chunk) = cite.target_chunk_id {
        let _ = write!(out, " target_chunk={chunk}");
    }
    if let Some(page) = cite.page {
        let _ = write!(out, " page={page}");
    }
    let _ = writeln!(out, "{COMMENT_SUFFIX}");
}

fn write_slide_directive(out: &mut String, chunk_id: u64, slide: &SlidePayload) {
    let regions = slide
        .regions
        .iter()
        .map(|r| format!("{}:{}", r.name, r.chunk_id))
        .collect::<Vec<_>>()
        .join(",");
    let _ = writeln!(
        out,
        "{CHUNK_PREFIX}chunk={chunk_id} type=slide layout={} regions=\"{}\"{COMMENT_SUFFIX}",
        attr_token(&slide.layout_id),
        escape_attr(&regions)
    );
}

fn write_attachment_directive(out: &mut String, chunk_id: u64, att: &AttachmentPayload) {
    let _ = write!(
        out,
        "{CHUNK_PREFIX}chunk={chunk_id} type=attachment filename=\"{}\" media_type={} sha256={}",
        escape_attr(&att.filename),
        attr_token(&att.media_type),
        att.sha256
    );
    if let Some(caption) = att.caption.as_deref() {
        let _ = write!(out, " caption=\"{}\"", escape_attr(caption));
    }
    let _ = writeln!(out, "{COMMENT_SUFFIX}");
}

fn parse_slide_regions(raw: &str, line_no: usize) -> Result<Vec<SlideRegion>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(parse_err(line_no, 1, "slide regions= must be non-empty"));
    }
    let mut regions = Vec::new();
    for part in raw.split(',') {
        let part = part.trim();
        let Some((name, id)) = part.split_once(':') else {
            return Err(parse_err(
                line_no,
                1,
                format!("bad region '{part}' (expected name:chunk_id)"),
            ));
        };
        let chunk_id = id
            .trim()
            .parse::<u64>()
            .map_err(|_| parse_err(line_no, 1, format!("invalid region chunk id in '{part}'")))?;
        regions.push(SlideRegion {
            name: name.trim().to_owned(),
            chunk_id,
        });
    }
    Ok(regions)
}

fn strip_markdown_wrapper(header: &TextHeader, body: &str) -> String {
    let body = body.trim();
    match header.role {
        TextRole::Heading => {
            let trimmed = body.trim_start_matches('#').trim_start();
            trimmed.to_owned()
        }
        TextRole::ListItem => {
            let t = body.trim_start();
            if let Some(rest) = t.strip_prefix("- ") {
                rest.to_owned()
            } else if let Some(rest) = t.strip_prefix("* ") {
                rest.to_owned()
            } else {
                // Ordered: "N. rest"
                let digits = t.chars().take_while(char::is_ascii_digit).count();
                if digits > 0 {
                    let after = &t[digits..];
                    if let Some(rest) = after.strip_prefix(". ") {
                        rest.to_owned()
                    } else if let Some(rest) = after.strip_prefix('.') {
                        rest.trim_start().to_owned()
                    } else {
                        body.to_owned()
                    }
                } else {
                    body.to_owned()
                }
            }
        }
        TextRole::Blockquote => body
            .lines()
            .map(|line| {
                line.strip_prefix("> ")
                    .or_else(|| line.strip_prefix('>'))
                    .unwrap_or(line)
            })
            .collect::<Vec<_>>()
            .join("\n"),
        TextRole::CodeBlock => strip_fence(body),
        TextRole::Table => {
            if body.lines().any(|l| l.trim_start().starts_with('|')) {
                String::new()
            } else {
                strip_fence(body)
            }
        }
        TextRole::Math => {
            let t = body.trim();
            let t = t.strip_prefix("$$").unwrap_or(t);
            let t = t.strip_suffix("$$").unwrap_or(t);
            t.trim().to_owned()
        }
        TextRole::Paragraph => body.to_owned(),
    }
}

fn fence_lang(body: &str) -> Option<String> {
    let first = body.lines().next()?.trim_start();
    let rest = first.strip_prefix("```")?;
    let lang = rest.split_whitespace().next().unwrap_or("");
    (!lang.is_empty()).then(|| lang.to_owned())
}

fn parse_markdown_table(body: &str) -> Option<TableData> {
    let lines: Vec<&str> = body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.len() < 2 || !lines[0].starts_with('|') {
        return None;
    }
    let mut rows = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if i == 1 && line.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
            continue; // separator
        }
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<TableCell> = line
            .trim_matches('|')
            .split('|')
            .map(|c| TableCell {
                text: c.trim().replace("\\|", "|"),
                spans: Vec::new(),
                align: None,
                is_header: i == 0,
                rowspan: None,
                colspan: None,
            })
            .collect();
        if !cells.is_empty() {
            rows.push(TableRow { cells });
        }
    }
    (!rows.is_empty()).then_some(TableData { rows })
}

fn strip_fence(body: &str) -> String {
    let mut lines = body.lines().peekable();
    if lines
        .peek()
        .is_some_and(|l| l.trim_start().starts_with("```"))
    {
        let _ = lines.next();
    }
    let collected: Vec<&str> = lines.collect();
    let end = if collected
        .last()
        .is_some_and(|l| l.trim() == "```" || l.trim_start().starts_with("```"))
    {
        collected.len().saturating_sub(1)
    } else {
        collected.len()
    };
    collected[..end].join("\n")
}

fn parse_figure_markdown(body: &str, line_no: usize) -> Result<(String, Option<u64>)> {
    let body = body.trim();
    // ![alt](media:chunk-N)
    let Some(rest) = body.strip_prefix("![") else {
        return Err(parse_err(
            line_no,
            1,
            "figure body must be ![alt](media:chunk-N)",
        ));
    };
    let Some((alt, after_alt)) = rest.split_once("](") else {
        return Err(parse_err(line_no, 1, "figure markdown missing ']('"));
    };
    let Some(url) = after_alt.strip_suffix(')') else {
        return Err(parse_err(line_no, 1, "figure markdown missing closing ')'"));
    };
    let image_id = url
        .strip_prefix("media:chunk-")
        .and_then(|s| s.parse::<u64>().ok());
    Ok((unescape_alt(alt), image_id))
}

pub(super) fn parse_attrs(
    attrs: &str,
    line_no: usize,
) -> Result<std::collections::BTreeMap<String, String>> {
    let mut map = std::collections::BTreeMap::new();
    let mut rest = attrs.trim();
    while !rest.is_empty() {
        let eq = rest.find('=').ok_or_else(|| {
            parse_err(
                line_no,
                1,
                format!("malformed attribute near '{rest}' (expected key=value)"),
            )
        })?;
        let key = rest[..eq].trim();
        if key.is_empty() {
            return Err(parse_err(line_no, 1, "empty attribute key"));
        }
        rest = rest[eq + 1..].trim_start();
        let (value, next) = if let Some(quoted) = rest.strip_prefix('"') {
            let end = quoted
                .find('"')
                .ok_or_else(|| parse_err(line_no, 1, "unterminated quoted attribute"))?;
            let value = quoted[..end].to_owned();
            (value, quoted[end + 1..].trim_start())
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            (rest[..end].to_owned(), rest[end..].trim_start())
        };
        map.insert(key.to_owned(), value);
        rest = next;
    }
    Ok(map)
}

fn parse_role(raw: &str, line_no: usize) -> Result<TextRole> {
    match raw {
        "paragraph" => Ok(TextRole::Paragraph),
        "heading" => Ok(TextRole::Heading),
        "list_item" => Ok(TextRole::ListItem),
        "blockquote" => Ok(TextRole::Blockquote),
        "code_block" => Ok(TextRole::CodeBlock),
        "table" => Ok(TextRole::Table),
        "math" => Ok(TextRole::Math),
        other => Err(parse_err(line_no, 1, format!("unknown role '{other}'"))),
    }
}

fn parse_list_kind(raw: &str, line_no: usize) -> Result<ListKind> {
    match raw {
        "bullet" => Ok(ListKind::Bullet),
        "ordered" => Ok(ListKind::Ordered),
        other => Err(parse_err(
            line_no,
            1,
            format!("unknown list kind '{other}'"),
        )),
    }
}

fn parse_placement(raw: &str, region: Option<&str>, line_no: usize) -> Result<ImagePlacement> {
    match raw {
        "flow" => Ok(ImagePlacement::Flow),
        "full_width" => Ok(ImagePlacement::FullWidth),
        "float_start" => Ok(ImagePlacement::FloatStart),
        "float_end" => Ok(ImagePlacement::FloatEnd),
        "inline" => Ok(ImagePlacement::Inline),
        "background" => Ok(ImagePlacement::Background),
        "region" => Ok(ImagePlacement::Region {
            name: region.unwrap_or("default").to_owned(),
        }),
        other => Err(parse_err(
            line_no,
            1,
            format!("unknown placement '{other}'"),
        )),
    }
}

fn parse_classes(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or("")
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn required_u64(
    map: &std::collections::BTreeMap<String, String>,
    key: &str,
    line_no: usize,
) -> Result<u64> {
    let raw = map
        .get(key)
        .ok_or_else(|| parse_err(line_no, 1, format!("missing required attribute '{key}'")))?;
    raw.parse::<u64>()
        .map_err(|_| parse_err(line_no, 1, format!("invalid {key} value '{raw}'")))
}

fn required_u32(
    map: &std::collections::BTreeMap<String, String>,
    key: &str,
    line_no: usize,
) -> Result<u32> {
    let raw = map
        .get(key)
        .ok_or_else(|| parse_err(line_no, 1, format!("missing required attribute '{key}'")))?;
    raw.parse::<u32>()
        .map_err(|_| parse_err(line_no, 1, format!("invalid {key} value '{raw}'")))
}

fn optional_u64(map: &std::collections::BTreeMap<String, String>, key: &str) -> Option<u64> {
    map.get(key)?.parse().ok()
}

fn optional_u32(map: &std::collections::BTreeMap<String, String>, key: &str) -> Option<u32> {
    map.get(key)?.parse().ok()
}

pub(super) fn trim_block_body(lines: &[&str]) -> String {
    let mut start = 0;
    let mut end = lines.len();
    while start < end && lines[start].trim().is_empty() {
        start += 1;
    }
    while end > start && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    lines[start..end].join("\n")
}

fn escape_attr(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn attr_token(s: &str) -> String {
    if s.chars().any(|c| c.is_whitespace() || c == '"' || c == '=') {
        format!("\"{}\"", escape_attr(s))
    } else {
        s.to_owned()
    }
}

fn escape_alt(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn unescape_alt(s: &str) -> String {
    s.replace("\\[", "[")
        .replace("\\]", "]")
        .replace("\\\\", "\\")
}

fn parse_err(line: usize, column: usize, message: impl Into<String>) -> TesError {
    TesError::EditParse {
        line,
        column,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{DocumentCatalog, TesWriterSession};
    use crate::layout::DocKind;
    use tempfile::tempdir;

    #[test]
    fn round_trip_text_classes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.tes");
        let mut session = TesWriterSession::create(&path, DocKind::Note);
        session
            .set_catalog(DocumentCatalog::new(
                "550e8400-e29b-41d4-a716-446655440000",
                "Demo",
                "2026-07-27T00:00:00Z",
                "2026-07-27T00:00:00Z",
                DocKind::Note,
            ))
            .unwrap();
        let mut header = TextHeader::heading(1);
        header.classes = vec!["lead".into()];
        session.add_text_chunk(&header, "Hello").unwrap();
        session
            .add_text_chunk(&TextHeader::paragraph(), "Body")
            .unwrap();
        session.commit().unwrap();

        let file = TesFile::open(&path).unwrap();
        let text = encode_tessprek(&file, "abc").unwrap();
        assert!(text.contains("class=\"lead\""));
        let blocks = decode_tessprek(&text).unwrap();
        assert_eq!(blocks.len(), 2);
        match &blocks[0] {
            ContentBlock::Text { header, body, .. } => {
                assert_eq!(header.role, TextRole::Heading);
                assert_eq!(header.classes, vec!["lead"]);
                assert_eq!(body, "Hello");
            }
            _ => panic!("expected text"),
        }
    }
}
