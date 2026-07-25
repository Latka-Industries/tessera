//! Chunk index (`TIDX`): fixed-size rows for random access to payloads.
//!
//! Wire spec: `docs/layout_v0.md` — *Chunk index region*.

use crate::error::{Result, TesError};
use argus::{LeReader, LeWriter};

/// Magic tag at the start of the chunk index region.
pub const MAGIC: [u8; 4] = *b"TIDX";

/// Only chunk index version understood by this build.
pub const INDEX_VERSION: u32 = 0;

/// Encoded size of the chunk index header, in bytes.
pub const HEADER_LEN: usize = 32;

/// Encoded size of a single chunk index entry, in bytes.
pub const ENTRY_LEN: usize = 48;

/// Chunk index flag bits.
pub mod chunk_flags {
    /// The chunk participates in reading-order text export.
    pub const READING_ORDER: u32 = 1;
}

/// Payload storage codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Codec {
    /// Stored bytes equal raw bytes (default for text).
    Raw = 0,
    /// Payload is a zstd frame decoding to `raw_byte_len` bytes.
    Zstd = 1,
}

impl Codec {
    /// Decode a codec discriminant.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::InvalidEnum`] if `value` is not a known codec.
    pub fn from_u32(value: u32) -> Result<Self> {
        match value {
            0 => Ok(Self::Raw),
            1 => Ok(Self::Zstd),
            other => Err(TesError::InvalidEnum {
                field: "codec",
                value: other,
            }),
        }
    }

    /// The `u32` discriminant stored on disk.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

/// Chunk payload type (`docs/layout_v0.md` — *Chunk types*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ChunkType {
    /// Semantic header + UTF-8 body.
    Text = 1,
    /// MIME + dimensions + raw or compressed image bytes.
    Image = 2,
    /// Display + resolved target (optional; redundant with link table).
    Link = 3,
    /// Quote span + target doc/chunk/range.
    Cite = 4,
    /// Layout id + ordered block list.
    Slide = 5,
    /// Imported PDF page raster.
    Page = 6,
    /// Contextual figure use referencing an [`Image`] chunk.
    Figure = 7,
}

impl ChunkType {
    /// Decode a chunk-type discriminant.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::InvalidEnum`] if `value` is not a known chunk type.
    pub fn from_u32(value: u32) -> Result<Self> {
        Ok(match value {
            1 => Self::Text,
            2 => Self::Image,
            3 => Self::Link,
            4 => Self::Cite,
            5 => Self::Slide,
            6 => Self::Page,
            7 => Self::Figure,
            other => {
                return Err(TesError::InvalidEnum {
                    field: "chunk_type",
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

    /// Lowercase type name for CLI / JSON summaries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::Link => "link",
            Self::Cite => "cite",
            Self::Slide => "slide",
            Self::Page => "page",
            Self::Figure => "figure",
        }
    }
}

/// A single 48-byte chunk index row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkIndexEntry {
    /// Stable within the file; 1-based in the v0 reference writer.
    pub chunk_id: u64,
    /// Payload type.
    pub chunk_type: ChunkType,
    /// Bitfield of [`chunk_flags`] values.
    pub chunk_flags: u32,
    /// File offset to the stored bytes.
    pub payload_offset: u64,
    /// Uncompressed payload size.
    pub raw_byte_len: u64,
    /// On-disk size at `payload_offset`.
    pub stored_byte_len: u64,
    /// Storage codec.
    pub codec: Codec,
}

impl ChunkIndexEntry {
    /// Whether this chunk is part of reading-order text export.
    #[must_use]
    pub const fn is_reading_order(&self) -> bool {
        self.chunk_flags & chunk_flags::READING_ORDER != 0
    }

    /// Encode the entry into exactly [`ENTRY_LEN`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; ENTRY_LEN] {
        let mut buf = [0u8; ENTRY_LEN];
        let mut w = LeWriter::new(&mut buf);
        w.put_u64(self.chunk_id);
        w.put_u32(self.chunk_type.as_u32());
        w.put_u32(self.chunk_flags);
        w.put_u64(self.payload_offset);
        w.put_u64(self.raw_byte_len);
        w.put_u64(self.stored_byte_len);
        w.put_u32(self.codec.as_u32());
        w.put_zeros(4); // reserved
        debug_assert_eq!(w.position(), ENTRY_LEN);
        buf
    }

    /// Decode an entry from the start of `buf`.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::BufferTooSmall`] if `buf` is shorter than
    /// [`ENTRY_LEN`], or [`TesError::InvalidEnum`] for a bad type/codec.
    pub fn from_bytes(buf: &[u8]) -> Result<Self> {
        let mut r = LeReader::require(buf, "ChunkIndexEntry", ENTRY_LEN)?;
        let chunk_id = r.take_u64();
        let chunk_type = ChunkType::from_u32(r.take_u32())?;
        let chunk_flags = r.take_u32();
        let payload_offset = r.take_u64();
        let raw_byte_len = r.take_u64();
        let stored_byte_len = r.take_u64();
        let codec = Codec::from_u32(r.take_u32())?;
        r.skip(4); // reserved
        Ok(Self {
            chunk_id,
            chunk_type,
            chunk_flags,
            payload_offset,
            raw_byte_len,
            stored_byte_len,
            codec,
        })
    }
}

/// Fixed 32-byte header preceding the chunk index rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkIndexHeader {
    /// Number of index rows that follow the header.
    pub entry_count: u64,
}

impl ChunkIndexHeader {
    /// Create a header for `entry_count` rows.
    #[must_use]
    pub const fn new(entry_count: u64) -> Self {
        Self { entry_count }
    }

