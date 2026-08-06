//! Compile Tessprek [`ContentBlock`]s back into sealed `.tes` bytes.

use std::path::PathBuf;

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::catalog::OutboundLink;
use crate::catalog::chunk::{CitePayload, TextHeader};
use crate::catalog::document::DocumentCatalog;
use crate::catalog::history::attach_footer;
use crate::catalog::index::ChunkType;
use crate::catalog::media::{AttachmentPayload, ImagePayload};
use crate::catalog::{InlineKind, InlineSpan, TesFile, TesWriterSession};
use crate::error::{Result, TesError};
use crate::io::cite::{cite_key_from_payload, insert_cite_key};

use super::{CatalogPatch, ContentBlock, EditMediaBag};

/// Re-attach an existing THST footer so edits do not drop revision history.
pub(super) fn seal_with_history(source: &TesFile, body: Vec<u8>) -> Result<Vec<u8>> {
    match source.history()? {
        Some(history) => attach_footer(body, &history),
        None => Ok(body),
    }
}

/// Encode reading-order blocks (plus catalog/media) into a sealed `.tes` body.
///
/// # Errors
///
/// Returns decode/encode errors from payloads, catalog, or the writer session.
pub(super) fn compile_blocks_to_bytes(
    source: &TesFile,
    blocks: &[ContentBlock],
    catalog_patch: Option<&CatalogPatch>,
    media: &EditMediaBag,
) -> Result<Vec<u8>> {
    let catalog = catalog_for_compile(source, catalog_patch);
    let bag_images = media.image_map()?;
    let bag_attachments = media.attachment_map()?;
    let image_payloads = load_referenced_images(source, blocks, &bag_images)?;

    // Build into an ephemeral session path (encode_file only; no commit).
    let phantom = PathBuf::from("__tessera_edit_encode__.tes");
    let mut session = TesWriterSession::create(&phantom, source.superblock().doc_kind);
    session.set_catalog(catalog)?;

    let mut image_id_map = std::collections::HashMap::new();
    for (old_id, payload) in &image_payloads {
        let new_id = session.add_image_chunk(payload)?;
        image_id_map.insert(*old_id, new_id);
    }

    let cite_keys = predicted_cite_key_map(blocks, image_payloads.len() as u64)?;

    for block in blocks {
        write_compiled_block(
            &mut session,
            source,
            block,
            &image_id_map,
            &bag_attachments,
            &cite_keys,
        )?;
    }
    session.encode_file()
}

/// Map cite keys → chunk ids that the upcoming reading-order write will assign
/// (after `image_count` image payloads already queued).
fn predicted_cite_key_map(
    blocks: &[ContentBlock],
    image_count: u64,
) -> Result<std::collections::BTreeMap<String, u64>> {
    let mut map = std::collections::BTreeMap::new();
    let mut next = image_count.saturating_add(1);
    for block in blocks {
        if let ContentBlock::Cite { cite, .. } = block
            && let Some(key) = cite_key_from_payload(cite)
        {
            insert_cite_key(&mut map, &key, next)?;
        }
        next = next.saturating_add(1);
    }
    Ok(map)
}

fn catalog_for_compile(source: &TesFile, patch: Option<&CatalogPatch>) -> DocumentCatalog {
    let doc_kind = source.superblock().doc_kind;
    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into());
    let mut catalog = source.catalog().cloned().unwrap_or_else(|| {
        DocumentCatalog::new(
            uuid::Uuid::new_v4().to_string(),
            "Untitled",
            now.clone(),
            now.clone(),
            doc_kind,
        )
    });
    if let Some(patch) = patch {
        patch.apply_to(&mut catalog);
    }
    catalog.modified = now;
    catalog
}

fn load_referenced_images(
    source: &TesFile,
    blocks: &[ContentBlock],
    bag_images: &std::collections::HashMap<u64, &ImagePayload>,
) -> Result<Vec<(u64, ImagePayload)>> {
    let mut needed_images: Vec<u64> = blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Figure { figure, .. } => Some(figure.image_chunk_id),
            _ => None,
        })
        .collect();
    needed_images.sort_unstable();
    needed_images.dedup();

    let mut image_payloads = Vec::with_capacity(needed_images.len());
    for old_id in needed_images {
        if let Some(payload) = bag_images.get(&old_id) {
            payload.validate()?;
            image_payloads.push((old_id, (*payload).clone()));
            continue;
        }
        let raw = source_payload_bytes(source, old_id, ChunkType::Image, "image")?;
        image_payloads.push((old_id, ImagePayload::from_bytes(raw.as_ref())?));
    }
    Ok(image_payloads)
}

fn source_payload_bytes<'a>(
    source: &'a TesFile,
    chunk_id: u64,
    expected: ChunkType,
    kind: &str,
) -> Result<std::borrow::Cow<'a, [u8]>> {
    let entry = source.chunk_by_id(chunk_id).map_err(|_| TesError::EditOp {
        message: format!("{kind} chunk {chunk_id} missing from source and media bag"),
    })?;
    if entry.chunk_type != expected {
        return Err(TesError::EditOp {
            message: format!("chunk {chunk_id} is not a {kind}"),
        });
    }
    source.decode_payload(entry)
}

