//! Tessera Markdown (Tessprek) encode/decode for virtual editor buffers.
//!
//! Format (v2): hybrid plain Markdown for heading/paragraph/list/quote/table/
//! math/fenced-code, plus LaTeX-lite brace commands for structured chunks
//! (`\figure{}` / `\cite{}` / `\slide{}` / `\attach{}`) and an optional
//! `\text{class=… lang=… align=…}` directive before a Markdown block when
//! those attrs cannot live in Markdown itself. See `docs/tessprek.md`.
//!
//! `.tes` stays canonical; Tessprek is a lossy projection only.

mod format;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::catalog::TesFile;
use crate::catalog::chunk::{CitePayload, OrderedListNumbering, TextHeader, decode_text_payload};
use crate::catalog::index::ChunkType;
use crate::catalog::media::{AttachmentPayload, FigureRef, ImagePlacement};
use crate::catalog::slide::{SlidePayload, SlideRegion};
use crate::error::{Result, TesError};

use super::ContentBlock;

pub use format::{normalize_tessprek, tessprek_needs_format};

/// Tessprek v2 wire markers: `\tessera{}` header + `\ids{}` reading order,
/// LaTeX-lite brace commands for structured chunks. Shared by encode/decode
/// and LSP hover. No per-block `\id{N}`, no HTML comments, no YAML front
/// matter, no dual-read of the v1 HTML-comment format.
pub mod markers {
    /// `format=` value stamped in the document header.
    pub const FORMAT: &str = "tessprek";
    /// `version=` value stamped in the document header.
    pub const VERSION: &str = "2";
    /// Document header: `\tessera{format=… version=… source-hash=…}`.
    pub const TESSERA_PREFIX: &str = "\\tessera{";
    /// Reading-order chunk id list: `\ids{1,2,3,…}`.
    pub const IDS_PREFIX: &str = "\\ids{";
    /// Optional preserved-attrs directive before a Markdown block.
    pub const TEXT_PREFIX: &str = "\\text{";
    /// Figure directive: `\figure{image=… placement=… caption=…}`.
    pub const FIGURE_PREFIX: &str = "\\figure{";
    /// Cite directive: `\cite{label=… target_chunk=…}` + quote body.
    pub const CITE_PREFIX: &str = "\\cite{";
    /// Slide directive: `\slide{layout=… regions=…}`.
    pub const SLIDE_PREFIX: &str = "\\slide{";
    /// Attachment directive: `\attach{filename=… media_type=… sha256=…}`.
    pub const ATTACH_PREFIX: &str = "\\attach{";
    /// Closing delimiter for every brace command.
    pub const BRACE_SUFFIX: &str = "}";

    /// Header-only brace lines (`\tessera` / `\ids`).
    pub const HEADER_COMMANDS: &[(&str, &str)] =
        &[(TESSERA_PREFIX, "tessera"), (IDS_PREFIX, "ids")];

    /// Body brace lines (structured chunks + optional `\text`).
    /// Kind `attachment` matches [`super::decode_named_directive`].
    pub const BODY_COMMANDS: &[(&str, &str)] = &[
        (TEXT_PREFIX, "text"),
        (FIGURE_PREFIX, "figure"),
        (CITE_PREFIX, "cite"),
        (SLIDE_PREFIX, "slide"),
        (ATTACH_PREFIX, "attachment"),
    ];

    /// Wire surface name for completions (`attachment` → `attach`).
    #[must_use]
    pub fn surface_name(kind: &str) -> &str {
        if kind == "attachment" { "attach" } else { kind }
    }

    /// Parse a brace-command line → `(kind, inner attrs)`.
    ///
    /// When `include_header` is true, also matches `\tessera{…}` / `\ids{…}`
    /// (LSP hover). Format/decode body scans use `include_header = false`.
    #[must_use]
    pub fn parse_brace_command(
        trimmed: &str,
        include_header: bool,
    ) -> Option<(&'static str, &str)> {
        if include_header && let Some(hit) = match_brace_table(trimmed, HEADER_COMMANDS) {
            return Some(hit);
        }
        match_brace_table(trimmed, BODY_COMMANDS)
    }

