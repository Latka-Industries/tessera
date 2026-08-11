//! Sealed `.tes` writer session (`docs/engine.md` — write path).
//!
//! Buffers catalog + chunk payloads in memory, then writes a single sealed file
//! on [`TesWriterSession::commit`]. File map (v0 reference writer):
//!
//! ```text
//! Superblock (64) | Catalog? | TIDX? | Payloads… | THST?
//! ```
//!
//! The optional `THST` footer is attached by [`crate::history::save_revision`]
//! (and preserved across edit writes), not by this session.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::catalog::chunk::{
    CitePayload, InlineKind, InlineSpan, TextHeader, decode_text_payload, encode_text_payload,
};
use crate::catalog::document::DocumentCatalog;
use crate::catalog::features::{FeatureSet, ids as feature_ids};
use crate::catalog::index::{
    ChunkIndexEntry, ChunkIndexHeader, ChunkType, Codec, ENTRY_LEN, HEADER_LEN, chunk_flags,
};
use crate::catalog::link::{LinkEntry, LinkKind, LinkTarget, OutboundLink, encode_link_table};
use crate::catalog::media::{AttachmentPayload, FigureRef, ImagePayload};
use crate::catalog::slide::SlidePayload;
use crate::error::{Result, TesError};
use crate::layout::{DocKind, Region, SUPERBLOCK_LEN, SuperblockV0};
use argus::align8;

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
    links: Vec<LinkEntry>,
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
            links: Vec::new(),
            sealed: false,
        }
    }

    /// Target path for this session.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Set (or replace) the document catalog written after the superblock.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::SessionSealed`] if the session was already committed.
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
    ///
    /// Returns the 1-based `chunk_id` assigned on commit.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::SessionSealed`] if sealed, or payload encode errors
    /// from [`encode_text_payload`].
    pub fn add_text_chunk(&mut self, header: &TextHeader, body: &str) -> Result<u64> {
        self.ensure_open()?;
        let payload = encode_text_payload(header, body)?;
        self.chunks.push(PendingChunk {
            chunk_type: ChunkType::Text,
            chunk_flags: chunk_flags::READING_ORDER,
            payload,
        });
        Ok(self.chunks.len() as u64)
    }

    /// Append a text chunk and materialize outbound links into `TLNK` + Link spans.
    ///
    /// Drops any existing Link spans on `header` (and on table cells), assigns
    /// contiguous `link_id`s starting at the current table length, then adds one
    /// [`LinkEntry`] per outbound edge via [`OutboundLink::into_entry`].
    ///
    /// For structured tables, outbound links are applied to cell spans (row-major
    /// order matching temporary parse-time link ids) rather than `header.spans`.
    ///
    /// # Errors
    ///
    /// Returns session / encode / link validation errors.
    pub fn add_text_with_outbound_links(
        &mut self,
        mut header: TextHeader,
        body: &str,
        outbound: &[OutboundLink],
    ) -> Result<u64> {
        header
            .spans
            .retain(|s| !matches!(s.kind, InlineKind::Link { .. }));

        let cell_slots = table_link_slots(&header);
        if let Some(table) = header.table.as_mut() {
            for row in &mut table.rows {
                for cell in &mut row.cells {
                    cell.spans
                        .retain(|s| !matches!(s.kind, InlineKind::Link { .. }));
                }
            }
        }

        let base = self.link_count() as u64;
        if cell_slots.is_empty() {
            for (i, pending) in outbound.iter().enumerate() {
                header.spans.push(InlineSpan {
                    start: pending.start,
                    end: pending.end,
                    kind: InlineKind::Link {
                        link_id: base + i as u64,
                    },
                });
            }
        } else {
            let Some(table) = header.table.as_mut() else {
                return Err(TesError::EditOp {
                    message: "table link slots without table payload".into(),
                });
            };
            if outbound.len() != cell_slots.len() {
                return Err(TesError::EditOp {
                    message: format!(
                        "table link count mismatch: {} outbound vs {} cell slots",
                        outbound.len(),
                        cell_slots.len()
                    ),
                });
            }
            for (i, pending) in outbound.iter().enumerate() {
                let (ri, ci, start, end) = cell_slots[i];
                let cell = table
                    .rows
                    .get_mut(ri)
                    .and_then(|row| row.cells.get_mut(ci))
                    .ok_or_else(|| TesError::EditOp {
                        message: format!("missing table cell [{ri}][{ci}] for link"),
                    })?;
                // Prefer outbound offsets (trim-shifted); fall back to slot ranges.
                let (start, end) = if pending.end > pending.start {
                    (pending.start, pending.end)
                } else {
                    (start, end)
                };
                cell.spans.push(InlineSpan {
                    start,
                    end,
                    kind: InlineKind::Link {
                        link_id: base + i as u64,
                    },
                });
            }
        }
        let chunk_id = self.add_text_chunk(&header, body)?;
        for pending in outbound {
            let entry = pending.clone().into_entry(chunk_id, LinkKind::Wiki)?;
            self.add_link(entry)?;
        }
        Ok(chunk_id)
    }

    /// Append a raw store payload without re-encoding (history materialization).
    ///
    /// Chunk ids are assigned sequentially `1..n` in push order, matching the
    /// v0 reference writer. Callers should push manifest entries in id order.
    ///
    /// Returns the 1-based `chunk_id` assigned on commit.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::SessionSealed`] if the session was already committed.
    pub fn add_payload_chunk(
        &mut self,
        chunk_type: ChunkType,
        chunk_flags: u32,
        payload: Vec<u8>,
    ) -> Result<u64> {
        self.ensure_open()?;
        self.chunks.push(PendingChunk {
            chunk_type,
            chunk_flags,
            payload,
        });
        Ok(self.chunks.len() as u64)
    }

    /// Append a reusable image media chunk (not reading-order).
    ///
    /// Returns the 1-based `chunk_id` assigned on commit.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::SessionSealed`] if sealed, or encode/validation
    /// errors from [`ImagePayload::to_bytes`].
    pub fn add_image_chunk(&mut self, image: &ImagePayload) -> Result<u64> {
        self.ensure_open()?;
        let payload = image.to_bytes()?;
        self.chunks.push(PendingChunk {
            chunk_type: ChunkType::Image,
            chunk_flags: 0,
            payload,
        });
        Ok(self.chunks.len() as u64)
    }

    /// Append a reading-order figure referencing an image chunk.
    ///
    /// Returns the 1-based `chunk_id` assigned on commit.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::SessionSealed`] if sealed, or encode/validation
    /// errors from [`FigureRef::to_bytes`].
    pub fn add_figure(&mut self, figure: &FigureRef) -> Result<u64> {
        self.ensure_open()?;
        let payload = figure.to_bytes()?;
        self.chunks.push(PendingChunk {
            chunk_type: ChunkType::Figure,
            chunk_flags: chunk_flags::READING_ORDER,
            payload,
        });
        Ok(self.chunks.len() as u64)
    }

    /// Append a reading-order inert attachment chunk.
    ///
    /// Returns the 1-based `chunk_id` assigned on commit.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::SessionSealed`] if sealed, or encode/validation
    /// errors from [`AttachmentPayload::to_bytes`].
    pub fn add_attachment_chunk(&mut self, attachment: &AttachmentPayload) -> Result<u64> {
        self.ensure_open()?;
        let payload = attachment.to_bytes()?;
        self.chunks.push(PendingChunk {
            chunk_type: ChunkType::Attachment,
            chunk_flags: chunk_flags::READING_ORDER,
            payload,
        });
        Ok(self.chunks.len() as u64)
    }

    /// Append a reading-order cite chunk, mirroring a `TLNK` citation edge when targeted.
    ///
    /// Returns the 1-based `chunk_id` assigned on commit.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::SessionSealed`] if sealed, cite encode/validation
    /// errors, or [`TesError::InvalidDocId`] if `target_doc_id` is not a UUID.
    pub fn add_cite_chunk(&mut self, cite: &CitePayload) -> Result<u64> {
        self.ensure_open()?;
        let payload = cite.to_bytes()?;
        self.chunks.push(PendingChunk {
            chunk_type: ChunkType::Cite,
            chunk_flags: chunk_flags::READING_ORDER,
            payload,
        });
        let chunk_id = self.chunks.len() as u64;
        if let Some(doc_id) = cite.target_doc_id.as_deref() {
            let uuid = Uuid::parse_str(doc_id).map_err(|_| TesError::InvalidDocId {
                value: doc_id.to_owned(),
            })?;
            let end = u32::try_from(cite.quote.len()).unwrap_or(u32::MAX);
            self.links.push(LinkEntry::new(
                chunk_id,
                0,
                end,
                uuid,
                cite.target_chunk_id.unwrap_or(0),
                LinkKind::Citation,
            ));
        }
        Ok(chunk_id)
    }

    /// Append a reading-order slide with named region refs.
    ///
    /// Returns the 1-based `chunk_id` assigned on commit.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::SessionSealed`] if sealed, or encode/validation
    /// errors from [`SlidePayload::to_bytes`].
    pub fn add_slide(&mut self, slide: &SlidePayload) -> Result<u64> {
        self.ensure_open()?;
        let payload = slide.to_bytes()?;
        self.chunks.push(PendingChunk {
            chunk_type: ChunkType::Slide,
            chunk_flags: chunk_flags::READING_ORDER,
            payload,
        });
        Ok(self.chunks.len() as u64)
    }

    /// Append a reading-order layout chunk (`place` / `vspace` / `rule`).
    ///
    /// Returns the 1-based `chunk_id` assigned on commit.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::SessionSealed`] if sealed, or encode/validation
    /// errors from [`crate::catalog::LayoutPayload::to_bytes`].
    pub fn add_layout(&mut self, layout: &crate::catalog::LayoutPayload) -> Result<u64> {
        self.ensure_open()?;
        let payload = layout.to_bytes()?;
        self.chunks.push(PendingChunk {
            chunk_type: ChunkType::Layout,
            chunk_flags: chunk_flags::READING_ORDER,
            payload,
        });
        Ok(self.chunks.len() as u64)
    }

    /// Number of link-table rows queued so far (next `link_id`).
    #[must_use]
    pub fn link_count(&self) -> usize {
        self.links.len()
    }

    /// Add an outbound/internal/external link-table edge.
    ///
    /// Returns the 0-based link-table index (`link_id` for [`crate::catalog::InlineKind::Link`]).
    ///
    /// # Errors
    ///
    /// Returns [`TesError::SessionSealed`] if the session was already committed.
    pub fn add_link(&mut self, link: LinkEntry) -> Result<u64> {
        self.ensure_open()?;
        let id = self.links.len() as u64;
        self.links.push(link);
        Ok(id)
    }

    /// Write a sealed `.tes` and consume the session.
    ///
    /// Creates the file with `create_new` (fails if it already exists).
    ///
    /// # Errors
    ///
    /// Returns [`TesError::SessionSealed`] if already sealed, catalog encode
    /// errors from [`Self::encode_file`], or [`TesError::Io`] on create/write.
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
    ///
    /// # Errors
    ///
    /// Returns [`TesError::SessionSealed`] if sealed,
    /// [`TesError::CatalogTooLarge`] / [`TesError::Json`] when encoding the
    /// catalog, or [`TesError::IndexLengthMismatch`] if the chunk count would
    /// overflow the index region size.
    pub fn encode_file(&self) -> Result<Vec<u8>> {
        self.ensure_open()?;

        let catalog_bytes = match &self.catalog {
            Some(cat) => {
                let mut cat = cat.clone();
                cat.features.merge(&self.inferred_features());
                Some(cat.to_bytes()?)
            }
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

        let link_bytes = if self.links.is_empty() {
            Vec::new()
        } else {
            encode_link_table(&self.links)
        };
        let link_table_region = if link_bytes.is_empty() {
            Region::NONE
        } else {
            let offset = align8(cursor);
            let length = link_bytes.len() as u64;
            cursor = align8(offset + length);
            Region::new(offset, length)
        };

        let (chunk_index_region, index_bytes, payload_blobs) = if self.chunks.is_empty() {
            (Region::NONE, Vec::new(), Vec::new())
        } else {
            let header = ChunkIndexHeader::new(self.chunks.len() as u64);
            let index_len = header.region_len().ok_or(TesError::IndexLengthMismatch {
                expected: 0,
                got: header.entry_count,
            })?;
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
            link_table: link_table_region,
            chunk_index: chunk_index_region,
        };

        let mut out = Vec::with_capacity(cursor as usize);
        out.extend_from_slice(&sb.to_bytes());

        if let Some(cat) = catalog_bytes {
            pad_to(&mut out, catalog_region.offset as usize);
            out.extend_from_slice(&cat);
        }

        if !link_bytes.is_empty() {
            pad_to(&mut out, link_table_region.offset as usize);
            out.extend_from_slice(&link_bytes);
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

    /// Optional features implied by pending chunks / links (`layout_version` stays 0).
    fn inferred_features(&self) -> FeatureSet {
        let mut features = FeatureSet::default();
        for chunk in &self.chunks {
            match chunk.chunk_type {
                ChunkType::Attachment => features.declare_optional(feature_ids::ATTACHMENTS),
                ChunkType::Cite => features.declare_optional(feature_ids::CITATIONS),
                ChunkType::Slide => features.declare_optional(feature_ids::SLIDES),
                ChunkType::Layout => features.declare_optional(feature_ids::LAYOUT),
                ChunkType::Image | ChunkType::Figure => {
                    features.declare_optional(feature_ids::FIGURES);
                }
                ChunkType::Text => {
                    if let Ok((header, _)) = decode_text_payload(&chunk.payload)
                        && header.uses_layout_v1_features()
                    {
                        features.declare_optional(feature_ids::TEXT_SPANS);
                    }
                }
                _ => {}
            }
        }
        for link in &self.links {
            if matches!(link.target, LinkTarget::External { .. }) {
                features.declare_optional(feature_ids::EXTERNAL_URIS);
            }
            if link.link_kind == LinkKind::Citation {
                features.declare_optional(feature_ids::CITATIONS);
            }
        }
        features
    }
}

fn pad_to(buf: &mut Vec<u8>, offset: usize) {
    if buf.len() < offset {
        buf.resize(offset, 0);
    }
}

/// Row-major `(row, col, start, end)` for temporary / sealed cell Link spans.
fn table_link_slots(header: &TextHeader) -> Vec<(usize, usize, u32, u32)> {
    let Some(table) = header.table.as_ref() else {
        return Vec::new();
    };
    let mut found: Vec<(u64, usize, usize, u32, u32)> = Vec::new();
    for (ri, row) in table.rows.iter().enumerate() {
        for (ci, cell) in row.cells.iter().enumerate() {
            for span in &cell.spans {
                if let InlineKind::Link { link_id } = span.kind {
                    found.push((link_id, ri, ci, span.start, span.end));
                }
            }
        }
    }
    found.sort_by_key(|(id, _, _, _, _)| *id);
    found
        .into_iter()
        .map(|(_, ri, ci, start, end)| (ri, ci, start, end))
        .collect()
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
