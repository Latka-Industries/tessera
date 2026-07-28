//! Chunk payload codecs (`docs/layout_v0.md` — *Text chunk payload*).
//!
//! Layout-v1 text structure (spans, math, structured tables, language) is stored
//! as additive JSON fields on the text header — `layout_version` stays 0 until a
//! full container bump.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Result, TesError};
use crate::io::bib::BibEntry;
use argus::{LeReader, LeWriter};

/// Maximum text-chunk semantic header size (4 KiB).
pub const TEXT_HEADER_MAX_BYTES: usize = 4 * 1024;

/// Semantic role of a text chunk body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextRole {
    /// Body paragraph.
    Paragraph,
    /// Heading; use with [`TextHeader::level`].
    Heading,
    /// List item; use with [`TextHeader::list_kind`].
    ListItem,
    /// Block quote / pull quote.
    Blockquote,
    /// Monospace block; optional [`TextHeader::code_lang`].
    CodeBlock,
    /// Table: prefer [`TextHeader::table`]; v0 TSV body remains accepted.
    Table,
    /// Display math; body is LaTeX source.
    Math,
}

/// List marker kind for [`TextRole::ListItem`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListKind {
    /// Unordered bullet.
    Bullet,
    /// Ordered / numbered.
    Ordered,
}

/// Semantic horizontal alignment (never physical left/right).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    /// Start edge in writing direction.
    Start,
    /// Centered.
    Center,
    /// End edge in writing direction.
    End,
    /// Justified.
    Justify,
}

/// Closed inline formatting vocabulary (`docs/structure_v1.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InlineKind {
    /// Italic / emphasis.
    Emphasis,
    /// Bold / strong.
    Strong,
    /// Inline code.
    Code,
    /// Defined term.
    Term,
    /// Inline quotation.
    Quote,
    /// Inline math (LaTeX).
    Math {
        /// LaTeX source.
        tex: String,
    },
    /// Reference into the link table / link records.
    Link {
        /// Link record id.
        link_id: u64,
    },
    /// Reference to a cite chunk.
    Citation {
        /// Cite chunk id.
        cite_chunk_id: u64,
    },
}

/// Half-open UTF-8 byte range with a typed kind over a text body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineSpan {
    /// Inclusive start byte offset into the body.
    pub start: u32,
    /// Exclusive end byte offset into the body.
    pub end: u32,
    /// Formatting / reference kind.
    pub kind: InlineKind,
}

/// One cell in a structured table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableCell {
    /// Plain cell text.
    #[serde(default)]
    pub text: String,
    /// Optional inline spans over [`Self::text`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<InlineSpan>,
    /// Cell alignment override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<TextAlign>,
    /// Whether this cell is a column/row header.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_header: bool,
    /// Row span (HTML-like); omit or 1 for default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rowspan: Option<u32>,
    /// Column span; omit or 1 for default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colspan: Option<u32>,
}

/// One table row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableRow {
    /// Ordered cells.
    pub cells: Vec<TableCell>,
}

/// Structured table payload stored on the text header when `role = table`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableData {
    /// Ordered rows.
    pub rows: Vec<TableRow>,
}

/// JSON header prefixed to a type-`1` text chunk payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextHeader {
    /// Semantic role of the body.
    pub role: TextRole,
    /// Heading level 1–6 when `role` is [`TextRole::Heading`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    /// List marker when `role` is [`TextRole::ListItem`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_kind: Option<ListKind>,
    /// Legacy string emphasis tags (prefer [`Self::spans`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emphasis: Vec<String>,
    /// Theme hints imported from semantic HTML `class` attributes.
    #[serde(default, rename = "class", skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<String>,
    /// Ranged inline formatting over the UTF-8 body.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<InlineSpan>,
    /// Optional BCP-47 language override for this block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    /// Optional semantic alignment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<TextAlign>,
    /// Optional programming language when `role` is [`TextRole::CodeBlock`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_lang: Option<String>,
    /// Structured table when `role` is [`TextRole::Table`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<TableData>,
}