    fn match_brace_table<'a>(
        trimmed: &'a str,
        table: &'static [(&'static str, &'static str)],
    ) -> Option<(&'static str, &'a str)> {
        for &(prefix, kind) in table {
            if let Some(rest) = trimmed.strip_prefix(prefix)
                && let Some(attrs) = rest.strip_suffix(BRACE_SUFFIX)
            {
                return Some((kind, attrs));
            }
        }
        None
    }
}

use markers::{
    ATTACH_PREFIX, BRACE_SUFFIX, CITE_PREFIX, FIGURE_PREFIX, FORMAT, IDS_PREFIX, SLIDE_PREFIX,
    TESSERA_PREFIX, TEXT_PREFIX, VERSION,
};

/// Encode a `.tes` file as Tessprek, embedding `source_hash`.
///
/// # Errors
///
/// Returns decode errors for reading-order text/figure/cite/slide/attachment
/// payloads.
pub fn encode_tessprek(file: &TesFile, source_hash: &str) -> Result<String> {
    let mut blocks = Vec::new();
    for entry in file.reading_order_chunks() {
        let block = match entry.chunk_type {
            ChunkType::Text => {
                let raw = file.decode_payload(entry)?;
                let (header, body) = decode_text_payload(raw.as_ref())?;
                ContentBlock::Text {
                    chunk_id: Some(entry.chunk_id),
                    header,
                    body,
                    pending_links: Vec::new(),
                }
            }
            ChunkType::Figure => {
                let raw = file.decode_payload(entry)?;
                let figure = FigureRef::from_bytes(raw.as_ref())?;
                ContentBlock::Figure {
                    chunk_id: Some(entry.chunk_id),
                    figure,
                }
            }
            ChunkType::Cite => {
                let raw = file.decode_payload(entry)?;
                let cite = CitePayload::from_bytes(raw.as_ref())?;
                ContentBlock::Cite {
                    chunk_id: Some(entry.chunk_id),
                    cite,
                }
            }
            ChunkType::Slide => {
                let raw = file.decode_payload(entry)?;
                let slide = SlidePayload::from_bytes(raw.as_ref())?;
                ContentBlock::Slide {
                    chunk_id: Some(entry.chunk_id),
                    slide,
                }
            }
            ChunkType::Attachment => {
                let raw = file.decode_payload(entry)?;
                let att = AttachmentPayload::from_bytes(raw.as_ref())?;
                ContentBlock::Attachment {
                    chunk_id: Some(entry.chunk_id),
                    filename: att.filename,
                    media_type: att.media_type,
                    caption: att.caption,
                    sha256: att.sha256,
                }
            }
            _ => continue,
        };
        blocks.push(block);
    }
    Ok(encode_content_blocks(
        Some(source_hash),
        &blocks,
        file.links(),
    ))
}

/// Parse Tessprek v2 into typed content blocks.
///
/// Strict: requires a `\tessera{format=tessprek version=2 …}` header
/// immediately followed by `\ids{…}`, and the id count must match the number
/// of parsed blocks.
///
/// # Errors
///
/// Returns [`TesError::EditParse`] with line/column on malformed directives,
/// bodies, missing header/ids, or an id/block count mismatch.
pub fn decode_tessprek(input: &str) -> Result<Vec<ContentBlock>> {
    Ok(decode_tessprek_with_spans(input)?
        .into_iter()
        .map(|(_, _, block)| block)
        .collect())
}

