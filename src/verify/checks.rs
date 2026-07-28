//! Structural checks over a mapped or in-memory `.tes` image.
//!
//! Entry points: [`verify_tes_file`] and [`verify_bytes`].

use std::path::Path;

use crate::catalog::document::DocumentCatalog;
use crate::catalog::index::{
    ChunkIndexEntry, ChunkIndexHeader, Codec, ENTRY_LEN, HEADER_LEN, MAGIC as TIDX_MAGIC,
};
use crate::catalog::link::read_link_table;
use crate::error::Result;
use crate::layout::{self, MAGIC as TESS_MAGIC, Region, SUPERBLOCK_LEN};

use super::report::{Finding, Severity, TesVerifyReport};

/// Verify the `.tes` file at `path`.
///
/// `deep` additionally decodes every payload (codec + UTF-8 validation for text).
///
/// # Errors
///
/// Returns [`crate::error::TesError::Io`] if the file cannot be opened or memory-mapped.
pub fn verify_tes_file(path: impl AsRef<Path>, deep: bool) -> Result<TesVerifyReport> {
    let path = path.as_ref();
    let mmap = layout::open_mmap(path)?;
    Ok(verify_bytes(path, &mmap, deep))
}

/// Verify an in-memory image of a `.tes` file.
#[must_use]
pub fn verify_bytes(path: &Path, bytes: &[u8], deep: bool) -> TesVerifyReport {
    let file_len = bytes.len() as u64;
    let mut findings = Vec::new();

    let Some(superblock) = parse_superblock(bytes, &mut findings) else {
        return finish(path, file_len, 0, deep, findings);
    };

    check_region_bounds(&mut findings, "catalog", superblock.catalog, file_len);
    check_region_bounds(&mut findings, "link_table", superblock.link_table, file_len);
    check_region_bounds(
        &mut findings,
        "chunk_index",
        superblock.chunk_index,
        file_len,
    );

    if superblock.catalog.length == 0 && superblock.catalog.offset != 0 {
        findings.push(Finding::warning(
            "catalog.offset",
            "catalog_length is 0 but catalog_offset is non-zero",
        ));
    }

    verify_catalog_region(&mut findings, &superblock, bytes);
    verify_link_table_region(&mut findings, &superblock, bytes);
    let (chunk_count, entries) = verify_chunk_index_region(&mut findings, &superblock, bytes);
    verify_payload_bounds(&mut findings, &entries, bytes, file_len, deep);
    verify_figure_targets(&mut findings, &entries, bytes);
    verify_slide_targets(&mut findings, &entries, bytes);
    verify_cite_mirrors(&mut findings, &entries, &superblock, bytes);
    verify_attachment_limits(&mut findings, &entries, bytes, deep);
    verify_history_footer(&mut findings, &superblock, bytes, file_len);

    finish(path, file_len, chunk_count, deep, findings)
}

fn parse_superblock(
    bytes: &[u8],
    findings: &mut Vec<Finding>,
) -> Option<crate::layout::SuperblockV0> {
    if bytes.len() < SUPERBLOCK_LEN {
        findings.push(Finding::error(
            "superblock.size",
            format!(
                "file is {} bytes; superblock needs {SUPERBLOCK_LEN}",
                bytes.len()
            ),
        ));
        return None;
    }
    if bytes[0..4] != TESS_MAGIC {
        findings.push(Finding::error(
            "superblock.magic",
            format!("expected {TESS_MAGIC:?}, found {:?}", &bytes[0..4]),
        ));
        return None;
    }
    match crate::layout::SuperblockV0::from_bytes(bytes) {
        Ok(sb) => Some(sb),
        Err(err) => {
            findings.push(Finding::error("superblock.parse", err.to_string()));
            None
        }
    }
}

