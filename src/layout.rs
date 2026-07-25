//! Container layer: the fixed 64-byte superblock at offset 0.
//!
//! Wire spec: `docs/layout_v0.md` — *Superblock v0 (64 bytes)*.

use std::fs::File;
use std::path::Path;

use memmap2::Mmap;

use crate::error::{Result, TesError};
use argus::{LeReader, LeWriter};

/// Magic tag at bytes `0..4` of every `.tes` file.
pub const MAGIC: [u8; 4] = *b"TESS";

/// Only layout version understood by this build.
pub const LAYOUT_VERSION: u32 = 0;

/// Encoded size of the superblock, in bytes.
pub const SUPERBLOCK_LEN: usize = 64;

/// Superblock flag bits.
pub mod flags {
    /// An optional `THST` history footer is present at EOF.
    pub const HISTORY_FOOTER: u32 = 1;
}

/// Document kind, mirrored in the catalog JSON (`docs/layout_v0.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum DocKind {
    /// Short-form capture.
    Note = 0,
    /// Long-form prose.
    Document = 1,
    /// Fiction / chapters.
    Manuscript = 2,
    /// Papers, lit notes (+ cite chunks).
    Research = 3,
    /// Presentation (+ slide chunks).
    Deck = 4,
    /// Standalone wiki article.
    WikiPage = 5,
    /// Map-of-content / TOC index.
    Hub = 6,
    /// Vault catalog sidecar.
    Index = 7,
}

impl DocKind {
    /// Decode a `doc_kind` discriminant from the superblock.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::InvalidEnum`] if `value` is not a known document kind.
    pub fn from_u32(value: u32) -> Result<Self> {
        Ok(match value {
            0 => Self::Note,
            1 => Self::Document,
            2 => Self::Manuscript,
            3 => Self::Research,
            4 => Self::Deck,
            5 => Self::WikiPage,
            6 => Self::Hub,
            7 => Self::Index,
            other => {
                return Err(TesError::InvalidEnum {
                    field: "doc_kind",
                    value: other,
                });
            }
        })
    }

    /// The `u32` discriminant stored on disk.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    /// The lowercase string mirror used in the catalog JSON.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Document => "document",
            Self::Manuscript => "manuscript",
            Self::Research => "research",
            Self::Deck => "deck",
            Self::WikiPage => "wiki_page",
            Self::Hub => "hub",
            Self::Index => "index",
        }
    }
}

/// Points to an optional region (catalog or link table) by offset and length.
///
/// A region is absent when `length == 0` (offset is then `0` by convention).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Region {
    /// Byte offset from the start of the file. `0` when absent.
    pub offset: u64,
    /// Byte length of the region. `0` when absent.
    pub length: u64,
}

impl Region {
    /// An absent region (`offset = 0`, `length = 0`).
    pub const NONE: Region = Region {
        offset: 0,
        length: 0,
    };

    /// Create a present region.
    #[must_use]
    pub const fn new(offset: u64, length: u64) -> Self {
        Self { offset, length }
    }

    /// Whether the region carries any bytes.
    #[must_use]
    pub const fn is_present(self) -> bool {
        self.length > 0
    }

    /// Exclusive end offset (`offset + length`).
    #[must_use]
    pub const fn end(self) -> u64 {
        self.offset + self.length
    }

    /// Slice `bytes` for this region, checking bounds against `file_len`.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::OutOfBounds`] if the region extends past `bytes`.
    pub fn slice<'a>(self, bytes: &'a [u8], structure: &'static str) -> Result<&'a [u8]> {
        let file_len = bytes.len() as u64;
        if !self.is_present() {
            return Ok(&[]);
        }
        if self.end() > file_len {
            return Err(TesError::OutOfBounds {
                structure,
                offset: self.offset,
                length: self.length,
                file_len,
            });
        }
        let start = self.offset as usize;
        let end = self.end() as usize;
        Ok(&bytes[start..end])
    }
}

/// Memory-map an existing `.tes` file for read-only access.
///
/// # Errors
///
/// Returns [`TesError::Io`] if the file cannot be opened or mapped.
pub fn open_mmap(path: &Path) -> Result<Mmap> {
    let file = File::open(path)?;
    Ok(unsafe { Mmap::map(&file)? })
}

/// The fixed 64-byte header at offset 0 of a `.tes` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuperblockV0 {
    /// Bitfield of [`flags`] values.
    pub flags: u32,
    /// Document kind (also mirrored in the catalog JSON).
    pub doc_kind: DocKind,
    /// Document catalog region (JSON blob). Absent when `length == 0`.
    pub catalog: Region,
    /// Link table region (`TLNK`). Absent when `length == 0`.
    pub link_table: Region,
    /// Chunk index region (`TIDX`). Absent when `length == 0`.
    pub chunk_index: Region,
}

impl SuperblockV0 {
    /// Create a superblock for the given kind with all regions absent.
    #[must_use]
    pub fn new(doc_kind: DocKind) -> Self {
        Self {
            flags: 0,
            doc_kind,
            catalog: Region::NONE,
            link_table: Region::NONE,
            chunk_index: Region::NONE,
        }
    }

    /// Whether the `THST` history footer flag is set.
    #[must_use]
    pub const fn has_history_footer(&self) -> bool {
        self.flags & flags::HISTORY_FOOTER != 0
    }

