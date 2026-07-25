//! Chunk payload codecs (`docs/layout_v0.md` — *Text chunk payload*).

use serde::{Deserialize, Serialize};

use crate::error::{Result, TesError};
use crate::wire::{LeReader, LeWriter};

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
    /// Monospace block.
    CodeBlock,
    /// Table (v0: TSV body).
    Table,
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
    /// Emphasis spans (v0: usually empty; structure lives in fields).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emphasis: Vec<String>,
    /// Theme hints imported from semantic HTML `class` attributes.
    #[serde(default, rename = "class", skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<String>,
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
        }
    }
}

impl TextHeader {
    /// A plain paragraph header.
    #[must_use]
    pub fn paragraph() -> Self {
        Self {
            role: TextRole::Paragraph,
            level: None,
            list_kind: None,
            emphasis: Vec::new(),
            classes: Vec::new(),
        }
    }

    /// A heading header at `level` (1–6).
    #[must_use]
    pub fn heading(level: u32) -> Self {
        Self {
            role: TextRole::Heading,
            level: Some(level),
            list_kind: None,
            emphasis: Vec::new(),
            classes: Vec::new(),
        }
    }

    /// A list-item header.
    #[must_use]
    pub fn list_item(kind: ListKind) -> Self {
        Self {
            role: TextRole::ListItem,
            level: None,
            list_kind: Some(kind),
            emphasis: Vec::new(),
            classes: Vec::new(),
        }
    }
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
}

impl CitePayload {
    /// Parse a cite payload from UTF-8 JSON bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(serde_json::from_slice(bytes)?)
    }
}

/// Encode a text chunk payload: `u32 header_len | header JSON | UTF-8 body`.
pub fn encode_text_payload(header: &TextHeader, body: &str) -> Result<Vec<u8>> {
    let header_bytes = serde_json::to_vec(header)?;
    if header_bytes.len() > TEXT_HEADER_MAX_BYTES {
        return Err(TesError::TextHeaderTooLarge {
            len: header_bytes.len(),
            limit: TEXT_HEADER_MAX_BYTES,
        });
    }
    let header_len =
        u32::try_from(header_bytes.len()).expect("header length checked against 4 KiB");
    let mut out = Vec::with_capacity(4 + header_bytes.len() + body.len());
    let mut len_buf = [0u8; 4];
    {
        let mut w = LeWriter::new(&mut len_buf);
        w.put_u32(header_len);
    }
    out.extend_from_slice(&len_buf);
    out.extend_from_slice(&header_bytes);
    out.extend_from_slice(body.as_bytes());
    Ok(out)
}

/// Decode a text chunk payload into `(header, body)`.
pub fn decode_text_payload(bytes: &[u8]) -> Result<(TextHeader, String)> {
    let mut r = LeReader::require(bytes, "TextChunkPayload", 4)?;
    let header_len = r.take_u32() as usize;
    let rest = &bytes[4..];
    if rest.len() < header_len {
        return Err(TesError::BufferTooSmall {
            structure: "TextChunkPayload.header",
            need: header_len,
            got: rest.len(),
        });
    }
    let header: TextHeader = serde_json::from_slice(&rest[..header_len])?;
    let body = std::str::from_utf8(&rest[header_len..])
        .map_err(|_| TesError::InvalidUtf8 {
            structure: "TextChunkPayload.body",
        })?
        .to_owned();
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
}