fn verify_catalog_region(
    findings: &mut Vec<Finding>,
    superblock: &crate::layout::SuperblockV0,
    bytes: &[u8],
) {
    if !superblock.catalog.is_present() {
        return;
    }
    match superblock.catalog.slice(bytes, "catalog") {
        Ok(slice) => match DocumentCatalog::from_bytes(slice) {
            Ok(cat) => {
                if cat.doc_id.is_empty() {
                    findings.push(Finding::error("catalog.doc_id", "doc_id is empty"));
                }
                if cat.doc_kind != superblock.doc_kind.as_str() {
                    findings.push(Finding::warning(
                        "catalog.doc_kind",
                        format!(
                            "catalog doc_kind '{}' differs from superblock '{}'",
                            cat.doc_kind,
                            superblock.doc_kind.as_str()
                        ),
                    ));
                }
            }
            Err(err) => findings.push(Finding::error("catalog.json", err.to_string())),
        },
        Err(err) => findings.push(Finding::error("catalog.bounds", err.to_string())),
    }
}

fn verify_link_table_region(
    findings: &mut Vec<Finding>,
    superblock: &crate::layout::SuperblockV0,
    bytes: &[u8],
) {
    if !superblock.link_table.is_present() {
        return;
    }
    match superblock.link_table.slice(bytes, "link_table") {
        Ok(region) => {
            if let Err(err) = read_link_table(region) {
                findings.push(Finding::error("link_table.parse", err.to_string()));
            }
        }
        Err(err) => findings.push(Finding::error("link_table.bounds", err.to_string())),
    }
}

fn verify_chunk_index_region(
    findings: &mut Vec<Finding>,
    superblock: &crate::layout::SuperblockV0,
    bytes: &[u8],
) -> (u64, Vec<ChunkIndexEntry>) {
    let mut chunk_count = 0u64;
    let mut entries = Vec::new();
    if !superblock.chunk_index.is_present() {
        return (chunk_count, entries);
    }
    match superblock.chunk_index.slice(bytes, "chunk_index") {
        Ok(region) => {
            if region.len() < HEADER_LEN {
                findings.push(Finding::error(
                    "chunk_index.size",
                    format!(
                        "region is {} bytes; header needs {HEADER_LEN}",
                        region.len()
                    ),
                ));
            } else if region[0..4] != TIDX_MAGIC {
                findings.push(Finding::error(
                    "chunk_index.magic",
                    format!("expected {TIDX_MAGIC:?}, found {:?}", &region[0..4]),
                ));
            } else {
                match ChunkIndexHeader::from_bytes(region) {
                    Ok(header) => {
                        chunk_count = header.entry_count;
                        let expected = header.region_len();
                        if expected == region.len() as u64 {
                            for i in 0..header.entry_count as usize {
                                let start = HEADER_LEN + i * ENTRY_LEN;
                                match ChunkIndexEntry::from_bytes(&region[start..]) {
                                    Ok(entry) => entries.push(entry),
                                    Err(err) => findings.push(Finding::error(
                                        "chunk_index.entry",
                                        format!("row {i}: {err}"),
                                    )),
                                }
                            }
                        } else {
                            findings.push(Finding::error(
                                "chunk_index.length",
                                format!(
                                    "header implies {expected} bytes (32 + {}×48), region is {}",
                                    header.entry_count,
                                    region.len()
                                ),
                            ));
                        }
                    }
                    Err(err) => {
                        findings.push(Finding::error("chunk_index.header", err.to_string()));
                    }
                }
            }
        }
        Err(err) => findings.push(Finding::error("chunk_index.bounds", err.to_string())),
    }
    (chunk_count, entries)
}