    /// Encode the superblock into exactly [`SUPERBLOCK_LEN`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; SUPERBLOCK_LEN] {
        let mut buf = [0u8; SUPERBLOCK_LEN];
        let mut w = LeWriter::new(&mut buf);
        w.put_bytes(&MAGIC);
        w.put_u32(LAYOUT_VERSION);
        w.put_u32(self.flags);
        w.put_u32(self.doc_kind.as_u32());
        w.put_u64(self.catalog.offset);
        w.put_u64(self.catalog.length);
        w.put_u64(self.link_table.offset);
        w.put_u64(self.link_table.length);
        w.put_u64(self.chunk_index.offset);
        w.put_u64(self.chunk_index.length);
        debug_assert_eq!(w.position(), SUPERBLOCK_LEN);
        buf
    }

    /// Decode a superblock from the start of `buf`.
    ///
    /// Validates magic and layout version, and rejects unknown `doc_kind`.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::BufferTooSmall`] if `buf` is shorter than
    /// [`SUPERBLOCK_LEN`], [`TesError::BadMagic`] / [`TesError::UnsupportedVersion`]
    /// for a bad header, or [`TesError::InvalidEnum`] for an unknown `doc_kind`.
    pub fn from_bytes(buf: &[u8]) -> Result<Self> {
        let mut r = LeReader::require(buf, "SuperblockV0", SUPERBLOCK_LEN)?;

        let magic = r.take_4();
        if magic != MAGIC {
            return Err(TesError::BadMagic {
                structure: "SuperblockV0",
                expected: MAGIC,
                found: magic,
            });
        }

        let layout_version = r.take_u32();
        if layout_version != LAYOUT_VERSION {
            return Err(TesError::UnsupportedVersion {
                structure: "SuperblockV0",
                found: layout_version,
                supported: LAYOUT_VERSION,
            });
        }

        let flags = r.take_u32();
        let doc_kind = DocKind::from_u32(r.take_u32())?;
        let catalog = Region::new(r.take_u64(), r.take_u64());
        let link_table = Region::new(r.take_u64(), r.take_u64());
        let chunk_index = Region::new(r.take_u64(), r.take_u64());

        Ok(Self {
            flags,
            doc_kind,
            catalog,
            link_table,
            chunk_index,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_kind_round_trip_all_variants() {
        for kind in [
            DocKind::Note,
            DocKind::Document,
            DocKind::Manuscript,
            DocKind::Research,
            DocKind::Deck,
            DocKind::WikiPage,
            DocKind::Hub,
            DocKind::Index,
        ] {
            assert_eq!(DocKind::from_u32(kind.as_u32()).unwrap(), kind);
        }
    }

    #[test]
    fn doc_kind_rejects_unknown() {
        let err = DocKind::from_u32(8).unwrap_err();
        assert!(matches!(
            err,
            TesError::InvalidEnum {
                field: "doc_kind",
                value: 8
            }
        ));
    }

    #[test]
    fn superblock_round_trip() {
        let mut sb = SuperblockV0::new(DocKind::Research);
        sb.flags = flags::HISTORY_FOOTER;
        sb.catalog = Region::new(64, 128);
        sb.link_table = Region::new(192, 80);
        sb.chunk_index = Region::new(272, 128);

        let bytes = sb.to_bytes();
        assert_eq!(bytes.len(), SUPERBLOCK_LEN);
        assert_eq!(&bytes[0..4], b"TESS");

        let decoded = SuperblockV0::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, sb);
        assert!(decoded.has_history_footer());
    }

    #[test]
    fn empty_skeleton_round_trip() {
        let sb = SuperblockV0::new(DocKind::Note);
        let decoded = SuperblockV0::from_bytes(&sb.to_bytes()).unwrap();
        assert_eq!(decoded, sb);
        assert!(!decoded.catalog.is_present());
        assert!(!decoded.chunk_index.is_present());
        assert!(!decoded.has_history_footer());
    }

    #[test]
    fn field_offsets_match_spec() {
        // Spot-check the documented byte offsets from layout_v0.md.
        let mut sb = SuperblockV0::new(DocKind::Document);
        sb.catalog = Region::new(0x1122_3344_5566_7788, 0);
        let bytes = sb.to_bytes();
        // magic 0..4, layout_version 4..8, flags 8..12, doc_kind 12..16
        assert_eq!(&bytes[0..4], b"TESS");
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(bytes[12..16].try_into().unwrap()), 1);
        // catalog_offset at 16..24
        assert_eq!(
            u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
            0x1122_3344_5566_7788
        );
    }

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = SuperblockV0::new(DocKind::Note).to_bytes();
        bytes[0] = b'X';
        assert!(matches!(
            SuperblockV0::from_bytes(&bytes),
            Err(TesError::BadMagic { .. })
        ));
    }

    #[test]
    fn rejects_future_version() {
        let mut bytes = SuperblockV0::new(DocKind::Note).to_bytes();
        bytes[4..8].copy_from_slice(&1u32.to_le_bytes());
        assert!(matches!(
            SuperblockV0::from_bytes(&bytes),
            Err(TesError::UnsupportedVersion { found: 1, .. })
        ));
    }

    #[test]
    fn rejects_short_buffer() {
        let short = [0u8; 32];
        assert!(matches!(
            SuperblockV0::from_bytes(&short),
            Err(TesError::BufferTooSmall {
                need: 64,
                got: 32,
                ..
            })
        ));
    }
}