impl TextRole {
    /// Lowercase role name used in JSONL / debug headers.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Paragraph => "paragraph",
            Self::Heading => "heading",
            Self::ListItem => "list_item",
            Self::Blockquote => "blockquote",
            Self::CodeBlock => "code_block",
            Self::Table => "table",
            Self::Math => "math",
        }
    }
}

impl TextAlign {
    /// Lowercase wire / Tessprek name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Center => "center",
            Self::End => "end",
            Self::Justify => "justify",
        }
    }

    /// Parse a lowercase align name.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::InvalidTextHeader`] for unknown names.
    pub fn from_name(name: &str) -> Result<Self> {
        Ok(match name {
            "start" => Self::Start,
            "center" => Self::Center,
            "end" => Self::End,
            "justify" => Self::Justify,
            other => {
                return Err(TesError::InvalidTextHeader {
                    message: format!("unknown text align '{other}'"),
                });
            }
        })
    }
}

impl TextHeader {
    /// Empty optional fields for `role`.
    #[must_use]
    pub fn with_role(role: TextRole) -> Self {
        Self {
            role,
            level: None,
            list_kind: None,
            emphasis: Vec::new(),
            classes: Vec::new(),
            spans: Vec::new(),
            lang: None,
            align: None,
            code_lang: None,
            table: None,
        }
    }

    /// A plain paragraph header.
    #[must_use]
    pub fn paragraph() -> Self {
        Self::with_role(TextRole::Paragraph)
    }

    /// Whether this header uses additive layout-v1 fields (`text_spans` feature).
    #[must_use]
    pub fn uses_layout_v1_features(&self) -> bool {
        !self.spans.is_empty()
            || self.lang.is_some()
            || self.align.is_some()
            || self.code_lang.is_some()
            || self.table.is_some()
            || matches!(self.role, TextRole::Table | TextRole::Math)
    }

    /// A heading header at `level` (1–6).
    #[must_use]
    pub fn heading(level: u32) -> Self {
        let mut h = Self::with_role(TextRole::Heading);
        h.level = Some(level);
        h
    }

    /// A list-item header.
    #[must_use]
    pub fn list_item(kind: ListKind) -> Self {
        let mut h = Self::with_role(TextRole::ListItem);
        h.list_kind = Some(kind);
        h
    }

    /// A code-block header with optional fence language.
    #[must_use]
    pub fn code_block(code_lang: Option<&str>) -> Self {
        let mut h = Self::with_role(TextRole::CodeBlock);
        h.code_lang = code_lang.map(str::to_owned).filter(|s| !s.is_empty());
        h
    }

    /// A display-math header (body is LaTeX).
    #[must_use]
    pub fn math() -> Self {
        Self::with_role(TextRole::Math)
    }

    /// A structured table header.
    #[must_use]
    pub fn table(data: TableData) -> Self {
        let mut h = Self::with_role(TextRole::Table);
        h.table = Some(data);
        h
    }

    /// Validate spans/table fields against `body`.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::InvalidTextHeader`] when ranges are empty, inverted,
    /// out of bounds, not on UTF-8 character boundaries, or when table cell
    /// spans are invalid.
    pub fn validate(&self, body: &str) -> Result<()> {
        validate_spans(body, &self.spans)?;
        if let Some(table) = &self.table {
            if self.role != TextRole::Table {
                return Err(TesError::InvalidTextHeader {
                    message: "table payload requires role=table".into(),
                });
            }
            for (ri, row) in table.rows.iter().enumerate() {
                for (ci, cell) in row.cells.iter().enumerate() {
                    validate_spans(&cell.text, &cell.spans).map_err(|e| match e {
                        TesError::InvalidTextHeader { message } => TesError::InvalidTextHeader {
                            message: format!("table[{ri}][{ci}]: {message}"),
                        },
                        other => other,
                    })?;
                    if matches!(cell.rowspan, Some(0)) || matches!(cell.colspan, Some(0)) {
                        return Err(TesError::InvalidTextHeader {
                            message: format!("table[{ri}][{ci}]: rowspan/colspan must be >= 1"),
                        });
                    }
                }
            }
        }
        if self.code_lang.is_some() && self.role != TextRole::CodeBlock {
            return Err(TesError::InvalidTextHeader {
                message: "code_lang is only valid on code_block".into(),
            });
        }
        if self.role == TextRole::Heading
            && let Some(level) = self.level
            && !(1..=6).contains(&level)
        {
            return Err(TesError::InvalidTextHeader {
                message: format!("heading level {level} must be 1..=6"),
            });
        }
        Ok(())
    }

