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
    decode_named_directive, encode_content_blocks, markers, parse_attrs, set_chunk_id,
    trim_block_body,
};

use markers::{IDS_PREFIX, TESSERA_PREFIX, parse_brace_command};

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
    let source_hash = extract_source_hash(&lines);
    let declared_ids = extract_declared_ids(&lines);
    let mut blocks = build_content_blocks(&lines)?;

    let mut ids = IdAllocator::new(declared_ids.iter().copied().collect());
    for (idx, block) in blocks.iter_mut().enumerate() {
        let preferred = declared_ids.get(idx).copied();
        let id = ids.alloc(preferred);
        set_chunk_id(block, id);
    }

    Ok(encode_content_blocks(source_hash.as_deref(), &blocks, &[]))
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
    let segments = scan_segments(lines)?;
    let mut blocks = Vec::new();
    for segment in segments {
        match segment {
            Segment::Markdown {
                line_no,
                preserve,
                text,
            } => {
                append_mixed_markdown(&mut blocks, &text, preserve.as_ref(), line_no)?;
            }
            Segment::Directive {
                line_no,
                kind,
                map,
                body,
            } => {
                blocks.push(decode_named_directive(&kind, &map, &body, line_no)?);
            }
        }
    }
    Ok(blocks)
}

#[derive(Debug)]
enum Segment {
    /// Free Markdown run, optionally preceded by `\text{…}` (attrs applied to
    /// the first resulting block).
    Markdown {
        line_no: usize,
        preserve: Option<BTreeMap<String, String>>,
        text: String,
    },
    /// A brace-command directive (`figure` / `cite` / `slide` / `attachment`).
    Directive {
        line_no: usize,
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

fn extract_source_hash(lines: &[&str]) -> Option<String> {
    for line in lines {
        let Some(("tessera", attrs)) = parse_brace_command(line.trim(), true) else {
            continue;
        };
        let Ok(map) = parse_attrs(attrs, 1) else {
            continue;
        };
        if let Some(hash) = map.get("source-hash").filter(|s| !s.is_empty()) {
            return Some(hash.clone());
        }
    }
    None
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
                    let body_start = i;
                    i = next_boundary(lines, i);
                    let text = trim_block_body(&lines[body_start..i]);
                    segments.push(Segment::Markdown {
                        line_no,
                        preserve: Some(map),
                        text,
                    });
                }
                "slide" | "attachment" => {
                    segments.push(Segment::Directive {
                        line_no,
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
                        line_no,
                        kind: kind.to_owned(),
                        map,
                        body,
                    });
                }
            }
        } else {
            let body_start = i;
            i = next_boundary(lines, i);
            let text = trim_block_body(&lines[body_start..i]);
            if !text.is_empty() {
                segments.push(Segment::Markdown {
                    line_no,
                    preserve: None,
                    text,
                });
            }
        }
    }

    Ok(segments)
}

fn looks_like_gfm_table(body: &str) -> bool {
    let lines: Vec<&str> = body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    table_header_and_sep(&lines)
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
    let lines: Vec<&str> = lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
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

/// Emit tables and other Markdown from a Tessprek free-Markdown run.
///
/// Blank lines separate sections. Pipe sections become one chunk per GFM table
/// (split on a repeated header+`---` divider). Everything else uses
/// [`parse_markdown_blocks`]. `preserve` (from a leading `\text{…}`) applies
/// only to the very first non-table block.
fn append_mixed_markdown(
    out: &mut Vec<ContentBlock>,
    markdown: &str,
    preserve: Option<&BTreeMap<String, String>>,
    line_no: usize,
) -> Result<()> {
    let lines: Vec<&str> = markdown.lines().collect();
    let mut i = 0;
    let mut pending_preserve = preserve;
    while i < lines.len() {
        while i < lines.len() && lines[i].trim().is_empty() {
            i += 1;
        }
        if i >= lines.len() {
            break;
        }
        let start = i;
        while i < lines.len() && !lines[i].trim().is_empty() {
            i += 1;
        }
        let section_lines = &lines[start..i];
        let section = section_lines.join("\n");
        if looks_like_gfm_table(&section) {
            for table in split_pipe_run_into_tables(section_lines) {
                out.push(build_table_block(&table));
            }
        } else {
            append_markdown_blocks(out, &section, pending_preserve.take(), line_no)?;
        }
    }
    Ok(())
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
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.is_empty()
            || trimmed.starts_with(TESSERA_PREFIX)
            || trimmed.starts_with(IDS_PREFIX)
        {
            i += 1;
            continue;
        }
        break;
    }
    i
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
        assert!(
            out.contains("\\tessera{format=tessprek version=2}"),
            "{out}"
        );
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
        assert!(out.contains("1. first"), "{out}");
        assert!(out.contains("1. second"), "{out}");
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
