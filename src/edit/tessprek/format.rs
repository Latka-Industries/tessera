//! Normalize Tessprek buffers so Markdown-shaped bodies imply correct directives.
//!
//! Reuses [`crate::io::import::parse_markdown_blocks`] for role / list depth /
//! fence language inference (same rules as `tes import --markdown`).

use std::collections::{BTreeMap, BTreeSet};

use crate::catalog::chunk::{TextAlign, TextHeader, TextRole};
use crate::error::{Result, TesError};
use crate::io::import::parse_markdown_blocks;

use super::super::ContentBlock;
use super::{decode_directive_block, encode_content_blocks, markers, parse_attrs, trim_block_body};

use markers::{CHUNK_PREFIX, COMMENT_SUFFIX, HEADER_PREFIX};

/// Normalize a Tessprek buffer: infer text roles from Markdown shape, split
/// multi-block bodies, assign/reuse `chunk=` ids, and re-emit canonical Tessprek.
///
/// Free Markdown gaps (no directive) are accepted and get new chunk ids.
/// Non-text directives (`figure` / `cite` / `slide` / `attachment`) are preserved.
///
/// # Errors
///
/// Returns [`TesError::EditParse`] for malformed non-text directives.
pub fn normalize_tessprek(input: &str) -> Result<String> {
    let lines: Vec<&str> = input.lines().collect();
    let source_hash = extract_source_hash(&lines);
    let segments = scan_segments(&lines)?;
    let mut ids = IdAllocator::new(collect_declared_ids(&segments));

    let mut blocks = Vec::new();
    for segment in segments {
        match segment {
            Segment::Free { line_no, text } => {
                append_mixed_markdown(
                    &mut blocks,
                    &text,
                    None,
                    &BTreeMap::new(),
                    line_no,
                    &mut ids,
                )?;
            }
            Segment::Text {
                line_no,
                chunk_id,
                map,
                body,
            } => {
                append_mixed_markdown(&mut blocks, &body, Some(chunk_id), &map, line_no, &mut ids)?;
            }
            Segment::Other {
                line_no,
                kind,
                chunk_id,
                map,
                body,
            } => {
                let block = decode_directive_block(&kind, chunk_id, &map, &body, line_no)?;
                ids.claim(chunk_id);
                blocks.push(block);
            }
        }
    }

    Ok(encode_content_blocks(source_hash.as_deref(), &blocks))
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

#[derive(Debug)]
enum Segment {
    Free {
        line_no: usize,
        text: String,
    },
    Text {
        line_no: usize,
        chunk_id: u64,
        map: BTreeMap<String, String>,
        body: String,
    },
    Other {
        line_no: usize,
        kind: String,
        chunk_id: u64,
        map: BTreeMap<String, String>,
        body: String,
    },
}

struct IdAllocator {
    /// Declared in the input and not yet emitted.
    reserved: BTreeSet<u64>,
    emitted: BTreeSet<u64>,
    next_fresh: u64,
}

impl IdAllocator {
    fn new(reserved: BTreeSet<u64>) -> Self {
        let max = reserved.iter().next_back().copied().unwrap_or(0);
        Self {
            reserved,
            emitted: BTreeSet::new(),
            next_fresh: max.saturating_add(1).max(1),
        }
    }

    fn claim(&mut self, id: u64) {
        self.reserved.remove(&id);
        self.emitted.insert(id);
        self.bump_fresh();
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
        let trimmed = line.trim();
        if !trimmed.starts_with(HEADER_PREFIX) || !trimmed.ends_with(COMMENT_SUFFIX) {
            continue;
        }
        let attrs = &trimmed[HEADER_PREFIX.len()..trimmed.len() - COMMENT_SUFFIX.len()];
        let Ok(map) = parse_attrs(attrs, 1) else {
            continue;
        };
        if let Some(hash) = map.get("source-hash").filter(|s| !s.is_empty()) {
            return Some(hash.clone());
        }
    }
    None
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

        if trimmed.starts_with(CHUNK_PREFIX) && trimmed.ends_with(COMMENT_SUFFIX) {
            let attrs = &trimmed[CHUNK_PREFIX.len()..trimmed.len() - COMMENT_SUFFIX.len()];
            let map = parse_attrs(attrs, line_no)?;
            let chunk_id = required_u64(&map, "chunk", line_no)?;
            let kind = map
                .get("type")
                .map(String::as_str)
                .or_else(|| map.get("role").map(|_| "text"))
                .unwrap_or("text")
                .to_owned();
            i += 1;
            let body_start = i;
            i = next_directive_index(lines, i);
            let body = trim_block_body(&lines[body_start..i]);
            if is_text_kind(&kind) {
                segments.push(Segment::Text {
                    line_no,
                    chunk_id,
                    map,
                    body,
                });
            } else {
                segments.push(Segment::Other {
                    line_no,
                    kind,
                    chunk_id,
                    map,
                    body,
                });
            }
        } else {
            let body_start = i;
            i = next_directive_index(lines, i);
            let text = trim_block_body(&lines[body_start..i]);
            if !text.is_empty() {
                segments.push(Segment::Free { line_no, text });
            }
        }
    }

    Ok(segments)
}

