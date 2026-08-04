//! Normalize Tessprek buffers so Markdown-shaped bodies imply correct directives.
//!
//! Reuses [`crate::io::import::parse_markdown_blocks`] for role / list depth /
//! fence language inference (same rules as `tes import --markdown`).
//!
//! v2 has no per-block ids: `\ids{…}` is a flat, positional, reading-order list
//! under the header. [`build_content_blocks`] scans the body into free Markdown
//! runs (optionally preceded by `\text{…}`) and brace-command directives
//! (`\figure{}` / `\cite{}` / `\slide{}` / `\attach{}`), producing blocks with
//! `chunk_id: None`; callers assign ids afterward ([`decode_tessprek`] strictly
//! from `\ids{}`, [`normalize_tessprek`] via [`IdAllocator`]).
//!
//! [`decode_tessprek`]: super::decode_tessprek

use std::collections::BTreeMap;

use crate::catalog::chunk::{TextAlign, TextHeader, TextRole};
use crate::error::{Result, TesError};
use crate::io::import::parse_markdown_blocks;

use super::super::ContentBlock;
use super::{
    TessprekDocMeta, decode_named_directive, encode_content_blocks, markers, parse_attrs,
    set_chunk_id, skip_blank_lines, take_leading_tessera_header, take_tessera_header,
    trim_block_body,
};

use markers::{IDS_PREFIX, parse_brace_command};

/// Normalize a Tessprek buffer: infer text roles from Markdown shape, split
/// multi-block bodies, allocate/reuse `\ids{}` positionally, and re-emit
/// canonical Tessprek.
///
/// Free Markdown (no preceding `\text{}`) is accepted. Brace-command
/// directives (`\figure{}` / `\cite{}` / `\slide{}` / `\attach{}`) are
/// preserved.
///
/// # Errors
///
/// Returns [`TesError::EditParse`] for malformed directives.
pub fn normalize_tessprek(input: &str) -> Result<String> {
    let lines: Vec<&str> = input.lines().collect();
    let declared_ids = extract_declared_ids(&lines);
    let mut blocks = build_content_blocks(&lines)?;

    let mut ids = IdAllocator::new(declared_ids.iter().copied().collect());
    for (idx, block) in blocks.iter_mut().enumerate() {
        let preferred = declared_ids.get(idx).copied();
        let id = ids.alloc(preferred);
        set_chunk_id(block, id);
    }

    let meta = extract_doc_meta(&lines);
    Ok(encode_content_blocks(&meta, &blocks, &[]))
}

/// True when `normalize_tessprek(input)` would change the buffer (ignoring a
/// single trailing newline difference).
///
/// # Errors
///
/// Propagates normalize / parse errors.
pub fn tessprek_needs_format(input: &str) -> Result<bool> {
    let normalized = normalize_tessprek(input)?;
    Ok(normalize_newlines(&normalized) != normalize_newlines(input))
}

