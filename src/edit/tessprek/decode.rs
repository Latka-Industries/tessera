use std::collections::BTreeMap;

use crate::catalog::chunk::{CitePayload, FloatListSource, TextHeader, TextRole};
use crate::catalog::media::FigureRef;
use crate::catalog::slide::SlidePayload;
use crate::error::Result;

use super::super::ContentBlock;
use super::brace::{skip_blank_lines, take_leading_tessera_header};
use super::format;
use super::markers::{self, BRACE_SUFFIX, FORMAT, IDS_PREFIX, VERSION};
use super::util::{
    optional_u32, optional_u64, parse_attrs, parse_err, parse_figure_markdown, parse_placement,
    parse_slide_regions, required_u64,
};

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

    let (header_inner, header_start, header_end) = take_leading_tessera_header(&lines)?;
    let header_line_no = header_start + 1;
    let header_attrs = parse_attrs(&header_inner, header_line_no)?;
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
    let i = skip_blank_lines(&lines, header_end);
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

pub(crate) fn set_chunk_id(block: &mut ContentBlock, id: u64) {
    match block {
        ContentBlock::Text { chunk_id, .. }
        | ContentBlock::Figure { chunk_id, .. }
        | ContentBlock::Cite { chunk_id, .. }
        | ContentBlock::Slide { chunk_id, .. }
        | ContentBlock::Layout { chunk_id, .. }
        | ContentBlock::Attachment { chunk_id, .. } => *chunk_id = Some(id),
    }
}

/// Dispatch a brace-command body (`figure` / `cite` / `quote` / `ref` / …)
/// to its typed decoder. `kind` comes from which prefix matched during
/// scanning (see [`markers::parse_brace_command`]), not from a `type=` attribute.
pub(crate) fn decode_named_directive(
    kind: &str,
    map: &BTreeMap<String, String>,
    body: &str,
    line_no: usize,
) -> Result<ContentBlock> {
    match kind {
        "figure" => decode_figure_block(map, body, line_no),
        "cite" => decode_cite_block(map, line_no),
        "quote" => decode_quote_block(map, line_no),
        "ref" => decode_ref_block(map, line_no),
        "slide" => decode_slide_block(map, line_no),
        "layout" => decode_layout_block(body, line_no),
        "toc" => decode_toc_block(map, line_no),
        "lof" => decode_float_list_block(map, line_no, TextRole::Lof),
        "lot" => decode_float_list_block(map, line_no, TextRole::Lot),
        "columns" => decode_columns_block(map, line_no),
        "endcolumns" => Ok(ContentBlock::Text {
            chunk_id: None,
            header: TextHeader::columns_end(),
            body: String::new(),
            pending_links: Vec::new(),
            pending_cites: Vec::new(),
            pending_fonts: Vec::new(),
        }),
        "attachment" => decode_attachment_block(map, line_no),
        other => Err(parse_err(
            line_no,
            1,
            format!("unknown tessprek directive '\\{other}{{...}}'"),
        )),
    }
}

fn decode_toc_block(map: &BTreeMap<String, String>, line_no: usize) -> Result<ContentBlock> {
    let mut header = TextHeader::toc();
    if let Some(title) = map.get("title").cloned().filter(|s| !s.is_empty()) {
        header.title = Some(title);
    }
    if let Some(depth) = optional_u32(map, "depth") {
        if !(1..=6).contains(&depth) {
            return Err(parse_err(
                line_no,
                1,
                format!("toc depth={depth} must be 1..=6"),
            ));
        }
        header.toc_depth = Some(depth);
    }
    if let Some(raw) = map.get("page_numbers") {
        header.toc_pages = Some(parse_toc_bool(raw, "page_numbers", line_no)?);
    }
    if let Some(raw) = map.get("section_numbers") {
        header.toc_sections = Some(parse_toc_bool(raw, "section_numbers", line_no)?);
    }
    if let Some(raw) = map.get("leaders") {
        header.toc_leaders = Some(parse_toc_bool(raw, "leaders", line_no)?);
    }
    Ok(ContentBlock::Text {
        chunk_id: None,
        header,
        body: String::new(),
        pending_links: Vec::new(),
        pending_cites: Vec::new(),
        pending_fonts: Vec::new(),
    })
}

fn parse_toc_bool(raw: &str, attr: &str, line_no: usize) -> Result<bool> {
    match raw {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        other => Err(parse_err(
            line_no,
            1,
            format!("toc {attr} must be true/false, got '{other}'"),
        )),
    }
}