    /// Total encoded size of the index region: header + all entries.
    #[must_use]
    pub const fn region_len(&self) -> u64 {
        HEADER_LEN as u64 + self.entry_count * ENTRY_LEN as u64
    }

    /// Encode the header into exactly [`HEADER_LEN`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; HEADER_LEN] {
        let mut buf = [0u8; HEADER_LEN];
        let mut w = LeWriter::new(&mut buf);
        w.put_bytes(&MAGIC);
        w.put_u32(INDEX_VERSION);
        w.put_u64(self.entry_count);
        w.put_zeros(16); // reserved
        debug_assert_eq!(w.position(), HEADER_LEN);
        buf
    }

    /// Decode the header from the start of `buf`.
    ///
    /// Validates magic and index version.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::BufferTooSmall`] if `buf` is too short,
    /// [`TesError::BadMagic`] if the tag is not `TIDX`, or
    /// [`TesError::UnsupportedVersion`] for an unknown index version.
    pub fn from_bytes(buf: &[u8]) -> Result<Self> {
        let mut r = LeReader::require(buf, "ChunkIndexHeader", HEADER_LEN)?;
        let magic = r.take_4();
        if magic != MAGIC {
            return Err(TesError::BadMagic {
                structure: "ChunkIndexHeader",
                expected: MAGIC,
                found: magic,
            });
        }
        let index_version = r.take_u32();
        if index_version != INDEX_VERSION {
            return Err(TesError::UnsupportedVersion {
                structure: "ChunkIndexHeader",
                found: index_version,
                supported: INDEX_VERSION,
            });
        }
        let entry_count = r.take_u64();
        r.skip(16); // reserved
        Ok(Self { entry_count })
    }
}

