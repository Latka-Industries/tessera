//! Individual repair implementations.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use crate::catalog::TesWriterSession;
use crate::catalog::document::DocumentCatalog;
use crate::catalog::history::footer_suffix_len;
use crate::catalog::index::{
    ChunkIndexEntry, ChunkIndexHeader, ENTRY_LEN, HEADER_LEN, MAGIC as TIDX_MAGIC,
};
use crate::catalog::link::{LinkEntry, read_link_table};
use crate::error::{Result, TesError};
use crate::layout::{DocKind, SUPERBLOCK_LEN, SuperblockV0, flags};

use super::RepairActionResult;

/// Clear invalid `HISTORY_FOOTER` / strip a broken THST trailer.
pub(super) fn apply_footer_invalid(
    working: &mut Vec<u8>,
    target: &Path,
    dry_run: bool,
) -> Result<RepairActionResult> {
    if working.len() < SUPERBLOCK_LEN {
        return Ok(RepairActionResult {
            code: "footer_invalid".to_owned(),
            applied: false,
            dry_run,
            message: "file too short for superblock; cannot clear history flag".to_owned(),
        });
    }

    let mut sb = SuperblockV0::from_bytes(working)?;
    if !sb.has_history_footer() {
        return Ok(RepairActionResult {
            code: "footer_invalid".to_owned(),
            applied: false,
            dry_run,
            message: "history footer flag already clear".to_owned(),
        });
    }

    let mut truncate_to = working.len();
    let mut note = String::from("clear HISTORY_FOOTER flag");
    if working.len() >= 4 && &working[working.len() - 4..] == b"THST" {
        if let Some(suffix) = footer_suffix_len(working) {
            truncate_to = working.len() - suffix;
            note = format!("strip {suffix}-byte THST trailer and clear HISTORY_FOOTER flag");
        } else {
            note = String::from(
                "clear HISTORY_FOOTER flag (THST magic present but trailer length invalid; left in place)",
            );
        }
    }

    if dry_run {
        return Ok(RepairActionResult {
            code: "footer_invalid".to_owned(),
            applied: false,
            dry_run: true,
            message: format!(
                "would {note} (from {} bytes → {truncate_to})",
                working.len()
            ),
        });
    }

    sb.flags &= !flags::HISTORY_FOOTER;
    working[..SUPERBLOCK_LEN].copy_from_slice(&sb.to_bytes());
    working.truncate(truncate_to);
    write_bytes(target, working)?;
    Ok(RepairActionResult {
        code: "footer_invalid".to_owned(),
        applied: true,
        dry_run: false,
        message: format!("{note}; now {} bytes", working.len()),
    })
}

/// Drop chunk index rows whose payloads extend past EOF; rewrite a sealed body.
///
/// Also clears history (THST rebuild is out of scope). Preserves catalog and
/// link table when they parse; otherwise omits them rather than inventing data.
pub(super) fn apply_drop_oob_chunks(
    working: &mut Vec<u8>,
    target: &Path,
    dry_run: bool,
    also_clear_footer: bool,
) -> Result<RepairActionResult> {
    if working.len() < SUPERBLOCK_LEN {
        return Ok(drop_oob_skip(
            dry_run,
            "file too short for superblock; cannot salvage chunks",
        ));
    }

    let sb = SuperblockV0::from_bytes(working)?;
    let catalog = match load_salvage_catalog(working, &sb, dry_run) {
        Ok(cat) => cat,
        Err(result) => return Ok(result),
    };
    let links = load_salvage_links(working, &sb);
    let (entries, index_err) = read_index_entries(working, &sb);
    if let Some(err) = index_err {
        return Ok(drop_oob_skip(
            dry_run,
            format!("chunk index unreadable ({err}); cannot salvage"),
        ));
    }

    let file_len = working.len() as u64;
    let (kept, dropped) = partition_in_bounds(&entries, working, file_len);
    if dropped.is_empty() && !also_clear_footer && !sb.has_history_footer() {
        return Ok(drop_oob_skip(dry_run, "no out-of-bounds chunks to drop"));
    }

    let msg = drop_oob_message(&kept, &dropped);
    if dry_run {
        return Ok(RepairActionResult {
            code: "drop_oob_chunks".to_owned(),
            applied: false,
            dry_run: true,
            message: format!("would {msg}"),
        });
    }

    let bytes = rewrite_salvaged(target, sb.doc_kind, catalog, links, kept)?;
    write_bytes(target, &bytes)?;
    *working = bytes;
    Ok(RepairActionResult {
        code: "drop_oob_chunks".to_owned(),
        applied: true,
        dry_run: false,
        message: msg,
    })
}

fn drop_oob_skip(dry_run: bool, message: impl Into<String>) -> RepairActionResult {
    RepairActionResult {
        code: "drop_oob_chunks".to_owned(),
        applied: false,
        dry_run,
        message: message.into(),
    }
}

