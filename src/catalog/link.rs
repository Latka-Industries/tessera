//! Cross-document link table (`TLNK`).
//!
//! v0 / all-internal tables: header version `0`, fixed 48-byte rows only
//! (`8 + 4 + 4 + 16 + 8 + 4 + 4`). Existing goldens stay byte-identical.
//!
//! v1 (external / attachment targets): header version `1`, same 48-byte rows
//! plus a trailing UTF-8 URI heap. External URI bytes live in the heap, not in
//! the fixed row (`docs/structure_v1.md`).

use uuid::Uuid;

use crate::error::{Result, TesError};
use argus::{LeReader, LeWriter};

/// Magic tag at the start of a link table.
pub const MAGIC: [u8; 4] = *b"TLNK";
/// All-internal tables (byte-compatible with historical fixtures).
pub const TABLE_VERSION_V0: u32 = 0;
/// Tables that may include external URI / attachment targets + URI heap.
pub const TABLE_VERSION_V1: u32 = 1;
/// Encoded header size.
pub const HEADER_LEN: usize = 24;
/// Encoded entry size.
pub const ENTRY_LEN: usize = 48;
/// Soft upper bound on a single external URI.
pub const URI_MAX_BYTES: usize = 8 * 1024;
/// Soft upper bound on aggregate URI heap size.
pub const URI_HEAP_MAX_BYTES: usize = 256 * 1024;

const ALLOWED_SCHEMES: &[&str] = &["http", "https", "mailto"];

/// Pending outbound link discovered during Markdown/HTML/Tessprek parse.
///
/// Anchors are UTF-8 byte offsets into the clean text body; the destination is
/// either an allowed external URI or an internal document UUID string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundLink {
    /// Inclusive start byte offset in the body.
    pub start: u32,
    /// Exclusive end byte offset in the body.
    pub end: u32,
    /// Destination URI or document UUID.
    pub dest: String,
}

impl OutboundLink {
    /// Build a [`LinkEntry`] for `source_chunk_id` from this pending edge.
    ///
    /// Prefer external URI validation; fall back to UUID internal targets.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::InvalidLink`] / [`TesError::InvalidDocId`] when `dest`
    /// is neither an allowed URI nor a UUID.
    pub fn into_entry(self, source_chunk_id: u64, link_kind: LinkKind) -> Result<LinkEntry> {
        if let Some(rest) = self.dest.strip_prefix("attachment:") {
            let chunk_id = rest.parse::<u64>().map_err(|_| TesError::InvalidLink {
                message: format!("invalid attachment destination: {}", self.dest),
            })?;
            return LinkEntry::attachment(
                source_chunk_id,
                self.start,
                self.end,
                chunk_id,
                link_kind,
            );
        }
        if validate_external_uri(&self.dest).is_ok() {
            return LinkEntry::external(
                source_chunk_id,
                self.start,
                self.end,
                self.dest,
                link_kind,
            );
        }
        let uuid = Uuid::parse_str(self.dest.trim()).map_err(|_| TesError::InvalidLink {
            message: format!(
                "link destination is neither an allowed URI nor a UUID: {}",
                self.dest
            ),
        })?;
        Ok(LinkEntry::new(
            source_chunk_id,
            self.start,
            self.end,
            uuid,
            0,
            link_kind,
        ))
    }
}

/// Semantic link kind stored in a [`LinkEntry`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum LinkKind {
    /// Wiki / document graph edge.
    Wiki = 0,
    /// Footnote target.
    Footnote = 1,
    /// Citation stub.
    Citation = 2,
}

impl LinkKind {
    /// Decode a wire discriminant.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::InvalidEnum`] if `value` is not a known link kind.
    pub fn from_u32(value: u32) -> Result<Self> {
        match value {
            0 => Ok(Self::Wiki),
            1 => Ok(Self::Footnote),
            2 => Ok(Self::Citation),
            other => Err(TesError::InvalidEnum {
                field: "link_kind",
                value: other,
            }),
        }
    }