fn verify_payload_bounds(
    findings: &mut Vec<Finding>,
    entries: &[ChunkIndexEntry],
    bytes: &[u8],
    file_len: u64,
    deep: bool,
) {
    let has_history = file_len >= 64
        && (u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]])
            & crate::layout::flags::HISTORY_FOOTER
            != 0);
    let usable_len = crate::catalog::history::usable_file_len(bytes, has_history);
    let _ = file_len;
    for entry in entries {
        let check_id = "chunk.payload_bounds";
        match entry.payload_offset.checked_add(entry.stored_byte_len) {
            None => findings.push(Finding::error(
                check_id,
                format!(
                    "chunk {} offset {} + length {} overflows u64",
                    entry.chunk_id, entry.payload_offset, entry.stored_byte_len
                ),
            )),
            Some(end) if end > usable_len => findings.push(Finding::error(
                check_id,
                format!(
                    "chunk {} spans {}..{} beyond usable_len {usable_len}",
                    entry.chunk_id, entry.payload_offset, end
                ),
            )),
            Some(end) => {
                if entry.codec == Codec::Raw && entry.stored_byte_len != entry.raw_byte_len {
                    findings.push(Finding::warning(
                        "chunk.codec",
                        format!(
                            "chunk {} is raw but stored_byte_len {} != raw_byte_len {}",
                            entry.chunk_id, entry.stored_byte_len, entry.raw_byte_len
                        ),
                    ));
                }
                if deep {
                    verify_payload_decode(
                        findings,
                        entry,
                        &bytes[entry.payload_offset as usize..end as usize],
                    );
                }
            }
        }
    }
}

fn verify_history_footer(
    findings: &mut Vec<Finding>,
    superblock: &crate::layout::SuperblockV0,
    bytes: &[u8],
    file_len: u64,
) {
    if !superblock.has_history_footer() {
        return;
    }
    if file_len < 16 || &bytes[file_len as usize - 4..] != b"THST" {
        findings.push(Finding::error(
            "history.footer",
            "flags set HISTORY_FOOTER but THST magic missing at EOF",
        ));
        return;
    }
    match crate::catalog::history::footer_suffix_len(bytes) {
        None => findings.push(Finding::error(
            "history.footer",
            "THST magic present but trailer length is invalid",
        )),
        Some(suffix) => {
            if let Err(err) = crate::catalog::history::decode_footer(&bytes[bytes.len() - suffix..])
            {
                findings.push(Finding::error("history.decode", err.to_string()));
            }
        }
    }
}

fn verify_payload_decode(findings: &mut Vec<Finding>, entry: &ChunkIndexEntry, payload: &[u8]) {
    use crate::catalog::chunk::{CitePayload, decode_text_payload};
    use crate::catalog::index::ChunkType;
    use crate::catalog::media::{AttachmentPayload, FigureRef, ImagePayload};
    use crate::catalog::slide::SlidePayload;

    match entry.codec {
        Codec::Raw => {}
        Codec::Zstd => {
            let codec = argus::PayloadCodec::Zstd;
            match argus::decode(codec, payload, entry.raw_byte_len) {
                Ok(_) => {}
                Err(err) => findings.push(Finding::error(
                    "chunk.decode",
                    format!("chunk {} decode failed: {err}", entry.chunk_id),
                )),
            }
            return;
        }
    }

    match entry.chunk_type {
        ChunkType::Text => {
            if let Err(err) = decode_text_payload(payload) {
                findings.push(Finding::error(
                    "chunk.text_payload",
                    format!("chunk {}: {err}", entry.chunk_id),
                ));
            }
        }
        ChunkType::Image => {
            if let Err(err) = ImagePayload::from_bytes(payload) {
                findings.push(Finding::error(
                    "chunk.image_payload",
                    format!("chunk {}: {err}", entry.chunk_id),
                ));
            }
        }
        ChunkType::Figure => {
            if let Err(err) = FigureRef::from_bytes(payload) {
                findings.push(Finding::error(
                    "chunk.figure_payload",
                    format!("chunk {}: {err}", entry.chunk_id),
                ));
            }
        }
        ChunkType::Attachment => {
            if let Err(err) = AttachmentPayload::from_bytes(payload) {
                findings.push(Finding::error(
                    "chunk.attachment_payload",
                    format!("chunk {}: {err}", entry.chunk_id),
                ));
            }
        }
        ChunkType::Cite => {
            if let Err(err) = CitePayload::from_bytes(payload) {
                findings.push(Finding::error(
                    "chunk.cite_payload",
                    format!("chunk {}: {err}", entry.chunk_id),
                ));
            }
        }
        ChunkType::Slide => {
            if let Err(err) = SlidePayload::from_bytes(payload) {
                findings.push(Finding::error(
                    "chunk.slide_payload",
                    format!("chunk {}: {err}", entry.chunk_id),
                ));
            }
        }
        _ => {}
    }
}

