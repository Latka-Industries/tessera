use std::collections::BTreeMap;

use crate::catalog::OutboundLink;
use crate::catalog::chunk::{TextAlign, TextHeader, TextRole};
use crate::edit::ContentBlock;
use crate::error::Result;
use crate::io::import::parse_markdown_blocks;

use super::super::inline_cite::extract_inline_cites;
use super::super::inline_font::extract_inline_fonts_mapped;
use super::parse_err;
use crate::io::import::is_gfm_separator_row;

pub(super) fn looks_like_gfm_table(body: &str) -> bool {
    table_header_and_sep(&nonempty_trimmed_lines_str(body))
}

fn nonempty_trimmed_lines_str(body: &str) -> Vec<&str> {
    body.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect()
}

fn nonempty_trimmed_lines<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect()
}

fn table_header_and_sep(lines: &[&str]) -> bool {
    lines.len() >= 2 && lines[0].starts_with('|') && is_gfm_separator_row(lines[1])
}

/// Split one or more GFM tables out of a contiguous pipe-row run.
///
/// A second header+separator (copied divider under a new header) starts a new
/// table even without a blank line between them.
pub(super) fn split_pipe_run_into_tables(lines: &[&str]) -> Vec<String> {
    let lines = nonempty_trimmed_lines(lines);
    if lines.is_empty() {
        return Vec::new();
    }
    let mut tables = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if !table_header_and_sep(&lines[i..]) {
            tables.push(lines[i..].join("\n"));
            break;
        }
        let start = i;
        i += 2; // header + separator
        while i < lines.len() {
            // Next separator ⇒ current row is the header of the following table.
            if i + 1 < lines.len() && is_gfm_separator_row(lines[i + 1]) {
                break;
            }
            if is_gfm_separator_row(lines[i]) || !lines[i].starts_with('|') {
                break;
            }
            i += 1;
        }
        tables.push(lines[start..i].join("\n"));
    }
    tables
}

pub(super) fn build_table_block(table_md: &str) -> Result<ContentBlock> {
    let parsed = parse_markdown_blocks(table_md);
    if let Some(block) = parsed
        .into_iter()
        .find(|b| b.header.role == TextRole::Table)
    {
        return text_block(None, block.header, &block.body, block.pending_links);
    }
    // Fallback: shouldn't happen since `looks_like_gfm_table` gated this call.
    text_block(None, TextHeader::paragraph(), table_md.trim(), Vec::new())
}

pub(super) fn append_markdown_blocks(
    out: &mut Vec<ContentBlock>,
    markdown: &str,
    preserve: Option<&BTreeMap<String, String>>,
    line_no: usize,
) -> Result<()> {
    let parsed = parse_markdown_blocks(markdown);
    if parsed.is_empty() {
        if markdown.trim().is_empty() {
            return Ok(());
        }
        let mut header = TextHeader::paragraph();
        if let Some(map) = preserve {
            apply_preserved_attrs(&mut header, map, line_no)?;
        }
        out.push(text_block(None, header, markdown.trim(), Vec::new())?);
        return Ok(());
    }

    for (idx, block) in parsed.into_iter().enumerate() {
        let mut header = block.header;
        if idx == 0
            && let Some(map) = preserve
        {
            apply_preserved_attrs(&mut header, map, line_no)?;
        }
        out.push(text_block(None, header, &block.body, block.pending_links)?);
    }
    Ok(())
}

fn text_block(
    chunk_id: Option<u64>,
    mut header: TextHeader,
    body: &str,
    mut pending_links: Vec<OutboundLink>,
) -> Result<ContentBlock> {
    // Fonts first so `\font{id}{\cite{key}}` keeps an extractable cite inside.
    // Markdown spans/links were measured on the pre-strip body — remap them.
    let extracted = extract_inline_fonts_mapped(body)?;
    if !extracted.pending.is_empty() {
        header.spans = header
            .spans
            .into_iter()
            .filter_map(|span| {
                let (start, end) = extracted.remap_range(span.start, span.end)?;
                Some(crate::catalog::chunk::InlineSpan {
                    start,
                    end,
                    kind: span.kind,
                })
            })
            .collect();
        pending_links = pending_links
            .into_iter()
            .filter_map(|link| {
                let (start, end) = extracted.remap_range(link.start, link.end)?;
                Some(OutboundLink {
                    start,
                    end,
                    dest: link.dest,
                })
            })
            .collect();
    }
    let (body, pending_cites) = extract_inline_cites(&extracted.body)?;
    Ok(ContentBlock::Text {
        chunk_id,
        header,
        body,
        pending_links,
        pending_cites,
        pending_fonts: extracted.pending,
    })
}

/// Apply `\block{class=… lang=… align=… caption=…}` attrs onto a Markdown-inferred header.
pub(super) fn apply_preserved_attrs(
    header: &mut TextHeader,
    map: &BTreeMap<String, String>,
    line_no: usize,
) -> Result<()> {
    if header.classes.is_empty()
        && let Some(class) = map.get("class")
    {
        header.classes = class
            .split_whitespace()
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect();
    }
    if header.lang.is_none()
        && let Some(lang) = map.get("lang").filter(|s| !s.is_empty())
    {
        header.lang = Some(lang.clone());
    }
    if header.align.is_none()
        && let Some(align) = map.get("align")
    {
        header.align =
            Some(TextAlign::from_name(align).map_err(|e| parse_err(line_no, 1, format!("{e}")))?);
    }
    if header.role == TextRole::CodeBlock
        && header.code_lang.is_none()
        && let Some(lang) = map.get("code_lang").filter(|s| !s.is_empty())
    {
        header.code_lang = Some(lang.clone());
    }
    if header.title.is_none()
        && let Some(title) = map.get("title").filter(|s| !s.is_empty())
    {
        if !matches!(
            header.role,
            TextRole::Table | TextRole::Math | TextRole::CodeBlock
        ) {
            return Err(parse_err(
                line_no,
                1,
                "title is only valid on table, math, or code_block",
            ));
        }
        header.title = Some(title.clone());
    }
    if header.caption.is_none()
        && let Some(caption) = map.get("caption").filter(|s| !s.is_empty())
    {
        if !matches!(
            header.role,
            TextRole::Table | TextRole::Math | TextRole::CodeBlock
        ) {
            return Err(parse_err(
                line_no,
                1,
                "caption is only valid on table, math, or code_block",
            ));
        }
        header.caption = Some(caption.clone());
    }
    Ok(())
}