    /// Wire discriminant.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    /// Stable lowercase name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wiki => "wiki",
            Self::Footnote => "footnote",
            Self::Citation => "citation",
        }
    }
}

/// Typed link target (`docs/structure_v1.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// Internal document / optional chunk.
    Internal {
        /// Target document UUID.
        doc_id: Uuid,
        /// Target chunk (`0` = whole document).
        chunk_id: u64,
    },
    /// External URI (http/https/mailto).
    External {
        /// Sanitized URI string.
        uri: String,
    },
    /// Attachment chunk in this file.
    Attachment {
        /// Attachment chunk id.
        chunk_id: u64,
    },
}

impl LinkTarget {
    /// Wire `target_kind` discriminant.
    #[must_use]
    pub const fn kind_u32(&self) -> u32 {
        match self {
            Self::Internal { .. } => 0,
            Self::External { .. } => 1,
            Self::Attachment { .. } => 2,
        }
    }

    /// Destination string for Markdown / Tessprek `[text](dest)` syntax.
    ///
    /// Internal targets use the bare document UUID; attachments use
    /// `attachment:{chunk_id}` (round-trips via [`OutboundLink::into_entry`]).
    #[must_use]
    pub fn markdown_destination(&self) -> String {
        match self {
            Self::External { uri } => uri.clone(),
            Self::Internal { doc_id, .. } => doc_id.to_string(),
            Self::Attachment { chunk_id } => format!("attachment:{chunk_id}"),
        }
    }

    /// `href` attribute value for HTML export (caller escapes).
    #[must_use]
    pub fn html_href(&self) -> String {
        match self {
            Self::External { uri } => uri.clone(),
            Self::Internal { doc_id, .. } => format!("tes://{doc_id}"),
            Self::Attachment { chunk_id } => format!("/attachment/{chunk_id}"),
        }
    }
}

/// One cross-document (or external / attachment) edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkEntry {
    /// Chunk containing the source anchor.
    pub source_chunk_id: u64,
    /// UTF-8 anchor start in the source body.
    pub source_byte_start: u32,
    /// Exclusive UTF-8 anchor end.
    pub source_byte_end: u32,
    /// Typed destination.
    pub target: LinkTarget,
    /// Link semantics.
    pub link_kind: LinkKind,
}

impl LinkEntry {
    /// Create an internal UUID-targeted entry (v0-compatible).
    #[must_use]
    pub fn new(
        source_chunk_id: u64,
        source_byte_start: u32,
        source_byte_end: u32,
        target_doc_id: Uuid,
        target_chunk_id: u64,
        link_kind: LinkKind,
    ) -> Self {
        Self {
            source_chunk_id,
            source_byte_start,
            source_byte_end,
            target: LinkTarget::Internal {
                doc_id: target_doc_id,
                chunk_id: target_chunk_id,
            },
            link_kind,
        }
    }

    /// Create an external URI entry.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::InvalidLink`] if the URI fails scheme/length checks.
    pub fn external(
        source_chunk_id: u64,
        source_byte_start: u32,
        source_byte_end: u32,
        uri: impl Into<String>,
        link_kind: LinkKind,
    ) -> Result<Self> {
        let uri = uri.into();
        validate_external_uri(&uri)?;
        Ok(Self {
            source_chunk_id,
            source_byte_start,
            source_byte_end,
            target: LinkTarget::External { uri },
            link_kind,
        })
    }

    /// Create an attachment-targeted entry.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::InvalidLink`] if `attachment_chunk_id` is zero.
    pub fn attachment(
        source_chunk_id: u64,
        source_byte_start: u32,
        source_byte_end: u32,
        attachment_chunk_id: u64,
        link_kind: LinkKind,
    ) -> Result<Self> {
        if attachment_chunk_id == 0 {
            return Err(TesError::InvalidLink {
                message: "attachment target chunk id must be non-zero".into(),
            });
        }
        Ok(Self {
            source_chunk_id,
            source_byte_start,
            source_byte_end,
            target: LinkTarget::Attachment {
                chunk_id: attachment_chunk_id,
            },
            link_kind,
        })
    }