fn verify_cite_mirrors(
    findings: &mut Vec<Finding>,
    entries: &[ChunkIndexEntry],
    superblock: &crate::layout::SuperblockV0,
    bytes: &[u8],
) {
    use crate::catalog::chunk::CitePayload;
    use crate::catalog::index::ChunkType;
    use crate::catalog::link::{LinkKind, read_link_table};
    use uuid::Uuid;

    let links = if superblock.link_table.is_present() {
        match superblock.link_table.slice(bytes, "link_table") {
            Ok(region) => match read_link_table(region) {
                Ok(links) => links,
                Err(_) => return,
            },
            Err(_) => return,
        }
    } else {
        Vec::new()
    };

    for entry in entries {
        if entry.chunk_type != ChunkType::Cite {
            continue;
        }
        let start = entry.payload_offset as usize;
        let end = start + entry.stored_byte_len as usize;
        if end > bytes.len() {
            continue;
        }
        let Ok(cite) = CitePayload::from_bytes(&bytes[start..end]) else {
            continue;
        };
        let Some(doc_id) = cite.target_doc_id.as_deref() else {
            continue;
        };
        let Ok(uuid) = Uuid::parse_str(doc_id) else {
            continue;
        };
        let target_chunk = cite.target_chunk_id.unwrap_or(0);
        let mirrored = links.iter().any(|link| {
            link.link_kind == LinkKind::Citation
                && link.source_chunk_id == entry.chunk_id
                && link.target_uuid() == Some(uuid)
                && link.target_chunk_id() == Some(target_chunk)
        });
        if !mirrored {
            findings.push(Finding::warning(
                "cite.mirror",
                format!(
                    "cite {} targets {} but has no matching TLNK citation row",
                    entry.chunk_id, doc_id
                ),
            ));
        }
    }
}

fn verify_slide_targets(findings: &mut Vec<Finding>, entries: &[ChunkIndexEntry], bytes: &[u8]) {
    use crate::catalog::index::ChunkType;
    use crate::catalog::slide::SlidePayload;
    use std::collections::HashMap;

    let by_id: HashMap<u64, &ChunkIndexEntry> = entries.iter().map(|e| (e.chunk_id, e)).collect();

    for entry in entries {
        if entry.chunk_type != ChunkType::Slide {
            continue;
        }
        let start = entry.payload_offset as usize;
        let end = start + entry.stored_byte_len as usize;
        if end > bytes.len() {
            continue;
        }
        let Ok(slide) = SlidePayload::from_bytes(&bytes[start..end]) else {
            continue;
        };
        for region in &slide.regions {
            match by_id.get(&region.chunk_id) {
                None => findings.push(Finding::error(
                    "slide.target",
                    format!(
                        "slide {} region '{}' references missing chunk {}",
                        entry.chunk_id, region.name, region.chunk_id
                    ),
                )),
                Some(target) if !target.chunk_type.is_slide_region_target() => {
                    findings.push(Finding::error(
                        "slide.target",
                        format!(
                            "slide {} region '{}' references chunk {} of type '{}'",
                            entry.chunk_id,
                            region.name,
                            region.chunk_id,
                            target.chunk_type.as_str()
                        ),
                    ));
                }
                Some(_) => {}
            }
        }
        if entry.chunk_flags & crate::catalog::index::chunk_flags::READING_ORDER == 0 {
            findings.push(Finding::warning(
                "slide.reading_order",
                format!(
                    "slide {} is not marked reading-order; exports may skip it",
                    entry.chunk_id
                ),
            ));
        }
    }
}