fn write_compiled_block(
    session: &mut TesWriterSession,
    source: &TesFile,
    block: &ContentBlock,
    image_id_map: &std::collections::HashMap<u64, u64>,
    bag_attachments: &std::collections::HashMap<u64, &AttachmentPayload>,
    cite_keys: &std::collections::BTreeMap<String, u64>,
) -> Result<()> {
    match block {
        ContentBlock::Text {
            header,
            body,
            pending_links,
            pending_cites,
            pending_faces,
            ..
        } => {
            let mut header = header.clone();
            if !pending_cites.is_empty() {
                header
                    .spans
                    .retain(|s| !matches!(s.kind, InlineKind::Citation { .. }));
                for pending in pending_cites {
                    let Some(&cite_chunk_id) = cite_keys.get(pending.key.as_str()) else {
                        return Err(TesError::EditOp {
                            message: format!("unknown cite key '{}'", pending.key),
                        });
                    };
                    header.spans.push(InlineSpan {
                        start: pending.start,
                        end: pending.end,
                        kind: InlineKind::Citation { cite_chunk_id },
                    });
                }
            }
            if !pending_faces.is_empty() {
                header
                    .spans
                    .retain(|s| !matches!(s.kind, InlineKind::Face { .. }));
                for pending in pending_faces {
                    header.spans.push(InlineSpan {
                        start: pending.start,
                        end: pending.end,
                        kind: InlineKind::Face {
                            face_id: pending.face_id.clone(),
                        },
                    });
                }
            }
            let outbound = text_outbound_links(source, &header, pending_links);
            session.add_text_with_outbound_links(header, body, &outbound)?;
        }
        ContentBlock::Figure { figure, .. } => {
            let mut figure = figure.clone();
            let Some(&new_id) = image_id_map.get(&figure.image_chunk_id) else {
                return Err(TesError::EditOp {
                    message: format!("missing image payload for chunk {}", figure.image_chunk_id),
                });
            };
            figure.image_chunk_id = new_id;
            session.add_figure(&figure)?;
        }
        ContentBlock::Cite { cite, .. } => {
            write_cite_block(session, source, block, cite)?;
        }
        ContentBlock::Slide { slide, .. } => {
            session.add_slide(slide)?;
        }
        ContentBlock::Attachment { .. } => {
            write_attachment_block(session, source, bag_attachments, block)?;
        }
    }
    Ok(())
}

fn text_outbound_links(
    source: &TesFile,
    header: &TextHeader,
    pending_links: &[OutboundLink],
) -> Vec<OutboundLink> {
    if !pending_links.is_empty() {
        return pending_links.to_vec();
    }
    // Remap existing Link spans from the source TLNK.
    header
        .spans
        .iter()
        .filter_map(|span| {
            let InlineKind::Link { link_id } = &span.kind else {
                return None;
            };
            let entry = source.links().get(*link_id as usize)?;
            Some(OutboundLink {
                start: span.start,
                end: span.end,
                dest: entry.target.markdown_destination(),
            })
        })
        .collect()
}

/// Persist a cite block, preferring Tessprek-provided biblio `source` over a
/// stale on-disk payload reused by chunk id.
fn write_cite_block(
    session: &mut TesWriterSession,
    source: &TesFile,
    block: &ContentBlock,
    cite: &CitePayload,
) -> Result<u64> {
    // Quote/ref and biblio stubs that carry `source` from Tessprek attrs are
    // authoritative — do not keep a stale BibEntry from a reused chunk id.
    if !crate::io::cite::is_biblio_cite(cite) || cite.source.is_some() {
        return session.add_cite_chunk(cite);
    }
    // Label-only biblio `\cite{label=…}`: merge onto the on-disk cite so an
    // attrs-free rename does not wipe `source`.
    if let Some(id) = block.chunk_id()
        && let Ok(entry) = source.chunk_by_id(id)
        && entry.chunk_type == ChunkType::Cite
    {
        let raw = source.decode_payload(entry)?;
        let mut full = CitePayload::from_bytes(raw.as_ref())?;
        full.quote.clone_from(&cite.quote);
        merge_opt(&mut full.label, cite.label.as_ref());
        merge_opt(&mut full.target_doc_id, cite.target_doc_id.as_ref());
        merge_opt(&mut full.target_chunk_id, cite.target_chunk_id.as_ref());
        merge_opt(&mut full.target_byte_start, cite.target_byte_start.as_ref());
        merge_opt(&mut full.target_byte_end, cite.target_byte_end.as_ref());
        merge_opt(&mut full.page, cite.page.as_ref());
        return session.add_cite_chunk(&full);
    }
    session.add_cite_chunk(cite)
}

/// Copy `src` over `dst` when Tessprek provided a value (omit → keep source).
fn merge_opt<T: Clone>(dst: &mut Option<T>, src: Option<&T>) {
    if let Some(value) = src {
        *dst = Some(value.clone());
    }
}

fn write_attachment_block(
    session: &mut TesWriterSession,
    source: &TesFile,
    bag_attachments: &std::collections::HashMap<u64, &AttachmentPayload>,
    block: &ContentBlock,
) -> Result<()> {
    let ContentBlock::Attachment {
        chunk_id,
        filename,
        media_type,
        caption,
        sha256,
    } = block
    else {
        return Err(TesError::EditOp {
            message: "internal: write_attachment_block expected Attachment".into(),
        });
    };
    let Some(id) = *chunk_id else {
        return Err(TesError::EditOp {
            message: "attachment directives require a chunk id (source or media bag)".into(),
        });
    };

    let mut payload = if let Some(bag) = bag_attachments.get(&id) {
        (*bag).clone()
    } else {
        let raw = source_payload_bytes(source, id, ChunkType::Attachment, "attachment")?;
        AttachmentPayload::from_bytes(raw.as_ref())?
    };

    // Allow Tessprek metadata edits that keep the same bytes.
    if payload.sha256 != *sha256 {
        return Err(TesError::EditOp {
            message: format!(
                "attachment chunk {id} sha256 mismatch: tessprek={sha256}, payload={}",
                payload.sha256
            ),
        });
    }
    payload.filename.clone_from(filename);
    payload.media_type.clone_from(media_type);
    payload.caption.clone_from(caption);
    payload.validate()?;
    session.add_attachment_chunk(&payload)?;
    Ok(())
}
