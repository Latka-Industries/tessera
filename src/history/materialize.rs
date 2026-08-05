//! Materialize / export / checkout a historical revision.

use std::fs;
use std::path::Path;

use crate::catalog::document::DocumentCatalog;
use crate::catalog::history::{HistoryV1, attach_footer, split_body_and_history};
use crate::catalog::index::ChunkType;
use crate::catalog::session::TesWriterSession;
use crate::error::Result;
use crate::layout::{DocKind, SuperblockV0};

use super::atomic_replace;

/// Materialize a revision as a sealed `.tes` body (no `THST` footer).
///
/// Rebuilds catalog + chunk payloads from the exact-hash store. Link tables
/// (`TLNK`) are not stored in revision manifests and are omitted.
///
/// # Errors
///
/// Returns [`crate::error::TesError::RevisionNotFound`], missing store payloads,
/// or encode errors.
pub fn materialize_revision(history: &HistoryV1, rev: &str) -> Result<Vec<u8>> {
    let record = history.resolve(rev)?;
    let catalog_bytes = history.get_payload(&record.catalog_hash)?;

    let mut session = TesWriterSession::create("materialize.tes", DocKind::Note);
    if !catalog_bytes.is_empty() {
        let catalog = DocumentCatalog::from_bytes(&catalog_bytes)?;
        session.set_catalog(catalog)?;
    }

    for entry in &record.chunks {
        let chunk_type = ChunkType::from_name(&entry.chunk_type)?;
        let payload = history.get_payload(&entry.hash)?;
        session.add_payload_chunk(chunk_type, chunk_type.default_flags(), payload)?;
    }

    session.encode_file()
}

/// Export a revision to a new `.tes` path.
///
/// By default writes body only. With `keep_history`, attaches the **current**
/// history footer from `path` (not a truncated copy).
///
/// # Errors
///
/// Returns history, materialize, or I/O errors.
pub fn export_revision(
    path: impl AsRef<Path>,
    rev_or_draft: &str,
    out_path: impl AsRef<Path>,
    keep_history: bool,
) -> Result<()> {
    let path = path.as_ref();
    let out_path = out_path.as_ref();
    let bytes = fs::read(path)?;
    let sb = SuperblockV0::from_bytes(&bytes)?;
    let (_body, history) = split_body_and_history(&bytes, sb.has_history_footer())?;
    let history = history.unwrap_or_else(HistoryV1::new);
    let body = materialize_revision(&history, rev_or_draft)?;
    let out = if keep_history {
        attach_footer(body, &history)?
    } else {
        body
    };
    if let Some(parent) = out_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(out_path, out)?;
    Ok(())
}

/// Replace the live sealed body with a materialized revision; re-attach the
/// full current `THST` footer (draft/head pointers unchanged).
///
/// # Errors
///
/// Returns history, materialize, or I/O errors.
pub fn checkout_revision(path: impl AsRef<Path>, rev_or_draft: &str) -> Result<()> {
    let path = path.as_ref();
    let bytes = fs::read(path)?;
    let sb = SuperblockV0::from_bytes(&bytes)?;
    let (_body, history) = split_body_and_history(&bytes, sb.has_history_footer())?;
    let history = history.unwrap_or_else(HistoryV1::new);
    let body = materialize_revision(&history, rev_or_draft)?;
    let out = attach_footer(body, &history)?;
    atomic_replace(path, &out)?;
    Ok(())
}

/// Tessprek projection for git `textconv` (stdout-only; no source-hash banner).
///
/// # Errors
///
/// Returns errors from [`crate::edit::edit_read`].
pub fn textconv(path: impl AsRef<Path>) -> Result<String> {
    let report = crate::edit::edit_read(path)?;
    Ok(report.tessprek)
}