    /// Lossy Markdown projection of a text-chunk body (export + Tessprek).
    #[must_use]
    pub fn render_markdown(&self, body: &str) -> String {
        self.render_markdown_with_links(body, &[])
    }

    /// Markdown projection resolving [`InlineKind::Link`] via the document link table.
    #[must_use]
    pub fn render_markdown_with_links(
        &self,
        body: &str,
        links: &[crate::catalog::LinkEntry],
    ) -> String {
        let body = body.trim_end();
        let spanned = apply_spans_markdown(body, &self.spans, links);
        match self.role {
            TextRole::Heading => {
                let level = self.level.unwrap_or(1).clamp(1, 6) as usize;
                format!("{} {spanned}", "#".repeat(level))
            }
            TextRole::ListItem => match self.list_kind.unwrap_or(ListKind::Bullet) {
                ListKind::Bullet => format!("- {spanned}"),
                ListKind::Ordered => format!("1. {spanned}"),
            },
            TextRole::Blockquote => spanned
                .lines()
                .map(|line| format!("> {line}"))
                .collect::<Vec<_>>()
                .join("\n"),
            TextRole::CodeBlock => {
                let lang = self.code_lang.as_deref().unwrap_or("");
                format!("```{lang}\n{body}\n```")
            }
            TextRole::Table => {
                if let Some(table) = &self.table {
                    render_table_markdown(table)
                } else {
                    format!("```tsv\n{body}\n```")
                }
            }
            TextRole::Math => format!("$$\n{body}\n$$"),
            TextRole::Paragraph => spanned,
        }
    }
}

fn validate_spans(body: &str, spans: &[InlineSpan]) -> Result<()> {
    let body_len = u32::try_from(body.len()).unwrap_or(u32::MAX);
    for span in spans {
        if span.start >= span.end {
            return Err(TesError::InvalidTextHeader {
                message: format!("empty or inverted span {}..{}", span.start, span.end),
            });
        }
        if span.end > body_len {
            return Err(TesError::InvalidTextHeader {
                message: format!(
                    "span {}..{} out of bounds for body length {body_len}",
                    span.start, span.end
                ),
            });
        }
        if !body.is_char_boundary(span.start as usize) || !body.is_char_boundary(span.end as usize)
        {
            return Err(TesError::InvalidTextHeader {
                message: format!(
                    "span {}..{} is not on a UTF-8 character boundary",
                    span.start, span.end
                ),
            });
        }
        if let InlineKind::Math { tex } = &span.kind
            && tex.is_empty()
        {
            return Err(TesError::InvalidTextHeader {
                message: "inline math tex must be non-empty".into(),
            });
        }
    }
    let mut ordered: Vec<&InlineSpan> = spans.iter().collect();
    ordered.sort_by_key(|s| (s.start, s.end));
    let mut stack: Vec<&InlineSpan> = Vec::new();
    for span in ordered {
        while stack.last().is_some_and(|outer| outer.end <= span.start) {
            stack.pop();
        }
        if let Some(outer) = stack.last()
            && span.end > outer.end
        {
            return Err(TesError::InvalidTextHeader {
                message: format!(
                    "span {}..{} partially overlaps {}..{}",
                    span.start, span.end, outer.start, outer.end
                ),
            });
        }
        stack.push(span);
    }
    Ok(())
}