/// Parse a full chunk-index region (`TIDX` header + fixed entries).
///
/// Requires `bytes.len()` to equal `32 + entry_count × 48`.
///
/// # Errors
///
/// Returns header/entry decode errors, or [`TesError::IndexLengthMismatch`]
/// if the region length does not match the header's `entry_count`.
pub fn read_chunk_index(bytes: &[u8]) -> Result<Vec<ChunkIndexEntry>> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let header = ChunkIndexHeader::from_bytes(bytes)?;
    let expected = header.region_len();
    let got = bytes.len() as u64;
    if got != expected {
        return Err(TesError::IndexLengthMismatch { expected, got });
    }
    let mut entries = Vec::with_capacity(header.entry_count as usize);
    for i in 0..header.entry_count as usize {
        let start = HEADER_LEN + i * ENTRY_LEN;
        entries.push(ChunkIndexEntry::from_bytes(&bytes[start..])?);
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry() -> ChunkIndexEntry {
        ChunkIndexEntry {
            chunk_id: 1,
            chunk_type: ChunkType::Text,
            chunk_flags: chunk_flags::READING_ORDER,
            payload_offset: 512,
            raw_byte_len: 128,
            stored_byte_len: 128,
            codec: Codec::Raw,
        }
    }

    #[test]
    fn entry_round_trip() {
        let entry = sample_entry();
        let bytes = entry.to_bytes();
        assert_eq!(bytes.len(), ENTRY_LEN);
        let decoded = ChunkIndexEntry::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, entry);
        assert!(decoded.is_reading_order());
    }

    #[test]
    fn entry_zstd_image_round_trip() {
        let entry = ChunkIndexEntry {
            chunk_id: 42,
            chunk_type: ChunkType::Image,
            chunk_flags: 0,
            payload_offset: 4096,
            raw_byte_len: 10_000,
            stored_byte_len: 3_200,
            codec: Codec::Zstd,
        };
        let decoded = ChunkIndexEntry::from_bytes(&entry.to_bytes()).unwrap();
        assert_eq!(decoded, entry);
        assert!(!decoded.is_reading_order());
    }

    #[test]
    fn header_round_trip_and_region_len() {
        let header = ChunkIndexHeader::new(3);
        assert_eq!(header.region_len(), 32 + 3 * 48);
        let bytes = header.to_bytes();
        assert_eq!(&bytes[0..4], b"TIDX");
        let decoded = ChunkIndexHeader::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, header);
    }

    #[test]
    fn empty_index_region_len() {
        assert_eq!(ChunkIndexHeader::new(0).region_len(), 32);
    }

    #[test]
    fn header_plus_entries_round_trip() {
        let entries = [
            sample_entry(),
            ChunkIndexEntry {
                chunk_id: 2,
                chunk_type: ChunkType::Cite,
                ..sample_entry()
            },
        ];
        let header = ChunkIndexHeader::new(entries.len() as u64);

        let mut region = Vec::new();
        region.extend_from_slice(&header.to_bytes());
        for e in &entries {
            region.extend_from_slice(&e.to_bytes());
        }
        assert_eq!(region.len() as u64, header.region_len());

        let decoded_header = ChunkIndexHeader::from_bytes(&region).unwrap();
        assert_eq!(decoded_header.entry_count, 2);
        for (i, expected) in entries.iter().enumerate() {
            let start = HEADER_LEN + i * ENTRY_LEN;
            let got = ChunkIndexEntry::from_bytes(&region[start..]).unwrap();
            assert_eq!(&got, expected);
        }
    }

    #[test]
    fn chunk_type_round_trip_all_variants() {
        for t in [
            ChunkType::Text,
            ChunkType::Image,
            ChunkType::Link,
            ChunkType::Cite,
            ChunkType::Slide,
            ChunkType::Page,
            ChunkType::Figure,
        ] {
            assert_eq!(ChunkType::from_u32(t.as_u32()).unwrap(), t);
        }
        assert!(matches!(
            ChunkType::from_u32(0),
            Err(TesError::InvalidEnum { .. })
        ));
        assert!(matches!(
            ChunkType::from_u32(8),
            Err(TesError::InvalidEnum { .. })
        ));
    }

    #[test]
    fn codec_rejects_unknown() {
        assert!(matches!(
            Codec::from_u32(2),
            Err(TesError::InvalidEnum {
                field: "codec",
                value: 2
            })
        ));
    }

    #[test]
    fn header_rejects_bad_magic() {
        let mut bytes = ChunkIndexHeader::new(1).to_bytes();
        bytes[0] = b'Z';
        assert!(matches!(
            ChunkIndexHeader::from_bytes(&bytes),
            Err(TesError::BadMagic { .. })
        ));
    }
}
