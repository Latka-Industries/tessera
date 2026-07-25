//! Sealed `.tes` writer session (`docs/engine.md` — write path).
//!
//! Buffers catalog + chunk payloads in memory, then writes a single sealed file
//! on [`TesWriterSession::commit`]. File map (v0 reference writer):
//!
//! ```text
//! Superblock (64) | Catalog? | TIDX? | Payloads…
//! ```
//!
//! Link table and `THST` footer are not emitted in this first merge.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::catalog::chunk::{TextHeader, encode_text_payload};
use crate::catalog::document::DocumentCatalog;
use crate::catalog::index::{
    ChunkIndexEntry, ChunkIndexHeader, ChunkType, Codec, ENTRY_LEN, HEADER_LEN, chunk_flags,
};
use crate::error::{Result, TesError};
use crate::layout::{DocKind, Region, SUPERBLOCK_LEN, SuperblockV0};
use crate::wire::align8;

struct PendingChunk {
    chunk_type: ChunkType,
    chunk_flags: u32,
    payload: Vec<u8>,
}

/// In-memory builder that seals one `.tes` file on commit.
pub struct TesWriterSession {
    path: PathBuf,
    doc_kind: DocKind,
    catalog: Option<DocumentCatalog>,
    chunks: Vec<PendingChunk>,
    sealed: bool,
}

impl TesWriterSession {
    /// Start a new session that will create `path` exclusively on commit.
    pub fn create(path: impl Into<PathBuf>, doc_kind: DocKind) -> Self {
        Self {
            path: path.into(),
            doc_kind,
            catalog: None,
            chunks: Vec::new(),
            sealed: false,
        }
    }

    /// Target path for this session.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Set (or replace) the document catalog written after the superblock.
    pub fn set_catalog(&mut self, catalog: DocumentCatalog) -> Result<()> {
        self.ensure_open()?;
        // Keep superblock `doc_kind` aligned with the catalog string mirror.
        if let Ok(kind) = doc_kind_from_str(&catalog.doc_kind) {
            self.doc_kind = kind;
        }
        self.catalog = Some(catalog);
        Ok(())
    }

    /// Append a reading-order text chunk with the given semantic header and body.
    pub fn add_text_chunk(&mut self, header: &TextHeader, body: &str) -> Result<()> {
        self.ensure_open()?;
        let payload = encode_text_payload(header, body)?;
        self.chunks.push(PendingChunk {
            chunk_type: ChunkType::Text,
            chunk_flags: chunk_flags::READING_ORDER,
            payload,
        });
        Ok(())
    }

    /// Write a sealed `.tes` and consume the session.
    ///
    /// Creates the file with `create_new` (fails if it already exists).
    pub fn commit(mut self) -> Result<()> {
        self.ensure_open()?;
        let bytes = self.encode_file()?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        self.sealed = true;
        Ok(())
    }

    /// Encode the sealed file bytes without writing (for tests / fixtures).
    pub fn encode_file(&self) -> Result<Vec<u8>> {
        self.ensure_open()?;

        let catalog_bytes = match &self.catalog {
            Some(cat) => Some(cat.to_bytes()?),
            None => None,
        };

        let mut cursor = SUPERBLOCK_LEN as u64;

        let catalog_region = if let Some(ref cat) = catalog_bytes {
            let offset = align8(cursor);
            let length = cat.len() as u64;
            cursor = align8(offset + length);
            Region::new(offset, length)
        } else {
            Region::NONE
        };

        let (chunk_index_region, index_bytes, payload_blobs) = if self.chunks.is_empty() {
            (Region::NONE, Vec::new(), Vec::new())
        } else {
            let header = ChunkIndexHeader::new(self.chunks.len() as u64);
            let index_len = header.region_len();
            let index_offset = align8(cursor);
            cursor = index_offset + index_len;

            let mut entries = Vec::with_capacity(self.chunks.len());
            let mut payloads = Vec::with_capacity(self.chunks.len());
            for (i, chunk) in self.chunks.iter().enumerate() {
                let payload_offset = cursor;
                let len = chunk.payload.len() as u64;
                entries.push(ChunkIndexEntry {
                    chunk_id: (i as u64) + 1,
                    chunk_type: chunk.chunk_type,
                    chunk_flags: chunk.chunk_flags,
                    payload_offset,
                    raw_byte_len: len,
                    stored_byte_len: len,
                    codec: Codec::Raw,
                });
                cursor += len;
                payloads.push(chunk.payload.clone());
            }

            let mut index_bytes = Vec::with_capacity(index_len as usize);
            index_bytes.extend_from_slice(&header.to_bytes());
            for entry in &entries {
                index_bytes.extend_from_slice(&entry.to_bytes());
            }
            debug_assert_eq!(index_bytes.len(), HEADER_LEN + entries.len() * ENTRY_LEN);

            (Region::new(index_offset, index_len), index_bytes, payloads)
        };

        let sb = SuperblockV0 {
            flags: 0,
            doc_kind: self.doc_kind,
            catalog: catalog_region,
            link_table: Region::NONE,
            chunk_index: chunk_index_region,
        };

        let mut out = Vec::with_capacity(cursor as usize);
        out.extend_from_slice(&sb.to_bytes());

        if let Some(cat) = catalog_bytes {
            pad_to(&mut out, catalog_region.offset as usize);
            out.extend_from_slice(&cat);
        }

        if !index_bytes.is_empty() {
            pad_to(&mut out, chunk_index_region.offset as usize);
            out.extend_from_slice(&index_bytes);
            for payload in &payload_blobs {
                out.extend_from_slice(payload);
            }
        }

        Ok(out)
    }