fn is_text_kind(kind: &str) -> bool {
    matches!(
        kind,
        "text"
            | "paragraph"
            | "heading"
            | "list_item"
            | "blockquote"
            | "code_block"
            | "table"
            | "math"
    )
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

/// Emit tables and other Markdown from a Tessprek text body / free gap.
///
/// Blank lines separate sections. Pipe sections become one chunk per GFM table
/// (split on a repeated header+`---` divider). Everything else uses
/// [`parse_markdown_blocks`].
fn append_mixed_markdown(
    out: &mut Vec<ContentBlock>,
    markdown: &str,
    mut first_id: Option<u64>,
    preserve: &BTreeMap<String, String>,
    line_no: usize,
    ids: &mut IdAllocator,
) -> Result<()> {
    let lines: Vec<&str> = markdown.lines().collect();
    let mut i = 0;
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
                push_table_block(out, &table, first_id.take(), line_no, ids)?;
            }
        } else {
            let preferred = first_id.take();
            append_markdown_blocks(out, &section, preferred, preserve, line_no, ids)?;
        }
    }
    Ok(())
}

fn push_table_block(
    out: &mut Vec<ContentBlock>,
    table_md: &str,
    preferred: Option<u64>,
    line_no: usize,
    ids: &mut IdAllocator,
) -> Result<()> {
    let chunk_id = ids.alloc(preferred);
    let mut map = BTreeMap::new();
    map.insert("role".into(), "table".into());
    out.push(decode_directive_block(
        "table", chunk_id, &map, table_md, line_no,
    )?);
    Ok(())
}

fn append_markdown_blocks(
    out: &mut Vec<ContentBlock>,
    markdown: &str,
    first_id: Option<u64>,
    preserve: &BTreeMap<String, String>,
    line_no: usize,
    ids: &mut IdAllocator,
) -> Result<()> {
    let parsed = parse_markdown_blocks(markdown);
    if parsed.is_empty() {
        if markdown.trim().is_empty() {
            return Ok(());
        }
        let mut header = TextHeader::paragraph();
        apply_preserved_attrs(&mut header, preserve, line_no)?;
        out.push(ContentBlock::Text {
            chunk_id: Some(ids.alloc(first_id)),
            header,
            body: markdown.trim().to_owned(),
            pending_links: Vec::new(),
        });
        return Ok(());
    }

    for (idx, block) in parsed.into_iter().enumerate() {
        let mut header = block.header;
        apply_preserved_attrs(&mut header, preserve, line_no)?;
        let preferred = if idx == 0 { first_id } else { None };
        out.push(ContentBlock::Text {
            chunk_id: Some(ids.alloc(preferred)),
            header,
            body: block.body,
            pending_links: block.pending_links,
        });
    }
    Ok(())
}