    /// Internal target document UUID, when applicable.
    #[must_use]
    pub fn target_uuid(&self) -> Option<Uuid> {
        match self.target {
            LinkTarget::Internal { doc_id, .. } => Some(doc_id),
            _ => None,
        }
    }

    /// Internal / attachment target chunk id (`0` / `None` when not applicable).
    #[must_use]
    pub fn target_chunk_id(&self) -> Option<u64> {
        match self.target {
            LinkTarget::Internal { chunk_id, .. } | LinkTarget::Attachment { chunk_id } => {
                Some(chunk_id)
            }
            LinkTarget::External { .. } => None,
        }
    }

    /// External URI, when this edge is external.
    #[must_use]
    pub fn external_uri(&self) -> Option<&str> {
        match &self.target {
            LinkTarget::External { uri } => Some(uri.as_str()),
            _ => None,
        }
    }

    fn encode_row(&self, uri_offset: u32, uri_len: u32) -> [u8; ENTRY_LEN] {
        let mut out = [0u8; ENTRY_LEN];
        let mut w = LeWriter::new(&mut out);
        w.put_u64(self.source_chunk_id);
        w.put_u32(self.source_byte_start);
        w.put_u32(self.source_byte_end);
        match &self.target {
            LinkTarget::Internal { doc_id, chunk_id } => {
                w.put_bytes(doc_id.as_bytes());
                w.put_u64(*chunk_id);
                w.put_u32(self.link_kind.as_u32());
                w.put_u32(0); // target_kind = Internal
            }
            LinkTarget::External { .. } => {
                let mut id = [0u8; 16];
                id[..4].copy_from_slice(&uri_offset.to_le_bytes());
                id[4..8].copy_from_slice(&uri_len.to_le_bytes());
                w.put_bytes(&id);
                w.put_u64(0);
                w.put_u32(self.link_kind.as_u32());
                w.put_u32(1); // External
            }
            LinkTarget::Attachment { chunk_id } => {
                w.put_bytes(&[0u8; 16]);
                w.put_u64(*chunk_id);
                w.put_u32(self.link_kind.as_u32());
                w.put_u32(2); // Attachment
            }
        }
        debug_assert_eq!(w.position(), ENTRY_LEN);
        out
    }
}

/// Validate an external URI for inert storage / export.
///
/// # Errors
///
/// Returns [`TesError::InvalidLink`] for empty, oversized, or disallowed schemes.
pub fn validate_external_uri(uri: &str) -> Result<()> {
    let trimmed = uri.trim();
    if trimmed.is_empty() {
        return Err(TesError::InvalidLink {
            message: "external URI must be non-empty".into(),
        });
    }
    if trimmed.len() > URI_MAX_BYTES {
        return Err(TesError::InvalidLink {
            message: format!("external URI exceeds {URI_MAX_BYTES} bytes"),
        });
    }
    if trimmed.contains('\0') || trimmed.chars().any(char::is_control) {
        return Err(TesError::InvalidLink {
            message: "external URI must not contain control characters".into(),
        });
    }
    let scheme = trimmed
        .split_once(':')
        .map(|(s, _)| s.to_ascii_lowercase())
        .ok_or_else(|| TesError::InvalidLink {
            message: "external URI must include a scheme".into(),
        })?;
    if !ALLOWED_SCHEMES.iter().any(|s| *s == scheme) {
        return Err(TesError::InvalidLink {
            message: format!("URI scheme '{scheme}' is not allowed (permit http, https, mailto)"),
        });
    }
    Ok(())
}

/// Encode a complete `TLNK` region (v0 if all-internal, else v1 + URI heap).
#[must_use]
pub fn encode_link_table(entries: &[LinkEntry]) -> Vec<u8> {
    let needs_v1 = entries
        .iter()
        .any(|e| !matches!(e.target, LinkTarget::Internal { .. }));
    if needs_v1 {
        encode_link_table_v1(entries)
    } else {
        encode_link_table_v0(entries)
    }
}

