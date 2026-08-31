use crate::catalog::TesFile;
use crate::catalog::chunk::{CitePayload, decode_text_payload};
use crate::catalog::index::ChunkType;
use crate::catalog::layout::LayoutPayload;
use crate::catalog::media::{AttachmentPayload, FigureRef, ImagePayload};
use crate::catalog::slide::SlidePayload;
use crate::error::Result;

use super::super::ContentBlock;
use super::types::{TessprekDocMeta, TessprekMediaEntry};
use super::write::encode_content_blocks;

/// Encode a `.tes` file as Tessprek, embedding `source_hash`.
///
/// # Errors
///
/// Returns decode errors for reading-order text/figure/cite/slide/attachment
/// payloads.
pub fn encode_tessprek(file: &TesFile, source_hash: &str) -> Result<String> {
    let mut blocks = Vec::new();
    for entry in file.reading_order_chunks() {
        let block = match entry.chunk_type {
            ChunkType::Text => {
                let raw = file.decode_payload(entry)?;
                let (header, body) = decode_text_payload(raw.as_ref())?;
                ContentBlock::Text {
                    chunk_id: Some(entry.chunk_id),
                    header,
                    body,
                    pending_links: Vec::new(),
                    pending_cites: Vec::new(),
                    pending_fonts: Vec::new(),
                    pending_notes: Vec::new(),
                }
            }
            ChunkType::Figure => {
                let raw = file.decode_payload(entry)?;
                let figure = FigureRef::from_bytes(raw.as_ref())?;
                ContentBlock::Figure {
                    chunk_id: Some(entry.chunk_id),
                    figure,
                }
            }
            ChunkType::Cite => {
                let raw = file.decode_payload(entry)?;
                let cite = CitePayload::from_bytes(raw.as_ref())?;
                ContentBlock::Cite {
                    chunk_id: Some(entry.chunk_id),
                    cite,
                }
            }
            ChunkType::Slide => {
                let raw = file.decode_payload(entry)?;
                let slide = SlidePayload::from_bytes(raw.as_ref())?;
                ContentBlock::Slide {
                    chunk_id: Some(entry.chunk_id),
                    slide,
                }
            }
            ChunkType::Layout => {
                let raw = file.decode_payload(entry)?;
                let layout = LayoutPayload::from_bytes(raw.as_ref())?;
                ContentBlock::Layout {
                    chunk_id: Some(entry.chunk_id),
                    layout,
                }
            }
            ChunkType::Attachment => {
                let raw = file.decode_payload(entry)?;
                let att = AttachmentPayload::from_bytes(raw.as_ref())?;
                ContentBlock::Attachment {
                    chunk_id: Some(entry.chunk_id),
                    filename: att.filename,
                    media_type: att.media_type,
                    caption: att.caption,
                    sha256: att.sha256,
                }
            }
            _ => continue,
        };
        blocks.push(block);
    }
    let media = media_entries_from_file(file, &blocks);
    Ok(encode_content_blocks(
        &file.catalog().map_or_else(
            || TessprekDocMeta {
                source_hash: Some(source_hash.to_owned()),
                ..TessprekDocMeta::default()
            },
            |catalog| TessprekDocMeta::from_catalog(catalog, Some(source_hash)),
        ),
        &blocks,
        file.links(),
        &media,
    ))
}

/// Collect `\media{…}` rows for figure-referenced image payloads in `file`.
fn media_entries_from_file(file: &TesFile, blocks: &[ContentBlock]) -> Vec<TessprekMediaEntry> {
    let mut ids = std::collections::BTreeSet::new();
    for block in blocks {
        if let ContentBlock::Figure { figure, .. } = block
            && figure.image_chunk_id != 0
        {
            ids.insert(figure.image_chunk_id);
        }
    }
    ids.into_iter()
        .map(|id| match file.chunk_by_id(id) {
            Ok(entry) if entry.chunk_type == ChunkType::Image => file
                .decode_payload(entry)
                .ok()
                .and_then(|raw| ImagePayload::from_bytes(raw.as_ref()).ok())
                .map_or_else(
                    || TessprekMediaEntry {
                        chunk_id: id,
                        ..TessprekMediaEntry::default()
                    },
                    |image| TessprekMediaEntry::from_payload(id, &image),
                ),
            _ => TessprekMediaEntry {
                chunk_id: id,
                ..TessprekMediaEntry::default()
            },
        })
        .collect()
}