fn apply_spans_markdown(
    body: &str,
    spans: &[InlineSpan],
    links: &[crate::catalog::LinkEntry],
) -> String {
    if spans.is_empty() {
        return body.to_owned();
    }
    let mut by_start: Vec<&InlineSpan> = spans.iter().collect();
    by_start.sort_by_key(|s| std::cmp::Reverse(s.start));
    let mut out = body.to_owned();
    for span in by_start {
        let start = span.start as usize;
        let end = span.end as usize;
        if end > out.len() || start > end {
            continue;
        }
        let inner = out[start..end].to_owned();
        let wrapped = match &span.kind {
            InlineKind::Emphasis | InlineKind::Term => format!("*{inner}*"),
            InlineKind::Strong => format!("**{inner}**"),
            InlineKind::Code => format!("`{inner}`"),
            InlineKind::Quote => format!("\u{201c}{inner}\u{201d}"),
            InlineKind::Math { tex } => format!("${tex}$"),
            InlineKind::Link { link_id } => match links.get(*link_id as usize) {
                Some(entry) => {
                    let dest = entry.target.markdown_destination();
                    format!("[{inner}]({dest})")
                }
                None => inner,
            },
            InlineKind::Citation { .. } => inner,
        };
        out.replace_range(start..end, &wrapped);
    }
    out
}

fn render_table_markdown(table: &TableData) -> String {
    if table.rows.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for (i, row) in table.rows.iter().enumerate() {
        out.push('|');
        for cell in &row.cells {
            out.push(' ');
            out.push_str(cell.text.replace('|', "\\|").trim());
            out.push_str(" |");
        }
        out.push('\n');
        if i == 0 {
            out.push('|');
            for _ in &row.cells {
                out.push_str(" --- |");
            }
            out.push('\n');
        }
    }
    out
}

/// Cite chunk JSON payload (type `4`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CitePayload {
    /// Quoted text.
    pub quote: String,
    /// Target document UUID string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_doc_id: Option<String>,
    /// Target chunk id (`0` / absent = whole document).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_chunk_id: Option<u64>,
    /// Inclusive/exclusive byte range on the target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_byte_start: Option<u32>,
    /// Exclusive end of the target byte range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_byte_end: Option<u32>,
    /// Citation label (e.g. `Smith2024`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Optional page number from an imported PDF.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Optional bibliographic source (interchange metadata; not a display style).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<BibEntry>,
}

impl CitePayload {
    /// Parse a cite payload from UTF-8 JSON bytes.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::Json`] on malformed JSON, or validation errors from
    /// [`Self::validate`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let cite: Self = serde_json::from_slice(bytes)?;
        cite.validate()?;
        Ok(cite)
    }

    /// Serialize to UTF-8 JSON bytes after validation.
    ///
    /// # Errors
    ///
    /// Returns validation errors from [`Self::validate`], or [`TesError::Json`]
    /// if serialization fails.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        Ok(serde_json::to_vec(self)?)
    }

    /// Reject inconsistent ranges and malformed target UUIDs.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::InvalidCite`] for inverted byte ranges or an empty
    /// `source.cite_key`, or [`TesError::InvalidDocId`] if `target_doc_id` is
    /// not a UUID.
    pub fn validate(&self) -> Result<()> {
        if let (Some(start), Some(end)) = (self.target_byte_start, self.target_byte_end)
            && start >= end
        {
            return Err(TesError::InvalidCite {
                message: format!("target_byte_start ({start}) must be < target_byte_end ({end})"),
            });
        }
        if let Some(doc_id) = self.target_doc_id.as_deref()
            && Uuid::parse_str(doc_id).is_err()
        {
            return Err(TesError::InvalidDocId {
                value: doc_id.to_owned(),
            });
        }
        if let Some(source) = &self.source
            && source.cite_key.trim().is_empty()
        {
            return Err(TesError::InvalidCite {
                message: "source.cite_key must be non-empty".into(),
            });
        }
        Ok(())
    }
}