fn encode_link_table_v0(entries: &[LinkEntry]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + entries.len() * ENTRY_LEN);
    let mut header = [0u8; HEADER_LEN];
    let mut w = LeWriter::new(&mut header);
    w.put_bytes(&MAGIC);
    w.put_u32(TABLE_VERSION_V0);
    w.put_u64(entries.len() as u64);
    w.put_zeros(8);
    out.extend_from_slice(&header);
    for entry in entries {
        out.extend_from_slice(&entry.encode_row(0, 0));
    }
    out
}

fn encode_link_table_v1(entries: &[LinkEntry]) -> Vec<u8> {
    let mut heap = Vec::new();
    let mut rows = Vec::with_capacity(entries.len());
    for entry in entries {
        let (off, len) = match &entry.target {
            LinkTarget::External { uri } => {
                let offset = u32::try_from(heap.len()).unwrap_or(u32::MAX);
                let bytes = uri.as_bytes();
                let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
                heap.extend_from_slice(bytes);
                (offset, len)
            }
            _ => (0, 0),
        };
        rows.push(entry.encode_row(off, len));
    }
    let mut out = Vec::with_capacity(HEADER_LEN + rows.len() * ENTRY_LEN + heap.len());
    let mut header = [0u8; HEADER_LEN];
    let mut w = LeWriter::new(&mut header);
    w.put_bytes(&MAGIC);
    w.put_u32(TABLE_VERSION_V1);
    w.put_u64(entries.len() as u64);
    w.put_zeros(8);
    out.extend_from_slice(&header);
    for row in &rows {
        out.extend_from_slice(row);
    }
    out.extend_from_slice(&heap);
    out
}

/// Decode a complete `TLNK` region (v0 or v1).
///
/// # Errors
///
/// Returns header / length / entry / URI validation errors.
pub fn read_link_table(bytes: &[u8]) -> Result<Vec<LinkEntry>> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let mut r = LeReader::require(bytes, "LinkTableHeader", HEADER_LEN)?;
    let magic = r.take_4();
    if magic != MAGIC {
        return Err(TesError::BadMagic {
            structure: "LinkTableHeader",
            expected: MAGIC,
            found: magic,
        });
    }
    let version = r.take_u32();
    let count = r.take_u64();
    r.skip(8);

    match version {
        TABLE_VERSION_V0 => {
            let expected = HEADER_LEN as u64 + count * ENTRY_LEN as u64;
            if bytes.len() as u64 != expected {
                return Err(TesError::LinkTableLengthMismatch {
                    expected,
                    got: bytes.len() as u64,
                });
            }
            let mut entries = Vec::with_capacity(count as usize);
            for i in 0..count as usize {
                let start = HEADER_LEN + i * ENTRY_LEN;
                entries.push(decode_row_v0(&bytes[start..start + ENTRY_LEN])?);
            }
            Ok(entries)
        }
        TABLE_VERSION_V1 => {
            let rows_end = HEADER_LEN as u64 + count * ENTRY_LEN as u64;
            if (bytes.len() as u64) < rows_end {
                return Err(TesError::LinkTableLengthMismatch {
                    expected: rows_end,
                    got: bytes.len() as u64,
                });
            }
            let heap = &bytes[rows_end as usize..];
            if heap.len() > URI_HEAP_MAX_BYTES {
                return Err(TesError::InvalidLink {
                    message: format!("URI heap {} bytes exceeds {URI_HEAP_MAX_BYTES}", heap.len()),
                });
            }
            let mut entries = Vec::with_capacity(count as usize);
            for i in 0..count as usize {
                let start = HEADER_LEN + i * ENTRY_LEN;
                entries.push(decode_row_v1(&bytes[start..start + ENTRY_LEN], heap)?);
            }
            Ok(entries)
        }
        other => Err(TesError::UnsupportedVersion {
            structure: "LinkTableHeader",
            found: other,
            supported: TABLE_VERSION_V1,
        }),
    }
}