fn decode_float_list_block(
    map: &BTreeMap<String, String>,
    line_no: usize,
    role: TextRole,
) -> Result<ContentBlock> {
    let mut header = match role {
        TextRole::Lof => TextHeader::lof(),
        TextRole::Lot => TextHeader::lot(),
        _ => unreachable!("float list roles only"),
    };
    if let Some(title) = map.get("title").cloned().filter(|s| !s.is_empty()) {
        header.title = Some(title);
    }
    if let Some(raw) = map.get("page_numbers") {
        header.toc_pages = Some(parse_toc_bool(raw, "page_numbers", line_no)?);
    }
    if let Some(raw) = map.get("leaders") {
        header.toc_leaders = Some(parse_toc_bool(raw, "leaders", line_no)?);
    }
    if let Some(raw) = map.get("source") {
        header.float_list_source = Some(match raw.as_str() {
            "title" => FloatListSource::Title,
            "caption" => FloatListSource::Caption,
            other => {
                return Err(parse_err(
                    line_no,
                    1,
                    format!("lof/lot source must be title or caption, got '{other}'"),
                ));
            }
        });
    }
    if map.contains_key("depth") || map.contains_key("section_numbers") {
        return Err(parse_err(
            line_no,
            1,
            "depth/section_numbers are only valid on \\toc",
        ));
    }
    Ok(ContentBlock::Text {
        chunk_id: None,
        header,
        body: String::new(),
        pending_links: Vec::new(),
        pending_cites: Vec::new(),
        pending_fonts: Vec::new(),
    })
}

fn decode_columns_block(map: &BTreeMap<String, String>, line_no: usize) -> Result<ContentBlock> {
    let mut header = TextHeader::columns();
    if let Some(n) = optional_u32(map, "n") {
        if !(1..=6).contains(&n) {
            return Err(parse_err(
                line_no,
                1,
                format!("columns n={n} must be 1..=6"),
            ));
        }
        header.columns_count = Some(u8::try_from(n).unwrap_or(2));
    }
    if let Some(gap) = optional_u32(map, "gap") {
        if gap > u32::from(u16::MAX) {
            return Err(parse_err(
                line_no,
                1,
                format!("columns gap={gap} exceeds u16"),
            ));
        }
        header.columns_gap = Some(u16::try_from(gap).unwrap_or(0));
    }
    Ok(ContentBlock::Text {
        chunk_id: None,
        header,
        body: String::new(),
        pending_links: Vec::new(),
        pending_cites: Vec::new(),
        pending_fonts: Vec::new(),
    })
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
    let title = map.get("title").cloned().filter(|s| !s.is_empty());
    let caption = map.get("caption").cloned().filter(|s| !s.is_empty());
    let alt_attr = map.get("alt").cloned().filter(|s| !s.is_empty());
    // Legacy: body `![alt](media:N)` after `\figure{…}` (pre-alt-attr Tessprek).
    let (alt_md, img_from_md) = if body.trim().is_empty() {
        (None, None)
    } else {
        let (alt, id) = parse_figure_markdown(body, line_no)?;
        (Some(alt), id)
    };
    let alt_text = alt_md.or(alt_attr).ok_or_else(|| {
        parse_err(
            line_no,
            1,
            "figure requires alt=\"…\" (or legacy ![alt](media:N) body)",
        )
    })?;
    let image_chunk_id = img_from_md.unwrap_or(image_chunk_id);
    Ok(ContentBlock::Figure {
        chunk_id: None,
        figure: FigureRef {
            image_chunk_id,
            alt_text,
            title,
            caption,
            placement,
        },
    })
}

fn decode_cite_block(map: &BTreeMap<String, String>, line_no: usize) -> Result<ContentBlock> {
    if map_has_targets(map) {
        return Err(parse_err(
            line_no,
            1,
            "bibliography \\cite{…} cannot include target_*; use \\quote{…} or \\ref{…}",
        ));
    }
    let label = map
        .get("label")
        .or_else(|| map.get("key"))
        .cloned()
        .filter(|s| !s.is_empty());
    let source = bib_entry_from_attrs(map, label.as_deref());
    Ok(ContentBlock::Cite {
        chunk_id: None,
        cite: CitePayload {
            quote: String::new(),
            target_doc_id: None,
            target_chunk_id: None,
            target_byte_start: None,
            target_byte_end: None,
            label,
            page: optional_u32(map, "page"),
            source,
        },
    })
}

