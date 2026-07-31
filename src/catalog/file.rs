//! Read-only `.tes` document (`docs/engine.md` — read path).
//!
//! Default open uses mmap; [`OpenMode::Copy`] / [`TesFile::open_buffered`] loads
//! into an owned buffer for untrusted or network-backed inputs.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use memmap2::Mmap;

use crate::catalog::document::DocumentCatalog;
use crate::catalog::index::{ChunkIndexEntry, Codec, read_chunk_index};
use crate::catalog::link::{LinkEntry, read_link_table};
use crate::error::{Result, TesError};
use crate::layout::{self, FileImage, OpenMode, SUPERBLOCK_LEN, SuperblockV0};

/// An open `.tes` file with parsed catalog and chunk index.
pub struct TesFile {
    path: PathBuf,
    bytes: FileImage,
    superblock: SuperblockV0,
    catalog: Option<DocumentCatalog>,
    chunks: Vec<ChunkIndexEntry>,
    links: Vec<LinkEntry>,
}

impl TesFile {
    /// Open `path` read-only via mmap and parse superblock + catalog + index.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::Io`] if the file cannot be opened or mapped, or
    /// parse/bounds errors from [`Self::from_image`].
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        Self::open_with(path, OpenMode::Mmap)
    }

    /// Open `path` by reading the whole file into memory (no mmap).
    ///
    /// Prefer this for untrusted or network-backed paths; see `docs/security.md`.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::Io`] if the file cannot be read, or parse/bounds
    /// errors from [`Self::from_image`].
    pub fn open_buffered(path: impl Into<PathBuf>) -> Result<Self> {
        Self::open_with(path, OpenMode::Copy)
    }

    /// Open `path` using [`OpenMode`].
    ///
    /// # Errors
    ///
    /// Returns [`TesError::Io`] on open/map/read failure, or parse/bounds errors
    /// from [`Self::from_image`].
    pub fn open_with(path: impl Into<PathBuf>, mode: OpenMode) -> Result<Self> {
        let path = path.into();
        let image = layout::open_image(&path, mode)?;
        Self::from_image(path, image)
    }

    /// Parse an already-mapped buffer (tests / advanced callers).
    ///
    /// # Errors
    ///
    /// Returns [`TesError::BufferTooSmall`] if the map is shorter than the
    /// superblock, decode errors from the superblock/catalog/index/link table,
    /// or [`TesError::OutOfBounds`] if a chunk payload extends past EOF.
    pub fn from_mmap(path: PathBuf, mmap: Mmap) -> Result<Self> {
        Self::from_image(path, FileImage::Map(mmap))
    }

    /// Parse an owned in-memory image (tests / downloaded blobs).
    ///
    /// # Errors
    ///
    /// Same as [`Self::from_image`].
    pub fn from_bytes(path: PathBuf, bytes: Vec<u8>) -> Result<Self> {
        Self::from_image(path, FileImage::Owned(bytes))
    }

    /// Parse a [`FileImage`] (mmap or owned).
    ///
    /// # Errors
    ///
    /// Returns [`TesError::BufferTooSmall`] if the image is shorter than the
    /// superblock, decode errors from the superblock/catalog/index/link table,
    /// or [`TesError::OutOfBounds`] if a chunk payload extends past EOF.
    pub fn from_image(path: PathBuf, bytes: FileImage) -> Result<Self> {
        if bytes.len() < SUPERBLOCK_LEN {
            return Err(TesError::BufferTooSmall {
                structure: "SuperblockV0",
                need: SUPERBLOCK_LEN,
                got: bytes.len(),
            });
        }
        let superblock = SuperblockV0::from_bytes(&bytes)?;

        let catalog = if superblock.catalog.is_present() {
            let region = superblock.catalog.slice(&bytes, "catalog")?;
            Some(DocumentCatalog::from_bytes(region)?)
        } else {
            None
        };

        let index_bytes = superblock.chunk_index.slice(&bytes, "chunk_index")?;
        let chunks = read_chunk_index(index_bytes)?;
        let link_bytes = superblock.link_table.slice(&bytes, "link_table")?;
        let links = read_link_table(link_bytes)?;

        // Light payload-bound check (full verify lands in THI-6).
        let usable_len =
            crate::catalog::history::usable_file_len(&bytes, superblock.has_history_footer());
        for entry in &chunks {
            let end = entry
                .payload_offset
                .checked_add(entry.stored_byte_len)
                .ok_or(TesError::OutOfBounds {
                    structure: "chunk_payload",
                    offset: entry.payload_offset,
                    length: entry.stored_byte_len,
                    file_len: usable_len,
                })?;
            if end > usable_len {
                return Err(TesError::OutOfBounds {
                    structure: "chunk_payload",
                    offset: entry.payload_offset,
                    length: entry.stored_byte_len,
                    file_len: usable_len,
                });
            }
        }

        Ok(Self {
            path,
            bytes,
            superblock,
            catalog,
            chunks,
            links,
        })
    }

    /// Path used to open this file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// File length in bytes.
    #[must_use]
    pub fn file_len(&self) -> u64 {
        self.bytes.len() as u64
    }

    /// Parsed superblock.
    #[must_use]
    pub fn superblock(&self) -> &SuperblockV0 {
        &self.superblock
    }

    /// Document catalog, if present.
    #[must_use]
    pub fn catalog(&self) -> Option<&DocumentCatalog> {
        self.catalog.as_ref()
    }

    /// Decode the optional THST history footer (M10).
    ///
    /// # Errors
    ///
    /// Returns history decode errors when the history flag is set but the
    /// trailer is malformed. Returns `Ok(None)` when the flag is clear.
    pub fn history(&self) -> Result<Option<crate::catalog::history::HistoryV1>> {
        if !self.superblock.has_history_footer() {
            return Ok(None);
        }
        let suffix = crate::catalog::history::footer_suffix_len(&self.bytes).ok_or_else(|| {
            TesError::InvalidHistory {
                message: "HISTORY_FOOTER flag set but THST trailer missing".into(),
            }
        })?;
        let start = self.bytes.len() - suffix;
        Ok(Some(crate::catalog::history::decode_footer(
            &self.bytes[start..],
        )?))
    }

    /// Chunk index rows (no payload bodies).
    #[must_use]
    pub fn chunks(&self) -> &[ChunkIndexEntry] {
        &self.chunks
    }

    /// Parsed outbound/internal link-table entries.
    #[must_use]
    pub fn links(&self) -> &[LinkEntry] {
        &self.links
    }

    /// Raw file bytes (mmap view or owned buffer).
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Stored payload bytes for an index entry (no codec decode).
    ///
    /// # Errors
    ///
    /// Returns [`TesError::OutOfBounds`] if the entry's payload region extends
    /// past the file length.
    pub fn payload_bytes(&self, entry: &ChunkIndexEntry) -> Result<&[u8]> {
        let usable_len = crate::catalog::history::usable_file_len(
            &self.bytes,
            self.superblock.has_history_footer(),
        );
        let end = entry
            .payload_offset
            .checked_add(entry.stored_byte_len)
            .ok_or(TesError::OutOfBounds {
                structure: "chunk_payload",
                offset: entry.payload_offset,
                length: entry.stored_byte_len,
                file_len: usable_len,
            })?;
        if end > usable_len {
            return Err(TesError::OutOfBounds {
                structure: "chunk_payload",
                offset: entry.payload_offset,
                length: entry.stored_byte_len,
                file_len: usable_len,
            });
        }
        Ok(&self.bytes[entry.payload_offset as usize..end as usize])
    }

    /// Decode a payload to its raw (uncompressed) bytes.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::OutOfBounds`] from [`Self::payload_bytes`], or
    /// [`TesError::Decode`] if codec decompression fails.
    pub fn decode_payload<'a>(&'a self, entry: &ChunkIndexEntry) -> Result<Cow<'a, [u8]>> {
        let stored = self.payload_bytes(entry)?;
        let codec = match entry.codec {
            Codec::Raw => argus::PayloadCodec::Raw,
            Codec::Zstd => argus::PayloadCodec::Zstd,
        };
        argus::decode(codec, stored, entry.raw_byte_len).map_err(|err| TesError::Decode {
            chunk_id: entry.chunk_id,
            message: err.to_string(),
        })
    }

    /// Look up a chunk by id.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::ChunkNotFound`] if no index row has `chunk_id`.
    pub fn chunk_by_id(&self, chunk_id: u64) -> Result<&ChunkIndexEntry> {
        self.chunks
            .iter()
            .find(|c| c.chunk_id == chunk_id)
            .ok_or(TesError::ChunkNotFound { chunk_id })
    }

    /// Reading-order index rows, sorted by ascending `chunk_id`.
    #[must_use]
    pub fn reading_order_chunks(&self) -> Vec<&ChunkIndexEntry> {
        let mut rows: Vec<&ChunkIndexEntry> = self
            .chunks
            .iter()
            .filter(|c| c.is_reading_order())
            .collect();
        rows.sort_by_key(|c| c.chunk_id);
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::DocKind;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/v0")
            .join(name)
    }

    #[test]
    fn open_empty_fixture() {
        let file = TesFile::open(fixture("empty.tes")).unwrap();
        assert_eq!(file.file_len(), SUPERBLOCK_LEN as u64);
        assert_eq!(file.superblock().doc_kind, DocKind::Note);
        assert!(file.catalog().is_none());
        assert!(file.chunks().is_empty());
    }

    #[test]
    fn open_note_one_chunk_fixture() {
        let file = TesFile::open(fixture("note_one_chunk.tes")).unwrap();
        let cat = file.catalog().expect("catalog");
        assert_eq!(cat.title, "Meeting notes");
        assert_eq!(cat.doc_kind, "note");
        assert_eq!(file.chunks().len(), 1);
        assert_eq!(file.chunks()[0].chunk_id, 1);
        let payload = file.payload_bytes(&file.chunks()[0]).unwrap();
        assert!(!payload.is_empty());
    }

    #[test]
    fn open_buffered_matches_mmap() {
        let path = fixture("note_one_chunk.tes");
        let mapped = TesFile::open(&path).unwrap();
        let buffered = TesFile::open_buffered(&path).unwrap();
        assert_eq!(mapped.as_bytes(), buffered.as_bytes());
        assert_eq!(
            mapped.catalog().map(|c| c.title.as_str()),
            buffered.catalog().map(|c| c.title.as_str())
        );
        assert_eq!(mapped.chunks().len(), buffered.chunks().len());
    }
}