/// Scan a Tessprek body into typed content blocks (`chunk_id: None`).
///
/// Shared by [`super::decode_tessprek`] (strict `\ids{}` zip) and
/// [`normalize_tessprek`] (positional id allocation).
///
/// # Errors
///
/// Returns [`TesError::EditParse`] for malformed directives.
pub(super) fn build_content_blocks(lines: &[&str]) -> Result<Vec<ContentBlock>> {
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
                end,
                preserve,
            } => {
                // `\text{…}` occupies `start`; body begins on the next line.
                let body_start = if preserve.is_some() {
                    start.saturating_add(1).min(end)
                } else {
                    start
                };
                append_mixed_markdown_spanned(&mut out, lines, body_start, end, preserve.as_ref())?;
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
                blocks.push(build_table_block(&table));
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
        ContentBlock::Cite { cite, .. } => first_nonempty_line(&cite.quote),
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

#[derive(Debug)]
enum Segment {
    /// Free Markdown run, optionally preceded by `\text{…}` (attrs applied to
    /// the first resulting block). Lines are 0-based half-open `[start, end)`.
    Markdown {
        start: usize,
        end: usize,
        preserve: Option<BTreeMap<String, String>>,
    },
    /// A brace-command directive (`figure` / `cite` / `slide` / `attachment`).
    Directive {
        start: usize,
        end: usize,
        kind: String,
        map: BTreeMap<String, String>,
        body: String,
    },
}

/// Reuses declared `\ids{}` values positionally; falls back to fresh ids
/// beyond the max declared value.
struct IdAllocator {
    reserved: std::collections::BTreeSet<u64>,
    emitted: std::collections::BTreeSet<u64>,
    next_fresh: u64,
}

impl IdAllocator {
    fn new(reserved: std::collections::BTreeSet<u64>) -> Self {
        let max = reserved.iter().next_back().copied().unwrap_or(0);
        Self {
            reserved,
            emitted: std::collections::BTreeSet::new(),
            next_fresh: max.saturating_add(1).max(1),
        }
    }

    fn alloc(&mut self, preferred: Option<u64>) -> u64 {
        if let Some(id) = preferred {
            if self.reserved.remove(&id) {
                self.emitted.insert(id);
                self.bump_fresh();
                return id;
            }
            if !self.emitted.contains(&id) {
                self.emitted.insert(id);
                self.bump_fresh();
                return id;
            }
        }
        loop {
            let id = self.next_fresh;
            self.next_fresh = self.next_fresh.saturating_add(1);
            if !self.emitted.contains(&id) && !self.reserved.contains(&id) {
                self.emitted.insert(id);
                return id;
            }
        }
    }

    fn bump_fresh(&mut self) {
        while self.emitted.contains(&self.next_fresh) || self.reserved.contains(&self.next_fresh) {
            self.next_fresh = self.next_fresh.saturating_add(1);
        }
    }
}

fn extract_doc_meta(lines: &[&str]) -> TessprekDocMeta {
    let Ok((attrs, _, _)) = take_leading_tessera_header(lines) else {
        return TessprekDocMeta::default();
    };
    let Ok(map) = parse_attrs(&attrs, 1) else {
        return TessprekDocMeta::default();
    };
    TessprekDocMeta::from_attrs(&map)
}

/// Lenient scan for the first `\ids{…}` list anywhere in the buffer.
fn extract_declared_ids(lines: &[&str]) -> Vec<u64> {
    for line in lines {
        let Some(("ids", inner)) = parse_brace_command(line.trim(), true) else {
            continue;
        };
        return inner
            .split(',')
            .filter_map(|s| s.trim().parse::<u64>().ok())
            .collect();
    }
    Vec::new()
}

fn scan_segments(lines: &[&str]) -> Result<Vec<Segment>> {
    let mut segments = Vec::new();
    let mut i = skip_header_and_blanks(lines, 0);

    while i < lines.len() {
        let start = i;
        let line_no = i + 1;
        let trimmed = lines[i].trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        if let Some((kind, attrs)) = parse_brace_command(trimmed, false) {
            let map = parse_attrs(attrs, line_no)?;
            i += 1;
            match kind {
                "text" => {
                    i = next_boundary(lines, i);
                    segments.push(Segment::Markdown {
                        start,
                        end: i,
                        preserve: Some(map),
                    });
                }
                "slide" | "attachment" => {
                    segments.push(Segment::Directive {
                        start,
                        end: i,
                        kind: kind.to_owned(),
                        map,
                        body: String::new(),
                    });
                }
                _ => {
                    let body_start = i;
                    i = next_boundary(lines, i);
                    let body = trim_block_body(&lines[body_start..i]);
                    segments.push(Segment::Directive {
                        start,
                        end: i,
                        kind: kind.to_owned(),
                        map,
                        body,
                    });
                }
            }
        } else {
            i = next_boundary(lines, i);
            // Skip all-blank runs (shouldn't happen after trim gate above).
            if lines[start..i].iter().any(|l| !l.trim().is_empty()) {
                segments.push(Segment::Markdown {
                    start,
                    end: i,
                    preserve: None,
                });
            }
        }
    }

    Ok(segments)
}

fn looks_like_gfm_table(body: &str) -> bool {
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

fn is_gfm_separator_row(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('|') && t.contains('-') && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

fn table_header_and_sep(lines: &[&str]) -> bool {
    lines.len() >= 2 && lines[0].starts_with('|') && is_gfm_separator_row(lines[1])
}

/// Split one or more GFM tables out of a contiguous pipe-row run.
///
/// A second header+separator (copied divider under a new header) starts a new
/// table even without a blank line between them.
fn split_pipe_run_into_tables(lines: &[&str]) -> Vec<String> {
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

fn build_table_block(table_md: &str) -> ContentBlock {
    let parsed = parse_markdown_blocks(table_md);
    if let Some(block) = parsed
        .into_iter()
        .find(|b| b.header.role == TextRole::Table)
    {
        return ContentBlock::Text {
            chunk_id: None,
            header: block.header,
            body: block.body,
            pending_links: block.pending_links,
        };
    }
    // Fallback: shouldn't happen since `looks_like_gfm_table` gated this call.
    ContentBlock::Text {
        chunk_id: None,
        header: TextHeader::paragraph(),
        body: table_md.trim().to_owned(),
        pending_links: Vec::new(),
    }
}

fn append_markdown_blocks(
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
        out.push(ContentBlock::Text {
            chunk_id: None,
            header,
            body: markdown.trim().to_owned(),
            pending_links: Vec::new(),
        });
        return Ok(());
    }

    for (idx, block) in parsed.into_iter().enumerate() {
        let mut header = block.header;
        if idx == 0
            && let Some(map) = preserve
        {
            apply_preserved_attrs(&mut header, map, line_no)?;
        }
        out.push(ContentBlock::Text {
            chunk_id: None,
            header,
            body: block.body,
            pending_links: block.pending_links,
        });
    }
    Ok(())
}

/// Apply `\text{class=… lang=… align=…}` attrs onto a Markdown-inferred header.
fn apply_preserved_attrs(
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
    Ok(())
}

fn skip_header_and_blanks(lines: &[&str], mut i: usize) -> usize {
    i = skip_blank_lines(lines, i);
    if let Ok((_, end)) = take_tessera_header(lines, i) {
        i = end;
    }
    i = skip_blank_lines(lines, i);
    if lines
        .get(i)
        .is_some_and(|l| l.trim().starts_with(IDS_PREFIX))
    {
        i += 1;
    }
    skip_blank_lines(lines, i)
}

fn next_boundary(lines: &[&str], mut i: usize) -> usize {
    while i < lines.len() {
        if parse_brace_command(lines[i].trim(), false).is_some() {
            break;
        }
        i += 1;
    }
    i
}

fn normalize_newlines(s: &str) -> String {
    let mut t = s.replace("\r\n", "\n");
    if t.ends_with('\n') {
        t.pop();
    }
    t
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

    #[test]
    fn splits_free_markdown_and_assigns_sequential_ids() {
        let input = "# Title\n\n- one\n- two\n";
        let out = normalize_tessprek(input).unwrap();
        assert!(out.contains("\\tessera{"), "{out}");
        assert!(out.contains("format=tessprek"), "{out}");
        assert!(out.contains("version=2"), "{out}");
        assert!(out.contains("\\ids{1,2,3}"), "{out}");
        assert!(out.contains("# Title"), "{out}");
        assert!(out.contains("- one"), "{out}");
        assert!(out.contains("- two"), "{out}");
        // No brace directives needed for plain roles.
        assert!(!out.contains("\\text{"), "{out}");
    }

    #[test]
    fn free_markdown_gets_ids_and_stays_markdown() {
        let input = "## Section\n\n1. first\n2. second\n";
        let out = normalize_tessprek(input).unwrap();
        assert!(out.contains("## Section"), "{out}");
        assert!(out.contains("1. first\n2. second"), "{out}");
        assert!(!out.contains("1. first\n\n2. second"), "{out}");
        assert!(out.contains("\\ids{1,2,3}"), "{out}");
    }

    #[test]
    fn nested_list_depth() {
        let input = "- top\n  - nested\n";
        let out = normalize_tessprek(input).unwrap();
        assert!(out.contains("  - nested"), "{out}");
    }

    #[test]
    fn preserves_source_hash_and_code_lang() {
        let input = "\\tessera{format=tessprek version=2 source-hash=abc123}\n\\ids{9}\n\n```rust\nfn x() {}\n```\n";
        let out = normalize_tessprek(input).unwrap();
        assert!(out.contains("source-hash=abc123"), "{out}");
        assert!(out.contains("```rust"), "{out}");
        assert!(out.contains("fn x() {}"), "{out}");
        assert!(out.contains("\\ids{9}"), "{out}");
    }

    #[test]
    fn preserves_rich_tessera_doc_meta() {
        let input = "\
\\tessera{format=tessprek version=2 source-hash=abc doc_id=550e8400-e29b-41d4-a716-446655440099 doc_kind=note title=\"Text roles\" language=en cite_style_id=numeric}\n\
\\ids{1}\n\
\n\
Hi\n\
";
        let out = normalize_tessprek(input).unwrap();
        assert!(out.contains("source-hash=abc"), "{out}");
        assert!(
            out.contains("doc_id=550e8400-e29b-41d4-a716-446655440099"),
            "{out}"
        );
        assert!(out.contains("doc_kind=note"), "{out}");
        assert!(out.contains("title=\"Text roles\""), "{out}");
        assert!(out.contains("language=en"), "{out}");
        assert!(out.contains("cite_style_id=numeric"), "{out}");
        assert!(out.contains("\\ids{1}"), "{out}");
    }

    #[test]
    fn idempotent_on_normalized() {
        let input = "\\tessera{format=tessprek version=2}\n\\ids{1,2}\n\n# Hello\n\n- item\n";
        let once = normalize_tessprek(input).unwrap();
        let twice = normalize_tessprek(&once).unwrap();
        assert_eq!(normalize_newlines(&once), normalize_newlines(&twice));
        assert!(!tessprek_needs_format(&once).unwrap());
    }

    #[test]
    fn gfm_table_stays_table() {
        let input = "\\tessera{format=tessprek version=2}\n\\ids{1}\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n";
        let out = normalize_tessprek(input).unwrap();
        assert!(out.contains('|'), "{out}");
        assert!(out.contains("\\ids{1}"), "{out}");
    }

    #[test]
    fn trailing_prose_after_table_becomes_paragraph() {
        let input = "\\tessera{format=tessprek version=2}\n\\ids{1}\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\nI am testing\n";
        let out = normalize_tessprek(input).unwrap();
        assert!(out.contains("I am testing"), "{out}");
        assert!(out.contains("\\ids{1,2}"), "{out}");
    }

    #[test]
    fn second_table_after_blank_line() {
        let input = "\\tessera{format=tessprek version=2}\n\\ids{1}\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n| C | D |\n| --- | --- |\n| 3 | 4 |\n";
        let out = normalize_tessprek(input).unwrap();
        assert!(out.matches('|').count() > 6, "{out}");
        assert!(out.contains("\\ids{1,2}"), "{out}");
        assert!(out.contains('C'), "{out}");
    }

    #[test]
    fn second_table_via_copied_divider_no_blank() {
        let input = "\\tessera{format=tessprek version=2}\n\\ids{1}\n\n| Role | Markdown cue |\n| --- | --- |\n| heading | # Title |\n| list_item | - / 1. |\n| Role | Markdown cue |\n| --- | --- |\n| heading | # Title |\n| list_item | - / 1. |\n";
        let out = normalize_tessprek(input).unwrap();
        assert!(out.contains("\\ids{1,2}"), "{out}");
    }

    #[test]
    fn text_directive_preserves_class_and_align() {
        let input = "\\text{class=\"lead\" align=center}\n# Hello\n";
        let out = normalize_tessprek(input).unwrap();
        assert!(out.contains("\\text{class=\"lead\" align=center}"), "{out}");
        assert!(out.contains("# Hello"), "{out}");
    }
}