fn decode_row_v0(bytes: &[u8]) -> Result<LinkEntry> {
    let mut r = LeReader::require(bytes, "LinkEntry", ENTRY_LEN)?;
    let source_chunk_id = r.take_u64();
    let source_byte_start = r.take_u32();
    let source_byte_end = r.take_u32();
    let target_doc_id = r.take_16();
    let target_chunk_id = r.take_u64();
    let link_kind = LinkKind::from_u32(r.take_u32())?;
    let reserved = r.take_u32();
    if reserved != 0 {
        return Err(TesError::InvalidLink {
            message: format!("v0 TLNK reserved field must be 0, got {reserved}"),
        });
    }
    Ok(LinkEntry {
        source_chunk_id,
        source_byte_start,
        source_byte_end,
        target: LinkTarget::Internal {
            doc_id: Uuid::from_bytes(target_doc_id),
            chunk_id: target_chunk_id,
        },
        link_kind,
    })
}

fn decode_row_v1(bytes: &[u8], heap: &[u8]) -> Result<LinkEntry> {
    let mut r = LeReader::require(bytes, "LinkEntry", ENTRY_LEN)?;
    let source_chunk_id = r.take_u64();
    let source_byte_start = r.take_u32();
    let source_byte_end = r.take_u32();
    let target_doc_id = r.take_16();
    let target_chunk_id = r.take_u64();
    let link_kind = LinkKind::from_u32(r.take_u32())?;
    let target_kind = r.take_u32();
    let target = match target_kind {
        0 => LinkTarget::Internal {
            doc_id: Uuid::from_bytes(target_doc_id),
            chunk_id: target_chunk_id,
        },
        1 => {
            let offset = u32::from_le_bytes(target_doc_id[0..4].try_into().unwrap()) as usize;
            let len = u32::from_le_bytes(target_doc_id[4..8].try_into().unwrap()) as usize;
            if offset.checked_add(len).is_none_or(|end| end > heap.len()) {
                return Err(TesError::InvalidLink {
                    message: format!(
                        "external URI slice {offset}..{} out of heap len {}",
                        offset.saturating_add(len),
                        heap.len()
                    ),
                });
            }
            let uri = std::str::from_utf8(&heap[offset..offset + len]).map_err(|_| {
                TesError::InvalidUtf8 {
                    structure: "LinkEntry.external_uri",
                }
            })?;
            validate_external_uri(uri)?;
            LinkTarget::External {
                uri: uri.to_owned(),
            }
        }
        2 => {
            if target_chunk_id == 0 {
                return Err(TesError::InvalidLink {
                    message: "attachment target chunk id must be non-zero".into(),
                });
            }
            LinkTarget::Attachment {
                chunk_id: target_chunk_id,
            }
        }
        other => {
            return Err(TesError::InvalidEnum {
                field: "link_target_kind",
                value: other,
            });
        }
    };
    Ok(LinkEntry {
        source_chunk_id,
        source_byte_start,
        source_byte_end,
        target,
        link_kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_round_trip_uses_48_byte_rows() {
        let target = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let entry = LinkEntry::new(3, 5, 12, target, 8, LinkKind::Wiki);
        let bytes = encode_link_table(&[entry.clone()]);
        assert_eq!(bytes.len(), 24 + 48);
        assert_eq!(&bytes[..4], b"TLNK");
        assert_eq!(bytes[4], 0); // v0
        assert_eq!(read_link_table(&bytes).unwrap(), vec![entry]);
    }

    #[test]
    fn external_uri_uses_v1_heap_and_round_trips() {
        let entry =
            LinkEntry::external(1, 0, 4, "https://example.com/path?q=1", LinkKind::Wiki).unwrap();
        let bytes = encode_link_table(&[entry.clone()]);
        assert_eq!(bytes[4], 1); // v1
        assert!(bytes.len() > 24 + 48);
        let back = read_link_table(&bytes).unwrap();
        assert_eq!(back, vec![entry]);
    }

    #[test]
    fn rejects_javascript_uri() {
        assert!(matches!(
            LinkEntry::external(1, 0, 1, "javascript:alert(1)", LinkKind::Wiki),
            Err(TesError::InvalidLink { .. })
        ));
    }
}