/// Frame `u32 LE length(head) | head | tail` (text headers, attachment meta, …).
///
/// # Panics
///
/// Panics if `head.len()` does not fit in `u32` (not reachable for Tessera
/// payload sizes).
#[must_use]
pub fn encode_u32_prefixed(head: &[u8], tail: &[u8]) -> Vec<u8> {
    let len = u32::try_from(head.len()).expect("prefixed head exceeds u32::MAX");
    let mut out = Vec::with_capacity(4 + head.len() + tail.len());
    let mut len_buf = [0u8; 4];
    {
        let mut w = LeWriter::new(&mut len_buf);
        w.put_u32(len);
    }
    out.extend_from_slice(&len_buf);
    out.extend_from_slice(head);
    out.extend_from_slice(tail);
    out
}

/// Split a [`encode_u32_prefixed`] buffer into `(head, tail)`.
///
/// # Errors
///
/// Returns [`TesError::BufferTooSmall`] when the buffer is truncated.
pub fn split_u32_prefixed<'a>(
    bytes: &'a [u8],
    structure: &'static str,
) -> Result<(&'a [u8], &'a [u8])> {
    let mut r = LeReader::require(bytes, structure, 4)?;
    let head_len = r.take_u32() as usize;
    let rest = &bytes[4..];
    if rest.len() < head_len {
        return Err(TesError::BufferTooSmall {
            structure,
            need: head_len,
            got: rest.len(),
        });
    }
    Ok((&rest[..head_len], &rest[head_len..]))
}

fn ensure_text_header_size(len: usize) -> Result<()> {
    if len > TEXT_HEADER_MAX_BYTES {
        Err(TesError::TextHeaderTooLarge {
            len,
            limit: TEXT_HEADER_MAX_BYTES,
        })
    } else {
        Ok(())
    }
}

/// Encode a text chunk payload: `u32 header_len | header JSON | UTF-8 body`.
///
/// # Errors
///
/// Returns validation errors from [`TextHeader::validate`], [`TesError::Json`]
/// if the header cannot be serialized, or [`TesError::TextHeaderTooLarge`] if
/// it exceeds [`TEXT_HEADER_MAX_BYTES`].
pub fn encode_text_payload(header: &TextHeader, body: &str) -> Result<Vec<u8>> {
    header.validate(body)?;
    let header_bytes = serde_json::to_vec(header)?;
    ensure_text_header_size(header_bytes.len())?;
    Ok(encode_u32_prefixed(&header_bytes, body.as_bytes()))
}