/// Decode Tessprek with 0-based half-open line spans per block (for LSP hover).
///
/// # Errors
///
/// Same as [`decode_tessprek`].
pub(crate) fn decode_tessprek_with_spans(input: &str) -> Result<Vec<(usize, usize, ContentBlock)>> {
    let lines: Vec<&str> = input.lines().collect();

    let mut i = 0;
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }
    let header_line_no = i + 1;
    let header_trimmed = lines.get(i).map_or("", |l| l.trim());
    let Some(("tessera", header_inner)) = markers::parse_brace_command(header_trimmed, true) else {
        return Err(parse_err(
            header_line_no,
            1,
            format!(
                "expected `{TESSERA_PREFIX}...{BRACE_SUFFIX}` document header, found: {header_trimmed}"
            ),
        ));
    };
    let header_attrs = parse_attrs(header_inner, header_line_no)?;
    if header_attrs.get("format").map(String::as_str) != Some(FORMAT) {
        return Err(parse_err(
            header_line_no,
            1,
            format!("unsupported tessprek header (expected format={FORMAT})"),
        ));
    }
    if header_attrs.get("version").map(String::as_str) != Some(VERSION) {
        return Err(parse_err(
            header_line_no,
            1,
            format!(
                "unsupported tessprek version (expected version={VERSION}); v1 HTML-comment Tessprek is no longer supported"
            ),
        ));
    }
    i += 1;
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }
    let ids_line_no = i + 1;
    let ids_trimmed = lines.get(i).map_or("", |l| l.trim());
    let Some(("ids", ids_inner)) = markers::parse_brace_command(ids_trimmed, true) else {
        return Err(parse_err(
            ids_line_no,
            1,
            format!(
                "expected `{IDS_PREFIX}...{BRACE_SUFFIX}` reading-order id list, found: {ids_trimmed}"
            ),
        ));
    };
    let ids = parse_ids_list(ids_inner, ids_line_no)?;

    let mut spanned = format::build_content_blocks_with_spans(&lines)?;
    if spanned.len() != ids.len() {
        return Err(parse_err(
            ids_line_no,
            1,
            format!(
                "`{IDS_PREFIX}...{BRACE_SUFFIX}` declares {} id(s) but document has {} block(s); \
                 run `:TesseraFormat` (or enable format-on-save) / `tes format` to refresh `\\ids{{}}`",
                ids.len(),
                spanned.len()
            ),
        ));
    }
    for ((_, _, block), id) in spanned.iter_mut().zip(ids) {
        set_chunk_id(block, id);
    }
    Ok(spanned)
}

fn parse_ids_list(inner: &str, line_no: usize) -> Result<Vec<u64>> {
    let inner = inner.trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    inner
        .split(',')
        .map(|s| {
            let s = s.trim();
            s.parse::<u64>().map_err(|_| {
                parse_err(
                    line_no,
                    1,
                    format!("invalid id '{s}' in `{IDS_PREFIX}...{BRACE_SUFFIX}`"),
                )
            })
        })
        .collect()
}

pub(super) fn set_chunk_id(block: &mut ContentBlock, id: u64) {
    match block {
        ContentBlock::Text { chunk_id, .. }
        | ContentBlock::Figure { chunk_id, .. }
        | ContentBlock::Cite { chunk_id, .. }
        | ContentBlock::Slide { chunk_id, .. }
        | ContentBlock::Attachment { chunk_id, .. } => *chunk_id = Some(id),
    }
}

/// Dispatch a brace-command body (`figure` / `cite` / `slide` / `attachment`)
/// to its typed decoder. `kind` comes from which prefix matched during
/// scanning (see [`markers::parse_brace_command`]), not from a `type=` attribute.
fn decode_named_directive(
    kind: &str,
    map: &BTreeMap<String, String>,
    body: &str,
    line_no: usize,
) -> Result<ContentBlock> {
    match kind {
        "figure" => decode_figure_block(map, body, line_no),
        "cite" => Ok(decode_cite_block(map, body)),
        "slide" => decode_slide_block(map, line_no),
        "attachment" => decode_attachment_block(map, line_no),
        other => Err(parse_err(
            line_no,
            1,
            format!("unknown tessprek directive '\\{other}{{...}}'"),
        )),
    }
}

fn decode_figure_block(
    map: &BTreeMap<String, String>,
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
        chunk_id: None,
        figure: FigureRef {
            image_chunk_id,
            alt_text,
            caption,
            placement,
        },
    })
}