fn verify_figure_targets(findings: &mut Vec<Finding>, entries: &[ChunkIndexEntry], bytes: &[u8]) {
    use crate::catalog::index::ChunkType;
    use crate::catalog::media::FigureRef;
    use std::collections::HashMap;

    let by_id: HashMap<u64, &ChunkIndexEntry> = entries.iter().map(|e| (e.chunk_id, e)).collect();

    for entry in entries {
        if entry.chunk_type != ChunkType::Figure {
            continue;
        }
        let start = entry.payload_offset as usize;
        let end = start + entry.stored_byte_len as usize;
        if end > bytes.len() {
            continue;
        }
        let Ok(figure) = FigureRef::from_bytes(&bytes[start..end]) else {
            continue;
        };
        match by_id.get(&figure.image_chunk_id) {
            None => findings.push(Finding::error(
                "figure.target",
                format!(
                    "figure {} references missing image chunk {}",
                    entry.chunk_id, figure.image_chunk_id
                ),
            )),
            Some(target) if target.chunk_type != ChunkType::Image => findings.push(Finding::error(
                "figure.target",
                format!(
                    "figure {} references chunk {} of type '{}', expected image",
                    entry.chunk_id,
                    figure.image_chunk_id,
                    target.chunk_type.as_str()
                ),
            )),
            Some(_) => {}
        }
        if entry.chunk_flags & crate::catalog::index::chunk_flags::READING_ORDER == 0 {
            findings.push(Finding::warning(
                "figure.reading_order",
                format!(
                    "figure {} is not marked reading-order; exports may skip it",
                    entry.chunk_id
                ),
            ));
        }
    }
}

fn verify_attachment_limits(
    findings: &mut Vec<Finding>,
    entries: &[ChunkIndexEntry],
    bytes: &[u8],
    deep: bool,
) {
    use crate::catalog::index::ChunkType;
    use crate::catalog::media::{
        ATTACHMENT_MAX_AGGREGATE_BYTES, ATTACHMENT_MAX_COUNT, AttachmentPayload,
    };

    let attachment_entries: Vec<&ChunkIndexEntry> = entries
        .iter()
        .filter(|e| e.chunk_type == ChunkType::Attachment)
        .collect();
    if attachment_entries.len() > ATTACHMENT_MAX_COUNT {
        findings.push(Finding::error(
            "attachment.count",
            format!(
                "{} attachment chunks exceeds limit {ATTACHMENT_MAX_COUNT}",
                attachment_entries.len()
            ),
        ));
    }

    let mut aggregate = 0u64;
    for entry in attachment_entries {
        let start = entry.payload_offset as usize;
        let end = start.saturating_add(entry.stored_byte_len as usize);
        if end > bytes.len() {
            continue;
        }
        let data_len = if deep && entry.codec == Codec::Raw {
            match AttachmentPayload::from_bytes(&bytes[start..end]) {
                Ok(att) => att.data.len() as u64,
                Err(_) => entry.raw_byte_len,
            }
        } else {
            entry.raw_byte_len
        };
        aggregate = aggregate.saturating_add(data_len);
    }
    if aggregate > ATTACHMENT_MAX_AGGREGATE_BYTES {
        findings.push(Finding::error(
            "attachment.aggregate_bytes",
            format!(
                "aggregate attachment bytes {aggregate} exceeds {ATTACHMENT_MAX_AGGREGATE_BYTES}"
            ),
        ));
    }
}

fn check_region_bounds(
    findings: &mut Vec<Finding>,
    name: &'static str,
    region: Region,
    file_len: u64,
) {
    if !region.is_present() {
        return;
    }
    match region.offset.checked_add(region.length) {
        None => findings.push(Finding::error(
            "region.bounds",
            format!("{name} offset + length overflows u64"),
        )),
        Some(end) if end > file_len => findings.push(Finding::error(
            "region.bounds",
            format!(
                "{name} spans {}..{end} beyond file_len {file_len}",
                region.offset
            ),
        )),
        Some(_) => {
            if !region.offset.is_multiple_of(8) {
                findings.push(Finding::warning(
                    "region.align",
                    format!("{name} offset {} is not 8-byte aligned", region.offset),
                ));
            }
        }
    }
}

fn finish(
    path: &Path,
    file_len: u64,
    chunk_count: u64,
    deep: bool,
    findings: Vec<Finding>,
) -> TesVerifyReport {
    let ok = !findings.iter().any(|f| f.severity == Severity::Error);
    TesVerifyReport {
        path: path.display().to_string(),
        file_len,
        ok,
        chunk_count,
        deep,
        findings,
    }
}
