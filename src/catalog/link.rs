//! Cross-document link table (`TLNK`).
//!
//! The v0 wire row is 48 bytes: `8 + 4 + 4 + 16 + 8 + 4 + 4`.

use uuid::Uuid;

use crate::error::{Result, TesError};
use crate::wire::{LeReader, LeWriter};

/// Magic tag at the start of a link table.
pub const MAGIC: [u8; 4] = *b"TLNK";
/// Only link-table version understood by this build.
pub const TABLE_VERSION: u32 = 0;
/// Encoded header size.
pub const HEADER_LEN: usize = 24;
/// Encoded entry size.
pub const ENTRY_LEN: usize = 48;

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

/// One fixed cross-document edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkEntry {
    /// Chunk containing the source anchor.
    pub source_chunk_id: u64,
    /// UTF-8 anchor start in the source body.
    pub source_byte_start: u32,
    /// Exclusive UTF-8 anchor end.
    pub source_byte_end: u32,
    /// RFC 4122 target UUID bytes.
    pub target_doc_id: [u8; 16],
    /// Target chunk (`0` = whole document).
    pub target_chunk_id: u64,
    /// Link semantics.
    pub link_kind: LinkKind,
}

impl LinkEntry {
    /// Create an entry from a UUID.
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
            target_doc_id: *target_doc_id.as_bytes(),
            target_chunk_id,
            link_kind,
        }
    }

    /// Target document as a UUID.
    #[must_use]
    pub const fn target_uuid(self) -> Uuid {
        Uuid::from_bytes(self.target_doc_id)
    }

    /// Encode exactly 48 bytes.
    #[must_use]
    pub fn to_bytes(self) -> [u8; ENTRY_LEN] {
        let mut out = [0u8; ENTRY_LEN];
        let mut w = LeWriter::new(&mut out);
        w.put_u64(self.source_chunk_id);
        w.put_u32(self.source_byte_start);
        w.put_u32(self.source_byte_end);
        w.put_bytes(&self.target_doc_id);
        w.put_u64(self.target_chunk_id);
        w.put_u32(self.link_kind.as_u32());
        w.put_zeros(4);
        debug_assert_eq!(w.position(), ENTRY_LEN);
        out
    }

    /// Decode one fixed row.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut r = LeReader::require(bytes, "LinkEntry", ENTRY_LEN)?;
        let source_chunk_id = r.take_u64();
        let source_byte_start = r.take_u32();
        let source_byte_end = r.take_u32();
        let target_doc_id = r.take_16();
        let target_chunk_id = r.take_u64();
        let link_kind = LinkKind::from_u32(r.take_u32())?;
        r.skip(4);
        Ok(Self {
            source_chunk_id,
            source_byte_start,
            source_byte_end,
            target_doc_id,
            target_chunk_id,
            link_kind,
        })
    }
}

/// Encode a complete `TLNK` region.
#[must_use]
pub fn encode_link_table(entries: &[LinkEntry]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + entries.len() * ENTRY_LEN);
    let mut header = [0u8; HEADER_LEN];
    let mut w = LeWriter::new(&mut header);
    w.put_bytes(&MAGIC);
    w.put_u32(TABLE_VERSION);
    w.put_u64(entries.len() as u64);
    w.put_zeros(8);
    out.extend_from_slice(&header);
    for entry in entries {
        out.extend_from_slice(&entry.to_bytes());
    }
    out
}

/// Decode a complete `TLNK` region.
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
    if version != TABLE_VERSION {
        return Err(TesError::UnsupportedVersion {
            structure: "LinkTableHeader",
            found: version,
            supported: TABLE_VERSION,
        });
    }
    let count = r.take_u64();
    r.skip(8);
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
        entries.push(LinkEntry::from_bytes(&bytes[start..])?);
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_round_trip_uses_48_byte_rows() {
        let target = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let entry = LinkEntry::new(3, 5, 12, target, 8, LinkKind::Wiki);
        let bytes = encode_link_table(&[entry]);
        assert_eq!(bytes.len(), 24 + 48);
        assert_eq!(&bytes[..4], b"TLNK");
        assert_eq!(read_link_table(&bytes).unwrap(), vec![entry]);
    }
}