fn decode_cite_block(map: &BTreeMap<String, String>, body: &str) -> ContentBlock {
    ContentBlock::Cite {
        chunk_id: None,
        cite: CitePayload {
            quote: strip_quote_body(body),
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

fn decode_slide_block(map: &BTreeMap<String, String>, line_no: usize) -> Result<ContentBlock> {
    let layout_id = map
        .get("layout")
        .cloned()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| parse_err(line_no, 1, "slide requires layout=…"))?;
    let regions = parse_slide_regions(map.get("regions").map_or("", String::as_str), line_no)?;
    Ok(ContentBlock::Slide {
        chunk_id: None,
        slide: SlidePayload { layout_id, regions },
    })
}

fn decode_attachment_block(map: &BTreeMap<String, String>, line_no: usize) -> Result<ContentBlock> {
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
        chunk_id: None,
        filename,
        media_type,
        caption: map.get("caption").cloned().filter(|s| !s.is_empty()),
        sha256,
    })
}

/// Encode typed content blocks as Tessprek v2 (optional `source-hash`).
///
/// `links` resolves `InlineKind::Link` spans on blocks whose `pending_links`
/// is empty (e.g. blocks freshly decoded from a `.tes` file); pass `&[]` when
/// blocks already carry `pending_links` (normalize / typed ops).
///
/// Used by [`normalize_tessprek`], [`encode_tessprek`], and tests.
#[must_use]
pub fn encode_content_blocks(
    source_hash: Option<&str>,
    blocks: &[ContentBlock],
    links: &[crate::catalog::LinkEntry],
) -> String {
    let mut out = String::new();
    write_header(&mut out, source_hash);
    write_ids(&mut out, blocks);
    out.push('\n');

    let mut ordered = OrderedListNumbering::default();
    for (i, block) in blocks.iter().enumerate() {
        let next = blocks.get(i + 1);
        match block {
            ContentBlock::Text {
                header,
                body,
                pending_links,
                ..
            } => {
                let ordered_index = ordered.take_for_text(header);
                write_text_directive(&mut out, header);
                out.push_str(
                    render_text_body(header, body, pending_links, links, ordered_index).trim_end(),
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
                        let _ = writeln!(
                            out,
                            "![{}](media:chunk-{})",
                            escape_alt(&figure.alt_text),
                            figure.image_chunk_id
                        );
                        out.push('\n');
                    }
                    ContentBlock::Cite { cite, .. } => {
                        write_cite_directive(&mut out, cite);
                        out.push_str(&render_quote_body(&cite.quote));
                        out.push_str("\n\n");
                    }
                    ContentBlock::Slide { slide, .. } => {
                        write_slide_directive(&mut out, slide);
                        out.push('\n');
                    }
                    ContentBlock::Attachment {
                        filename,
                        media_type,
                        caption,
                        sha256,
                        ..
                    } => {
                        write_attachment_directive(
                            &mut out,
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

fn write_brace_line(out: &mut String, prefix: &str, parts: &[String]) {
    let _ = writeln!(out, "{prefix}{}{BRACE_SUFFIX}", parts.join(" "));
}

fn write_header(out: &mut String, source_hash: Option<&str>) {
    let mut parts = vec![format!("format={FORMAT}"), format!("version={VERSION}")];
    if let Some(hash) = source_hash.filter(|h| !h.is_empty()) {
        parts.push(format!("source-hash={hash}"));
    }
    write_brace_line(out, TESSERA_PREFIX, &parts);
}

fn write_ids(out: &mut String, blocks: &[ContentBlock]) {
    let ids = blocks
        .iter()
        .map(|b| b.chunk_id().unwrap_or(0).to_string())
        .collect::<Vec<_>>()
        .join(",");
    write_brace_line(out, IDS_PREFIX, &[ids]);
}

fn render_text_body(
    header: &TextHeader,
    body: &str,
    pending_links: &[crate::catalog::OutboundLink],
    links: &[crate::catalog::LinkEntry],
    ordered_index: Option<u32>,
) -> String {
    use crate::catalog::{InlineKind, InlineSpan, LinkKind};

    if pending_links.is_empty() {
        return header.render_markdown_with_links_indexed(body, links, ordered_index);
    }

    let mut header = header.clone();
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
    header.render_markdown_with_links_indexed(body, &synthetic_links, ordered_index)
}

/// Write `\text{class=… lang=… align=…}` when the header carries attrs that
/// cannot live in plain Markdown. Emits nothing otherwise.
fn write_text_directive(out: &mut String, header: &TextHeader) {
    if header.classes.is_empty() && header.lang.is_none() && header.align.is_none() {
        return;
    }
    let mut parts = Vec::new();
    if !header.classes.is_empty() {
        parts.push(format!("class=\"{}\"", header.classes.join(" ")));
    }
    if let Some(lang) = header.lang.as_deref() {
        parts.push(format!("lang={}", attr_token(lang)));
    }
    if let Some(align) = header.align {
        parts.push(format!("align={}", align.as_str()));
    }
    write_brace_line(out, TEXT_PREFIX, &parts);
}

fn write_figure_directive(out: &mut String, figure: &FigureRef) {
    let mut parts = vec![
        format!("image={}", figure.image_chunk_id),
        format!("placement={}", figure.placement.as_str()),
    ];
    if let ImagePlacement::Region { name } = &figure.placement {
        parts.push(format!("region=\"{}\"", escape_attr(name)));
    }
    if let Some(caption) = figure.caption.as_deref() {
        parts.push(format!("caption=\"{}\"", escape_attr(caption)));
    }
    write_brace_line(out, FIGURE_PREFIX, &parts);
}

fn write_cite_directive(out: &mut String, cite: &CitePayload) {
    let mut parts = Vec::new();
    if let Some(label) = cite.label.as_deref() {
        parts.push(format!("label={}", attr_token(label)));
    }
    if let Some(doc) = cite.target_doc_id.as_deref() {
        parts.push(format!("target_doc={doc}"));
    }
    if let Some(chunk) = cite.target_chunk_id {
        parts.push(format!("target_chunk={chunk}"));
    }
    if let Some(page) = cite.page {
        parts.push(format!("page={page}"));
    }
    write_brace_line(out, CITE_PREFIX, &parts);
}

fn write_slide_directive(out: &mut String, slide: &SlidePayload) {
    let regions = slide
        .regions
        .iter()
        .map(|r| format!("{}:{}", r.name, r.chunk_id))
        .collect::<Vec<_>>()
        .join(",");
    write_brace_line(
        out,
        SLIDE_PREFIX,
        &[
            format!("layout={}", attr_token(&slide.layout_id)),
            format!("regions=\"{}\"", escape_attr(&regions)),
        ],
    );
}

fn write_attachment_directive(out: &mut String, att: &AttachmentPayload) {
    let mut parts = vec![
        format!("filename=\"{}\"", escape_attr(&att.filename)),
        format!("media_type={}", attr_token(&att.media_type)),
        format!("sha256={}", att.sha256),
    ];
    if let Some(caption) = att.caption.as_deref() {
        parts.push(format!("caption=\"{}\"", escape_attr(caption)));
    }
    write_brace_line(out, ATTACH_PREFIX, &parts);
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

/// Render a `CitePayload.quote` as a Markdown-blockquote-styled body.
fn render_quote_body(quote: &str) -> String {
    let trimmed = quote.trim_end();
    if trimmed.is_empty() {
        return String::from(">");
    }
    trimmed
        .lines()
        .map(|l| {
            if l.is_empty() {
                ">".to_owned()
            } else {
                format!("> {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Strip the `> ` blockquote styling from a `\cite{}` body.
fn strip_quote_body(body: &str) -> String {
    body.lines()
        .map(|line| {
            line.strip_prefix("> ")
                .or_else(|| line.strip_prefix('>'))
                .unwrap_or(line)
        })
        .collect::<Vec<_>>()
        .join("\n")
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

pub(crate) fn parse_attrs(attrs: &str, line_no: usize) -> Result<BTreeMap<String, String>> {
    let mut map = BTreeMap::new();
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

fn required_u64(map: &BTreeMap<String, String>, key: &str, line_no: usize) -> Result<u64> {
    let raw = map
        .get(key)
        .ok_or_else(|| parse_err(line_no, 1, format!("missing required attribute '{key}'")))?;
    raw.parse::<u64>()
        .map_err(|_| parse_err(line_no, 1, format!("invalid {key} value '{raw}'")))
}

fn optional_u64(map: &BTreeMap<String, String>, key: &str) -> Option<u64> {
    map.get(key)?.parse().ok()
}

fn optional_u32(map: &BTreeMap<String, String>, key: &str) -> Option<u32> {
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
    use crate::catalog::chunk::TextRole;
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
        assert!(text.contains("\\tessera{format=tessprek version=2 source-hash=abc}"));
        assert!(text.contains("\\text{class=\"lead\"}"), "{text}");
        assert!(text.contains("# Hello"), "{text}");
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

    #[test]
    fn round_trip_figure_cite_slide_attachment() {
        let blocks = vec![
            ContentBlock::Text {
                chunk_id: Some(1),
                header: TextHeader::heading(1),
                body: "Doc".into(),
                pending_links: Vec::new(),
            },
            ContentBlock::Figure {
                chunk_id: Some(2),
                figure: FigureRef {
                    image_chunk_id: 3,
                    alt_text: "A photo".into(),
                    caption: Some("Cap".into()),
                    placement: ImagePlacement::Flow,
                },
            },
            ContentBlock::Cite {
                chunk_id: Some(4),
                cite: CitePayload {
                    quote: "Some quoted text".into(),
                    target_doc_id: None,
                    target_chunk_id: Some(1),
                    target_byte_start: None,
                    target_byte_end: None,
                    label: Some("Smith2024".into()),
                    page: None,
                    source: None,
                },
            },
            ContentBlock::Slide {
                chunk_id: Some(5),
                slide: SlidePayload {
                    layout_id: "title".into(),
                    regions: vec![SlideRegion {
                        name: "body".into(),
                        chunk_id: 1,
                    }],
                },
            },
            ContentBlock::Attachment {
                chunk_id: Some(6),
                filename: "notes.pdf".into(),
                media_type: "application/pdf".into(),
                caption: Some("Handout".into()),
                sha256: "deadbeef".into(),
            },
        ];
        let text = encode_content_blocks(None, &blocks, &[]);
        assert!(text.contains("\\ids{1,2,4,5,6}"), "{text}");
        assert!(text.contains("\\figure{"), "{text}");
        assert!(text.contains("\\cite{"), "{text}");
        assert!(text.contains("> Some quoted text"), "{text}");
        assert!(text.contains("\\slide{"), "{text}");
        assert!(text.contains("\\attach{"), "{text}");
        let decoded = decode_tessprek(&text).unwrap();
        assert_eq!(decoded, blocks);
    }

    #[test]
    fn decode_rejects_missing_header() {
        let err = decode_tessprek("# Title\n").unwrap_err();
        assert!(matches!(err, TesError::EditParse { .. }));
    }

    #[test]
    fn decode_rejects_id_count_mismatch() {
        let text = "\\tessera{format=tessprek version=2}\n\\ids{1,2}\n\n# Title\n";
        let err = decode_tessprek(text).unwrap_err();
        match err {
            TesError::EditParse { message, .. } => {
                assert!(message.contains("id(s)"), "{message}");
                assert!(
                    message.contains("TesseraFormat") || message.contains("tes format"),
                    "{message}"
                );
            }
            other => panic!("expected EditParse, got {other:?}"),
        }
    }

    #[test]
    fn decode_rejects_v1_version() {
        let text = "\\tessera{format=tessprek version=1}\n\\ids{}\n";
        let err = decode_tessprek(text).unwrap_err();
        match err {
            TesError::EditParse { message, .. } => {
                assert!(message.contains("v1"), "{message}");
            }
            other => panic!("expected EditParse, got {other:?}"),
        }
    }
}