/// Decode a text chunk payload into `(header, body)`.
///
/// # Errors
///
/// Returns [`TesError::BufferTooSmall`] if the buffer is truncated,
/// [`TesError::TextHeaderTooLarge`] if the header exceeds
/// [`TEXT_HEADER_MAX_BYTES`], [`TesError::Json`] for a bad header,
/// [`TesError::InvalidUtf8`] if the body is not UTF-8, or validation errors
/// from [`TextHeader::validate`].
pub fn decode_text_payload(bytes: &[u8]) -> Result<(TextHeader, String)> {
    let (header_bytes, body_bytes) = split_u32_prefixed(bytes, "TextChunkPayload")?;
    ensure_text_header_size(header_bytes.len())?;
    let header: TextHeader = serde_json::from_slice(header_bytes)?;
    let body = std::str::from_utf8(body_bytes)
        .map_err(|_| TesError::InvalidUtf8 {
            structure: "TextChunkPayload.body",
        })?
        .to_owned();
    header.validate(&body)?;
    Ok((header, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_payload_round_trip() {
        let header = TextHeader::paragraph();
        let body = "We measured …";
        let bytes = encode_text_payload(&header, body).unwrap();
        let (h2, b2) = decode_text_payload(&bytes).unwrap();
        assert_eq!(h2, header);
        assert_eq!(b2, body);
    }

    #[test]
    fn heading_payload_round_trip() {
        let header = TextHeader::heading(2);
        let bytes = encode_text_payload(&header, "Methods").unwrap();
        let (h2, b2) = decode_text_payload(&bytes).unwrap();
        assert_eq!(h2.role, TextRole::Heading);
        assert_eq!(h2.level, Some(2));
        assert_eq!(b2, "Methods");
    }

    #[test]
    fn spans_math_table_round_trip() {
        let body = "alpha beta";
        let mut header = TextHeader::paragraph();
        header.lang = Some("en".into());
        header.align = Some(TextAlign::Start);
        header.spans = vec![InlineSpan {
            start: 0,
            end: 5,
            kind: InlineKind::Emphasis,
        }];
        let bytes = encode_text_payload(&header, body).unwrap();
        let (h2, b2) = decode_text_payload(&bytes).unwrap();
        assert_eq!(h2, header);
        assert_eq!(b2, body);

        let math = TextHeader::math();
        let mbytes = encode_text_payload(&math, "E = mc^2").unwrap();
        let (mh, mb) = decode_text_payload(&mbytes).unwrap();
        assert_eq!(mh.role, TextRole::Math);
        assert_eq!(mb, "E = mc^2");

        let table = TextHeader::table(TableData {
            rows: vec![
                TableRow {
                    cells: vec![
                        TableCell {
                            text: "A".into(),
                            spans: Vec::new(),
                            align: None,
                            is_header: true,
                            rowspan: None,
                            colspan: None,
                        },
                        TableCell {
                            text: "B".into(),
                            spans: Vec::new(),
                            align: Some(TextAlign::Center),
                            is_header: true,
                            rowspan: None,
                            colspan: None,
                        },
                    ],
                },
                TableRow {
                    cells: vec![
                        TableCell {
                            text: "1".into(),
                            spans: Vec::new(),
                            align: None,
                            is_header: false,
                            rowspan: None,
                            colspan: None,
                        },
                        TableCell {
                            text: "2".into(),
                            spans: Vec::new(),
                            align: None,
                            is_header: false,
                            rowspan: None,
                            colspan: None,
                        },
                    ],
                },
            ],
        });
        let tbytes = encode_text_payload(&table, "").unwrap();
        let (th, tb) = decode_text_payload(&tbytes).unwrap();
        assert_eq!(th.table.as_ref().unwrap().rows.len(), 2);
        assert!(tb.is_empty());
    }

    #[test]
    fn rejects_out_of_bounds_span() {
        let mut header = TextHeader::paragraph();
        header.spans = vec![InlineSpan {
            start: 0,
            end: 99,
            kind: InlineKind::Strong,
        }];
        assert!(encode_text_payload(&header, "hi").is_err());
    }

    #[test]
    fn cite_payload_round_trip() {
        let cite = CitePayload {
            quote: "We measured …".into(),
            target_doc_id: Some("660e8400-e29b-41d4-a716-446655440001".into()),
            target_chunk_id: Some(12),
            target_byte_start: Some(0),
            target_byte_end: Some(42),
            label: Some("Smith2024".into()),
            page: Some(7),
            source: None,
        };
        let decoded = CitePayload::from_bytes(&cite.to_bytes().unwrap()).unwrap();
        assert_eq!(decoded, cite);
    }

    #[test]
    fn cite_rejects_inverted_byte_range() {
        let cite = CitePayload {
            quote: String::new(),
            target_doc_id: None,
            target_chunk_id: None,
            target_byte_start: Some(10),
            target_byte_end: Some(10),
            label: None,
            page: None,
            source: None,
        };
        assert!(cite.validate().is_err());
    }
}