    fn ensure_open(&self) -> Result<()> {
        if self.sealed {
            Err(TesError::SessionSealed)
        } else {
            Ok(())
        }
    }
}

fn pad_to(buf: &mut Vec<u8>, offset: usize) {
    if buf.len() < offset {
        buf.resize(offset, 0);
    }
}

fn doc_kind_from_str(s: &str) -> Result<DocKind> {
    Ok(match s {
        "note" => DocKind::Note,
        "document" => DocKind::Document,
        "manuscript" => DocKind::Manuscript,
        "research" => DocKind::Research,
        "deck" => DocKind::Deck,
        "wiki_page" => DocKind::WikiPage,
        "hub" => DocKind::Hub,
        "index" => DocKind::Index,
        _ => {
            return Err(TesError::InvalidEnum {
                field: "doc_kind",
                value: u32::MAX,
            });
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::chunk::decode_text_payload;
    use crate::catalog::index::ChunkIndexHeader;
    use tempfile::tempdir;

    #[test]
    fn empty_skeleton_is_64_bytes() {
        let session = TesWriterSession::create("empty.tes", DocKind::Note);
        let bytes = session.encode_file().unwrap();
        assert_eq!(bytes.len(), SUPERBLOCK_LEN);
        let sb = SuperblockV0::from_bytes(&bytes).unwrap();
        assert_eq!(sb.doc_kind, DocKind::Note);
        assert!(!sb.catalog.is_present());
        assert!(!sb.chunk_index.is_present());
    }

    #[test]
    fn note_one_chunk_round_trip_structure() {
        let mut session = TesWriterSession::create("note.tes", DocKind::Note);
        session
            .set_catalog(DocumentCatalog::new(
                "550e8400-e29b-41d4-a716-446655440000",
                "Meeting notes",
                "2026-06-05T12:00:00Z",
                "2026-06-05T12:30:00Z",
                DocKind::Note,
            ))
            .unwrap();
        session
            .add_text_chunk(&TextHeader::paragraph(), "Hello from Tessera.")
            .unwrap();

        let bytes = session.encode_file().unwrap();
        let sb = SuperblockV0::from_bytes(&bytes).unwrap();
        assert!(sb.catalog.is_present());
        assert!(sb.chunk_index.is_present());

        let cat = DocumentCatalog::from_bytes(
            &bytes[sb.catalog.offset as usize..sb.catalog.end() as usize],
        )
        .unwrap();
        assert_eq!(cat.title, "Meeting notes");

        let index_slice = &bytes[sb.chunk_index.offset as usize..sb.chunk_index.end() as usize];
        let header = ChunkIndexHeader::from_bytes(index_slice).unwrap();
        assert_eq!(header.entry_count, 1);
        let entry = ChunkIndexEntry::from_bytes(&index_slice[HEADER_LEN..]).unwrap();
        assert_eq!(entry.chunk_id, 1);
        assert_eq!(entry.chunk_type, ChunkType::Text);
        assert!(entry.is_reading_order());
        assert_eq!(entry.codec, Codec::Raw);

        let payload = &bytes[entry.payload_offset as usize
            ..(entry.payload_offset + entry.stored_byte_len) as usize];
        let (header, body) = decode_text_payload(payload).unwrap();
        assert_eq!(header, TextHeader::paragraph());
        assert_eq!(body, "Hello from Tessera.");
    }

    #[test]
    fn commit_writes_exclusive_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.tes");
        let mut session = TesWriterSession::create(&path, DocKind::Note);
        session
            .add_text_chunk(&TextHeader::paragraph(), "hi")
            .unwrap();
        session.commit().unwrap();
        assert!(path.is_file());

        let mut again = TesWriterSession::create(&path, DocKind::Note);
        again
            .add_text_chunk(&TextHeader::paragraph(), "nope")
            .unwrap();
        let err = again.commit().unwrap_err();
        assert!(matches!(err, TesError::Io(_)));
    }
}