fn apply_preserved_attrs(
    header: &mut TextHeader,
    map: &BTreeMap<String, String>,
    line_no: usize,
) -> Result<()> {
    if header.classes.is_empty() {
        header.classes = map
            .get("class")
            .map(|s| {
                s.split_whitespace()
                    .filter(|p| !p.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
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
        && let Some(lang) = map.get("code_lang").or_else(|| map.get("fence"))
        && !lang.is_empty()
    {
        header.code_lang = Some(lang.clone());
    }
    Ok(())
}

fn collect_declared_ids(segments: &[Segment]) -> BTreeSet<u64> {
    let mut ids = BTreeSet::new();
    for segment in segments {
        match segment {
            Segment::Text { chunk_id, .. } | Segment::Other { chunk_id, .. } => {
                ids.insert(*chunk_id);
            }
            Segment::Free { .. } => {}
        }
    }
    ids
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

fn required_u64(map: &BTreeMap<String, String>, key: &str, line_no: usize) -> Result<u64> {
    let raw = map
        .get(key)
        .ok_or_else(|| parse_err(line_no, 1, format!("missing required attribute '{key}'")))?;
    raw.parse::<u64>()
        .map_err(|_| parse_err(line_no, 1, format!("invalid {key} value '{raw}'")))
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
    fn fixes_wrong_roles_and_splits_list() {
        let input = r#"<!-- tessera: format=tessprek version=1 -->

<!-- tes chunk=1 role=paragraph -->
# Title

<!-- tes chunk=2 role=paragraph -->
- one
- two
"#;
        let out = normalize_tessprek(input).unwrap();
        assert!(out.contains("role=heading level=1"), "{out}");
        assert!(out.contains("role=list_item list=bullet"), "{out}");
        assert!(out.contains("chunk=1 "), "{out}");
        assert!(out.contains("chunk=2 "), "{out}");
        assert!(out.contains("chunk=3 "), "{out}");
        assert!(out.contains("# Title"), "{out}");
        assert!(out.contains("- one"), "{out}");
        assert!(out.contains("- two"), "{out}");
    }

    #[test]
    fn free_markdown_gets_directives() {
        let input = r#"<!-- tessera: format=tessprek version=1 -->

## Section

1. first
2. second
"#;
        let out = normalize_tessprek(input).unwrap();
        assert!(out.contains("role=heading level=2"), "{out}");
        assert!(out.contains("list=ordered"), "{out}");
    }

    #[test]
    fn nested_list_depth() {
        let input = r#"<!-- tessera: format=tessprek version=1 -->

- top
  - nested
"#;
        let out = normalize_tessprek(input).unwrap();
        assert!(out.contains("depth=2"), "{out}");
    }

    #[test]
    fn preserves_source_hash_and_code_lang() {
        let input = r#"<!-- tessera: format=tessprek version=1 source-hash=abc123 -->

<!-- tes chunk=9 role=paragraph -->
```rust
fn x() {}
```
"#;
        let out = normalize_tessprek(input).unwrap();
        assert!(out.contains("source-hash=abc123"), "{out}");
        assert!(out.contains("role=code_block"), "{out}");
        assert!(out.contains("code_lang=rust"), "{out}");
        assert!(out.contains("chunk=9 "), "{out}");
    }

    #[test]
    fn idempotent_on_normalized() {
        let input = r#"<!-- tessera: format=tessprek version=1 -->

<!-- tes chunk=1 role=heading level=1 -->
# Hello

<!-- tes chunk=2 role=list_item list=bullet -->
- item
"#;
        let once = normalize_tessprek(input).unwrap();
        let twice = normalize_tessprek(&once).unwrap();
        assert_eq!(normalize_newlines(&once), normalize_newlines(&twice));
        assert!(!tessprek_needs_format(&once).unwrap());
    }

    #[test]
    fn gfm_table_stays_table() {
        let input = r#"<!-- tessera: format=tessprek version=1 -->

<!-- tes chunk=1 role=paragraph -->
| A | B |
| --- | --- |
| 1 | 2 |
"#;
        let out = normalize_tessprek(input).unwrap();
        assert!(out.contains("role=table"), "{out}");
        assert!(out.contains('|'), "{out}");
    }

    #[test]
    fn trailing_prose_after_table_becomes_paragraph() {
        let input = r#"<!-- tessera: format=tessprek version=1 -->

<!-- tes chunk=1 role=table -->
| A | B |
| --- | --- |
| 1 | 2 |

I am testing
"#;
        let out = normalize_tessprek(input).unwrap();
        assert!(out.contains("role=table"), "{out}");
        assert!(out.contains("role=paragraph"), "{out}");
        assert!(out.contains("I am testing"), "{out}");
        assert!(out.contains("chunk=2 "), "{out}");
    }

    #[test]
    fn second_table_after_blank_line() {
        let input = r#"<!-- tessera: format=tessprek version=1 -->

<!-- tes chunk=1 role=table -->
| A | B |
| --- | --- |
| 1 | 2 |

| C | D |
| --- | --- |
| 3 | 4 |
"#;
        let out = normalize_tessprek(input).unwrap();
        assert_eq!(out.matches("role=table").count(), 2, "{out}");
        assert!(out.contains("chunk=2 "), "{out}");
        assert!(out.contains("| C | D |") || out.contains("| C |"), "{out}");
    }

    #[test]
    fn second_table_via_copied_divider_no_blank() {
        let input = r#"<!-- tessera: format=tessprek version=1 -->

<!-- tes chunk=1 role=table -->
| Role | Markdown cue |
| --- | --- |
| heading | # Title |
| list_item | - / 1. |
| Role | Markdown cue |
| --- | --- |
| heading | # Title |
| list_item | - / 1. |
"#;
        let out = normalize_tessprek(input).unwrap();
        assert_eq!(out.matches("role=table").count(), 2, "{out}");
        assert!(out.contains("chunk=2 "), "{out}");
    }
}