fn decode_quote_block(map: &BTreeMap<String, String>, line_no: usize) -> Result<ContentBlock> {
    if !map_has_targets(map) {
        return Err(parse_err(
            line_no,
            1,
            "\\quote{…} requires target_doc= and/or target_chunk=",
        ));
    }
    let quote = map
        .get("quote")
        .cloned()
        .unwrap_or_default()
        .replace("\\n", "\n");
    if quote.trim().is_empty() {
        return Err(parse_err(
            line_no,
            1,
            "\\quote{…} requires quote=\"…\" (use \\ref{…} for a pointer without excerpt)",
        ));
    }
    Ok(ContentBlock::Cite {
        chunk_id: None,
        cite: CitePayload {
            quote,
            target_doc_id: map.get("target_doc").cloned().filter(|s| !s.is_empty()),
            target_chunk_id: optional_u64(map, "target_chunk"),
            target_byte_start: optional_u32(map, "target_byte_start"),
            target_byte_end: optional_u32(map, "target_byte_end"),
            label: map.get("label").cloned().filter(|s| !s.is_empty()),
            page: optional_u32(map, "page"),
            source: None,
        },
    })
}

fn decode_ref_block(map: &BTreeMap<String, String>, line_no: usize) -> Result<ContentBlock> {
    if !map_has_targets(map) {
        return Err(parse_err(
            line_no,
            1,
            "\\ref{…} requires target_doc= and/or target_chunk=",
        ));
    }
    Ok(ContentBlock::Cite {
        chunk_id: None,
        cite: CitePayload {
            quote: String::new(),
            target_doc_id: map.get("target_doc").cloned().filter(|s| !s.is_empty()),
            target_chunk_id: optional_u64(map, "target_chunk"),
            target_byte_start: optional_u32(map, "target_byte_start"),
            target_byte_end: optional_u32(map, "target_byte_end"),
            label: map.get("label").cloned().filter(|s| !s.is_empty()),
            page: optional_u32(map, "page"),
            source: None,
        },
    })
}

fn map_has_targets(map: &BTreeMap<String, String>) -> bool {
    map.contains_key("target_doc")
        || map.contains_key("target_chunk")
        || map.contains_key("target_byte_start")
        || map.contains_key("target_byte_end")
}

fn bib_entry_from_attrs(
    map: &BTreeMap<String, String>,
    label: Option<&str>,
) -> Option<crate::io::bib::BibEntry> {
    let cite_key = label
        .map(str::to_owned)
        .or_else(|| map.get("key").cloned())
        .filter(|s| !s.is_empty())?;
    let mut entry = crate::io::bib::BibEntry {
        cite_key,
        entry_type: map
            .get("entry_type")
            .cloned()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "misc".into()),
        ..crate::io::bib::BibEntry::default()
    };
    entry.author = map.get("author").cloned().filter(|s| !s.is_empty());
    entry.title = map.get("title").cloned().filter(|s| !s.is_empty());
    entry.journal = map.get("journal").cloned().filter(|s| !s.is_empty());
    entry.year = map.get("year").cloned().filter(|s| !s.is_empty());
    entry.volume = map.get("volume").cloned().filter(|s| !s.is_empty());
    entry.number = map.get("number").cloned().filter(|s| !s.is_empty());
    entry.pages = map.get("pages").cloned().filter(|s| !s.is_empty());
    entry.doi = map.get("doi").cloned().filter(|s| !s.is_empty());
    entry.publisher = map.get("publisher").cloned().filter(|s| !s.is_empty());
    entry.note = map.get("note").cloned().filter(|s| !s.is_empty());
    entry.howpublished = map.get("howpublished").cloned().filter(|s| !s.is_empty());
    entry.url = map.get("url").cloned().filter(|s| !s.is_empty());
    let has_fields = entry.author.is_some()
        || entry.title.is_some()
        || entry.journal.is_some()
        || entry.year.is_some()
        || entry.volume.is_some()
        || entry.number.is_some()
        || entry.pages.is_some()
        || entry.doi.is_some()
        || entry.publisher.is_some()
        || entry.note.is_some()
        || entry.howpublished.is_some()
        || entry.url.is_some()
        || entry.entry_type != "misc";
    has_fields.then_some(entry)
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

fn decode_layout_block(body: &str, line_no: usize) -> Result<ContentBlock> {
    let layout = super::layout_ops::parse_layout_inner(body, line_no)?;
    Ok(ContentBlock::Layout {
        chunk_id: None,
        layout,
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
    let chunk_id = optional_u64(map, "chunk");
    Ok(ContentBlock::Attachment {
        chunk_id,
        filename,
        media_type,
        caption: map.get("caption").cloned().filter(|s| !s.is_empty()),
        sha256,
    })
}
