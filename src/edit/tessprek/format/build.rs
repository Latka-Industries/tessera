use std::collections::BTreeMap;

use crate::catalog::chunk::TextRole;
use crate::error::Result;

use super::super::decode_named_directive;
use super::markdown::{
    append_markdown_blocks, apply_preserved_attrs, build_table_block, looks_like_gfm_table,
    split_pipe_run_into_tables,
};
use super::scan::{Segment, scan_segments};
use crate::edit::ContentBlock;

/// Scan a Tessprek body into typed content blocks (`chunk_id: None`).
///
/// Shared by [`super::super::decode_tessprek`] (strict `\ids{}` zip) and
/// [`super::normalize_tessprek`] (positional id allocation).
///
/// # Errors
///
/// Returns [`TesError::EditParse`] for malformed directives.
pub(crate) fn build_content_blocks(lines: &[&str]) -> Result<Vec<ContentBlock>> {
    Ok(build_content_blocks_with_spans(lines)?
        .into_iter()
        .map(|(_, _, block)| block)
        .collect())
}

/// Like [`build_content_blocks`], but each block carries a 0-based half-open
/// line span `[start, end)` covering its Tessprek source lines (for LSP hover).
///
/// # Errors
///
/// Returns [`TesError::EditParse`] for malformed directives.
pub(crate) fn build_content_blocks_with_spans(
    lines: &[&str],
) -> Result<Vec<(usize, usize, ContentBlock)>> {
    let segments = scan_segments(lines)?;
    let mut out = Vec::new();
    for segment in segments {
        match segment {
            Segment::Markdown {
                start,
                body_start,
                end,
                preserve,
            } => {
                let before = out.len();
                append_mixed_markdown_spanned(&mut out, lines, body_start, end, preserve.as_ref())?;
                // Include the `\text{…}` opener lines in the first block's hover span.
                if preserve.is_some()
                    && let Some((block_start, _, _)) = out.get_mut(before)
                {
                    *block_start = start;
                }
            }
            Segment::Directive {
                start,
                end,
                kind,
                map,
                body,
            } => {
                let line_no = start + 1;
                let block = decode_named_directive(&kind, &map, &body, line_no)?;
                out.push((start, end, block));
            }
        }
    }
    Ok(out)
}

/// Walk blank-separated sections in `lines[start..end)`, emitting blocks with
/// absolute line spans (avoids whole-run greedy anchoring that mis-attributed
/// math lines to a following table).
fn append_mixed_markdown_spanned(
    out: &mut Vec<(usize, usize, ContentBlock)>,
    lines: &[&str],
    start: usize,
    end: usize,
    preserve: Option<&BTreeMap<String, String>>,
) -> Result<()> {
    let mut i = start;
    let mut pending_preserve = preserve;
    while i < end {
        while i < end && lines[i].trim().is_empty() {
            i += 1;
        }
        if i >= end {
            break;
        }
        let sec_start = i;
        while i < end && !lines[i].trim().is_empty() {
            i += 1;
        }
        let sec_end = i;
        let section_lines = &lines[sec_start..sec_end];
        let section = section_lines.join("\n");
        let line_no = sec_start + 1;

        let mut blocks = Vec::new();
        if looks_like_gfm_table(&section) {
            for table in split_pipe_run_into_tables(section_lines) {
                blocks.push(build_table_block(&table)?);
            }
            if let Some(map) = pending_preserve.take()
                && let Some(ContentBlock::Text { header, .. }) = blocks.first_mut()
            {
                apply_preserved_attrs(header, map, line_no)?;
            }
        } else {
            append_markdown_blocks(&mut blocks, &section, pending_preserve.take(), line_no)?;
        }

        let ranges = distribute_line_spans(sec_start, sec_end, lines, &blocks);
        for ((s, e), block) in ranges.into_iter().zip(blocks) {
            out.push((s, e, block));
        }
    }
    Ok(())
}

/// Split `[start, end)` across `blocks.len()` spans by matching each block's
/// first significant body line (fallback: even-ish slices).
fn distribute_line_spans(
    start: usize,
    end: usize,
    lines: &[&str],
    blocks: &[ContentBlock],
) -> Vec<(usize, usize)> {
    if blocks.is_empty() {
        return Vec::new();
    }
    if blocks.len() == 1 || start >= end {
        return vec![(start, clamp_span_end(start, end, lines.len()))];
    }

    let mut starts = Vec::with_capacity(blocks.len());
    let mut cursor = start;
    for (idx, block) in blocks.iter().enumerate() {
        if idx == 0 {
            starts.push(start);
            continue;
        }
        let needle = block_anchor_line(block);
        let found = lines
            .iter()
            .enumerate()
            .take(end)
            .skip(cursor)
            .find_map(|(j, line)| line_anchors(line, &needle).then_some(j));
        let next_start = found.unwrap_or(cursor);
        starts.push(next_start);
        cursor = next_start.saturating_add(1).min(end);
    }

    let mut ranges = Vec::with_capacity(blocks.len());
    for i in 0..blocks.len() {
        let s = starts[i];
        let e = if i + 1 < starts.len() {
            starts[i + 1].max(s)
        } else {
            end.max(s)
        };
        ranges.push((s, clamp_span_end(s, e, lines.len())));
    }
    ranges
}

fn clamp_span_end(start: usize, end: usize, line_count: usize) -> usize {
    end.max(start.saturating_add(1).min(line_count.max(start + 1)))
}

fn block_anchor_line(block: &ContentBlock) -> String {
    match block {
        ContentBlock::Text { header, .. } if header.role == TextRole::Math => "$$".to_owned(),
        ContentBlock::Text { header, body, .. } if header.role == TextRole::CodeBlock => {
            format!("```{}", header.code_lang.as_deref().unwrap_or(""))
        }
        ContentBlock::Text { header, body, .. } if header.role == TextRole::Table => header
            .table
            .as_ref()
            .and_then(|t| t.rows.first())
            .and_then(|r| r.cells.first())
            .map_or_else(|| first_nonempty_line(body), |c| format!("| {} ", c.text)),
        ContentBlock::Text { body, .. } => first_nonempty_line(body),
        ContentBlock::Figure { figure, .. } => first_nonempty_line(&figure.alt_text),
        ContentBlock::Cite { cite, .. } => {
            cite.label.as_deref().filter(|s| !s.is_empty()).map_or_else(
                || {
                    let q = first_nonempty_line(&cite.quote);
                    if q.is_empty() { "cite".into() } else { q }
                },
                str::to_owned,
            )
        }
        ContentBlock::Slide { slide, .. } => slide.layout_id.clone(),
        ContentBlock::Attachment { filename, .. } => filename.clone(),
    }
}

fn first_nonempty_line(raw: &str) -> String {
    raw.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_owned()
}

fn line_anchors(line: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let t = line.trim();
    t == needle || t.starts_with(needle) || needle.starts_with(t)
}
