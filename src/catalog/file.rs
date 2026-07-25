//! Read-only mmap'd `.tes` document (`docs/engine.md` — read path).

use std::path::{Path, PathBuf};

use memmap2::Mmap;

use crate::catalog::document::DocumentCatalog;
use crate::catalog::index::{ChunkIndexEntry, read_chunk_index};
use crate::error::{Result, TesError};
use crate::layout::{self, SUPERBLOCK_LEN, SuperblockV0};

/// An open, memory-mapped `.tes` file with parsed catalog and chunk index.
pub struct TesFile {
    path: PathBuf,
    mmap: Mmap,
    superblock: SuperblockV0,
    catalog: Option<DocumentCatalog>,
    chunks: Vec<ChunkIndexEntry>,
}

impl TesFile {
    /// Open `path` read-only, mmap it, and parse superblock + catalog + index.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let mmap = layout::open_mmap(&path)?;
        Self::from_mmap(path, mmap)
    }

    /// Parse an already-mapped buffer (tests / advanced callers).
    pub fn from_mmap(path: PathBuf, mmap: Mmap) -> Result<Self> {
        if mmap.len() < SUPERBLOCK_LEN {
            return Err(TesError::BufferTooSmall {
                structure: "SuperblockV0",
                need: SUPERBLOCK_LEN,
                got: mmap.len(),
            });
        }
        let superblock = SuperblockV0::from_bytes(&mmap)?;

        let catalog = if superblock.catalog.is_present() {
            let bytes = superblock.catalog.slice(&mmap, "catalog")?;
            Some(DocumentCatalog::from_bytes(bytes)?)
        } else {
            None
        };

        let index_bytes = superblock.chunk_index.slice(&mmap, "chunk_index")?;
        let chunks = read_chunk_index(index_bytes)?;

        // Light payload-bound check (full verify lands in THI-6).
        let file_len = mmap.len() as u64;
        for entry in &chunks {
            let end = entry
                .payload_offset
                .checked_add(entry.stored_byte_len)
                .ok_or(TesError::OutOfBounds {
                    structure: "chunk_payload",
                    offset: entry.payload_offset,
                    length: entry.stored_byte_len,
                    file_len,
                })?;
            if end > file_len {
                return Err(TesError::OutOfBounds {
                    structure: "chunk_payload",
                    offset: entry.payload_offset,
                    length: entry.stored_byte_len,
                    file_len,
                });
            }
        }

        Ok(Self {
            path,
            mmap,
            superblock,
            catalog,
            chunks,
        })
    }

    /// Path used to open this file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Mapped file length in bytes.
    #[must_use]
    pub fn file_len(&self) -> u64 {
        self.mmap.len() as u64
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

    /// Chunk index rows (no payload bodies).
    #[must_use]
    pub fn chunks(&self) -> &[ChunkIndexEntry] {
        &self.chunks
    }

    /// Raw mmap bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.mmap
    }

    /// Stored payload bytes for an index entry (no codec decode).
    pub fn payload_bytes(&self, entry: &ChunkIndexEntry) -> Result<&[u8]> {
        let file_len = self.file_len();
        let end = entry
            .payload_offset
            .checked_add(entry.stored_byte_len)
            .ok_or(TesError::OutOfBounds {
                structure: "chunk_payload",
                offset: entry.payload_offset,
                length: entry.stored_byte_len,
                file_len,
            })?;
        if end > file_len {
            return Err(TesError::OutOfBounds {
                structure: "chunk_payload",
                offset: entry.payload_offset,
                length: entry.stored_byte_len,
                file_len,
            });
        }
        Ok(&self.mmap[entry.payload_offset as usize..end as usize])
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
}