fn load_salvage_catalog(
    working: &[u8],
    sb: &SuperblockV0,
    dry_run: bool,
) -> std::result::Result<Option<DocumentCatalog>, RepairActionResult> {
    if !sb.catalog.is_present() {
        return Ok(None);
    }
    match sb.catalog.slice(working, "catalog") {
        Ok(slice) => match DocumentCatalog::from_bytes(slice) {
            Ok(cat) => Ok(Some(cat)),
            Err(err) => Err(drop_oob_skip(
                dry_run,
                format!("catalog unreadable ({err}); refuse to invent catalog"),
            )),
        },
        Err(err) => Err(drop_oob_skip(
            dry_run,
            format!("catalog bounds invalid ({err})"),
        )),
    }
}

fn load_salvage_links(working: &[u8], sb: &SuperblockV0) -> Option<Vec<LinkEntry>> {
    if sb.link_table.is_present() {
        match sb.link_table.slice(working, "link_table") {
            Ok(region) => read_link_table(region).ok(),
            Err(_) => None,
        }
    } else {
        Some(Vec::new())
    }
}

fn partition_in_bounds<'a>(
    entries: &'a [ChunkIndexEntry],
    working: &[u8],
    file_len: u64,
) -> (Vec<(&'a ChunkIndexEntry, Vec<u8>)>, Vec<u64>) {
    let mut kept = Vec::new();
    let mut dropped = Vec::new();
    for entry in entries {
        let end = entry.payload_offset.saturating_add(entry.stored_byte_len);
        if end <= file_len {
            let start = entry.payload_offset as usize;
            let stop = end as usize;
            kept.push((entry, working[start..stop].to_vec()));
        } else {
            dropped.push(entry.chunk_id);
        }
    }
    (kept, dropped)
}

fn drop_oob_message(kept: &[(&ChunkIndexEntry, Vec<u8>)], dropped: &[u64]) -> String {
    if dropped.is_empty() {
        format!(
            "rewrite {} kept chunk(s), clear history flag (no OOB drops)",
            kept.len()
        )
    } else {
        format!(
            "drop chunk id(s) {dropped:?}; rewrite {} kept chunk(s); omit THST",
            kept.len()
        )
    }
}

fn rewrite_salvaged(
    target: &Path,
    doc_kind: DocKind,
    catalog: Option<DocumentCatalog>,
    links: Option<Vec<LinkEntry>>,
    kept: Vec<(&ChunkIndexEntry, Vec<u8>)>,
) -> Result<Vec<u8>> {
    let mut session = TesWriterSession::create(target, doc_kind);
    if let Some(cat) = catalog {
        session.set_catalog(cat)?;
    }
    if let Some(link_entries) = links {
        for link in link_entries {
            session.add_link(link)?;
        }
    }
    for (entry, payload) in kept {
        let _ = session.add_payload_chunk(entry.chunk_type, entry.chunk_flags, payload)?;
    }
    session.encode_file()
}

fn read_index_entries(bytes: &[u8], sb: &SuperblockV0) -> (Vec<ChunkIndexEntry>, Option<String>) {
    if !sb.chunk_index.is_present() {
        return (Vec::new(), None);
    }
    let region = match sb.chunk_index.slice(bytes, "chunk_index") {
        Ok(r) => r,
        Err(err) => return (Vec::new(), Some(err.to_string())),
    };
    if region.len() < HEADER_LEN {
        return (
            Vec::new(),
            Some(format!("index region too small ({})", region.len())),
        );
    }
    if region[0..4] != TIDX_MAGIC {
        return (Vec::new(), Some("bad TIDX magic".to_owned()));
    }
    let header = match ChunkIndexHeader::from_bytes(region) {
        Ok(h) => h,
        Err(err) => return (Vec::new(), Some(err.to_string())),
    };
    let Some(expected) = header.region_len() else {
        return (
            Vec::new(),
            Some(format!(
                "entry_count {} × index entry overflows u64",
                header.entry_count
            )),
        );
    };
    if expected != region.len() as u64 {
        return (
            Vec::new(),
            Some(format!(
                "index length mismatch (header {expected}, region {})",
                region.len()
            )),
        );
    }
    let Ok(count) = usize::try_from(header.entry_count) else {
        return (
            Vec::new(),
            Some(format!(
                "entry_count {} does not fit usize",
                header.entry_count
            )),
        );
    };
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let start = HEADER_LEN + i * ENTRY_LEN;
        match ChunkIndexEntry::from_bytes(&region[start..]) {
            Ok(entry) => entries.push(entry),
            Err(err) => return (Vec::new(), Some(format!("row {i}: {err}"))),
        }
    }
    (entries, None)
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    // Atomic-ish: write sibling temp then rename when replacing existing.
    let tmp = path.with_extension("tes.repair-tmp");
    {
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path).map_err(|err| {
        let _ = std::fs::remove_file(&tmp);
        TesError::Io(err)
    })?;
    Ok(())
}
