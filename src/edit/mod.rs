//! Virtual editor mutation protocol (`edit-read` / `edit-write` / `apply`).
//!
//! Safe mutation gate from `docs/security.md`:
//! advisory lock → source-hash recheck → sibling temp compile → deep verify →
//! atomic replace.

mod ops;
mod tessprek;

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::catalog::chunk::{CitePayload, TextHeader};
use crate::catalog::document::DocumentCatalog;
use crate::catalog::history::attach_footer;
use crate::catalog::index::ChunkType;
use crate::catalog::media::{FigureRef, ImagePayload};
use crate::catalog::slide::SlidePayload;
use crate::catalog::{TesFile, TesWriterSession};
use crate::error::{Result, TesError};
use crate::verify::{TesVerifyReport, verify_bytes, verify_tes_file};

pub use ops::{TesOp, apply_ops_to_blocks, parse_ops_json};
pub use tessprek::{decode_tessprek, encode_tessprek};

/// One reading-order block in a Tessprek projection.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // CitePayload carries optional BibEntry.
pub enum ContentBlock {
    /// Text chunk.
    Text {
        /// Optional stable id from the source projection.
        chunk_id: Option<u64>,
        /// Semantic header.
        header: TextHeader,
        /// UTF-8 body.
        body: String,
    },
    /// Figure chunk referencing an image payload.
    Figure {
        /// Optional stable id from the source projection.
        chunk_id: Option<u64>,
        /// Figure metadata + alt.
        figure: FigureRef,
    },
    /// Cite chunk.
    Cite {
        /// Optional stable id from the source projection.
        chunk_id: Option<u64>,
        /// Cite payload.
        cite: CitePayload,
    },
    /// Slide chunk with named region refs.
    Slide {
        /// Optional stable id from the source projection.
        chunk_id: Option<u64>,
        /// Slide payload.
        slide: SlidePayload,
    },
}

impl ContentBlock {
    /// Projected chunk id, when known.
    #[must_use]
    pub fn chunk_id(&self) -> Option<u64> {
        match self {
            Self::Text { chunk_id, .. }
            | Self::Figure { chunk_id, .. }
            | Self::Cite { chunk_id, .. }
            | Self::Slide { chunk_id, .. } => *chunk_id,
        }
    }
}

/// Result of `edit-read`.
#[derive(Debug, Clone)]
pub struct EditReadReport {
    /// Hex SHA-256 of the on-disk `.tes` bytes.
    pub source_hash: String,
    /// Tessera Markdown buffer.
    pub tessprek: String,
}

/// Options for mutation writes.
#[derive(Debug, Clone)]
pub struct EditWriteOptions {
    /// Expected source hash (required).
    pub source_hash: String,
    /// When true, compile + verify but do not replace the original.
    pub dry_run: bool,
}

/// Result of a successful (or dry-run) write.
#[derive(Debug, Clone)]
pub struct EditWriteReport {
    /// Path that was mutated (or would be).
    pub path: PathBuf,
    /// Prior source hash that was checked.
    pub source_hash: String,
    /// New source hash after replace (`None` on dry-run).
    pub new_source_hash: Option<String>,
    /// Deep-verify report for the compiled temp file.
    pub verify: TesVerifyReport,
    /// Unified-ish Tessprek diff (before → after) for dry-run / agents.
    pub diff: String,
    /// Whether the original was replaced.
    pub replaced: bool,
}

/// Hex-encoded SHA-256 of file bytes.
///
/// # Errors
///
/// Returns [`TesError::Io`] if the file cannot be read.
pub fn file_source_hash(path: impl AsRef<Path>) -> Result<String> {
    let bytes = fs::read(path.as_ref())?;
    Ok(hash_bytes(&bytes))
}

/// Hex-encoded SHA-256 of an in-memory buffer.
#[must_use]
pub fn hash_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex_encode(&digest)
}

/// Decode a `.tes` file to Tessera Markdown for an editor buffer.
///
/// # Errors
///
/// Returns I/O or decode errors from opening/encoding the file.
pub fn edit_read(path: impl AsRef<Path>) -> Result<EditReadReport> {
    let path = path.as_ref();
    let source_hash = file_source_hash(path)?;
    let file = TesFile::open(path)?;
    let tessprek = encode_tessprek(&file, &source_hash)?;
    Ok(EditReadReport {
        source_hash,
        tessprek,
    })
}

/// Compile Tessera Markdown and atomically replace `path` under lock + hash check.
///
/// # Errors
///
/// Returns lock, hash mismatch, parse, verify, or I/O errors. On failure the
/// original file is left untouched.
pub fn edit_write(
    path: impl AsRef<Path>,
    tessprek: &str,
    options: &EditWriteOptions,
) -> Result<EditWriteReport> {
    let path = path.as_ref();
    let _lock = AdvisoryLock::acquire(path)?;
    let current_hash = file_source_hash(path)?;
    if current_hash != options.source_hash {
        return Err(TesError::SourceHashMismatch {
            expected: options.source_hash.clone(),
            found: current_hash,
        });
    }

    let before = edit_read(path)?;
    let source = TesFile::open(path)?;
    let blocks = decode_tessprek(tessprek)?;
    let compiled = seal_with_history(&source, compile_blocks_to_bytes(&source, &blocks, None)?)?;

    let verify = verify_bytes(path, &compiled, true);
    if !verify.ok {
        let first = verify
            .findings
            .iter()
            .find(|f| matches!(f.severity, crate::verify::Severity::Error))
            .map(|f| f.message.clone())
            .unwrap_or_else(|| "deep verify failed".into());
        return Err(TesError::EditVerifyFailed { message: first });
    }

    let after_tessprek = {
        // Encode from compiled bytes via a temp file for the diff projection.
        let tmp_for_diff = sibling_temp_path(path, "diff")?;
        fs::write(&tmp_for_diff, &compiled)?;
        let report = edit_read(&tmp_for_diff);
        let _ = fs::remove_file(&tmp_for_diff);
        report?.tessprek
    };
    let diff = simple_diff(
        &normalize_tessprek_for_diff(&before.tessprek),
        &normalize_tessprek_for_diff(&after_tessprek),
    );

    if options.dry_run {
        return Ok(EditWriteReport {
            path: path.to_path_buf(),
            source_hash: current_hash,
            new_source_hash: None,
            verify,
            diff,
            replaced: false,
        });
    }

    let tmp = sibling_temp_path(path, "tmp")?;
    {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(&compiled)?;
        file.sync_all()?;
    }
    // Re-check after compile (another writer could have raced before we locked
    // in pathological cases; lock covers the common editor path).
    let recheck = file_source_hash(path)?;
    if recheck != options.source_hash {
        let _ = fs::remove_file(&tmp);
        return Err(TesError::SourceHashMismatch {
            expected: options.source_hash.clone(),
            found: recheck,
        });
    }
    fs::rename(&tmp, path)?;
    let new_source_hash = file_source_hash(path)?;
    // Confirm the replaced file still verifies on disk.
    let on_disk = verify_tes_file(path, true)?;
    if !on_disk.ok {
        return Err(TesError::EditVerifyFailed {
            message: "post-replace deep verify failed".into(),
        });
    }

    Ok(EditWriteReport {
        path: path.to_path_buf(),
        source_hash: current_hash,
        new_source_hash: Some(new_source_hash),
        verify: on_disk,
        diff,
        replaced: true,
    })
}

/// Apply a Tessprek patch file through the same mutation gate as [`edit_write`].
///
/// # Errors
///
/// Same as [`edit_write`].
pub fn apply_patch(
    path: impl AsRef<Path>,
    patch_tessprek: &str,
    options: &EditWriteOptions,
) -> Result<EditWriteReport> {
    edit_write(path, patch_tessprek, options)
}

/// Apply typed JSON ops: project → mutate → compile → verify → replace.
///
/// # Errors
///
/// Returns op/parse/verify/I/O errors. Original untouched on failure.
pub fn apply_ops(
    path: impl AsRef<Path>,
    ops: &[TesOp],
    options: &EditWriteOptions,
) -> Result<EditWriteReport> {
    let path = path.as_ref();
    let _lock = AdvisoryLock::acquire(path)?;
    let current_hash = file_source_hash(path)?;
    if current_hash != options.source_hash {
        return Err(TesError::SourceHashMismatch {
            expected: options.source_hash.clone(),
            found: current_hash,
        });
    }

    let before = edit_read(path)?;
    let source = TesFile::open(path)?;
    let mut blocks = decode_tessprek(&before.tessprek)?;
    let mut title = source
        .catalog()
        .map(|c| c.title.clone())
        .unwrap_or_else(|| "Untitled".into());
    apply_ops_to_blocks(&mut blocks, &mut title, ops)?;
    let compiled = seal_with_history(
        &source,
        compile_blocks_to_bytes(&source, &blocks, Some(title.as_str()))?,
    )?;

    let verify = verify_bytes(path, &compiled, true);
    if !verify.ok {
        let first = verify
            .findings
            .iter()
            .find(|f| matches!(f.severity, crate::verify::Severity::Error))
            .map(|f| f.message.clone())
            .unwrap_or_else(|| "deep verify failed".into());
        return Err(TesError::EditVerifyFailed { message: first });
    }

    let after_tessprek = {
        let tmp_for_diff = sibling_temp_path(path, "diff")?;
        fs::write(&tmp_for_diff, &compiled)?;
        let report = edit_read(&tmp_for_diff);
        let _ = fs::remove_file(&tmp_for_diff);
        report?.tessprek
    };
    let diff = simple_diff(
        &normalize_tessprek_for_diff(&before.tessprek),
        &normalize_tessprek_for_diff(&after_tessprek),
    );

    if options.dry_run {
        return Ok(EditWriteReport {
            path: path.to_path_buf(),
            source_hash: current_hash,
            new_source_hash: None,
            verify,
            diff,
            replaced: false,
        });
    }

    let tmp = sibling_temp_path(path, "tmp")?;
    {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(&compiled)?;
        file.sync_all()?;
    }
    let recheck = file_source_hash(path)?;
    if recheck != options.source_hash {
        let _ = fs::remove_file(&tmp);
        return Err(TesError::SourceHashMismatch {
            expected: options.source_hash.clone(),
            found: recheck,
        });
    }
    fs::rename(&tmp, path)?;
    let new_source_hash = file_source_hash(path)?;
    let on_disk = verify_tes_file(path, true)?;
    if !on_disk.ok {
        return Err(TesError::EditVerifyFailed {
            message: "post-replace deep verify failed".into(),
        });
    }

    Ok(EditWriteReport {
        path: path.to_path_buf(),
        source_hash: current_hash,
        new_source_hash: Some(new_source_hash),
        verify: on_disk,
        diff,
        replaced: true,
    })
}

/// Re-attach an existing THST footer so edits do not drop revision history.
fn seal_with_history(source: &TesFile, body: Vec<u8>) -> Result<Vec<u8>> {
    match source.history()? {
        Some(history) => attach_footer(body, &history),
        None => Ok(body),
    }
}

fn compile_blocks_to_bytes(
    source: &TesFile,
    blocks: &[ContentBlock],
    title_override: Option<&str>,
) -> Result<Vec<u8>> {
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
    if let Some(title) = title_override {
        catalog.title = title.to_owned();
    }
    catalog.modified = now;

    // Collect image payloads referenced by figures, preserving source bytes.
    let mut needed_images: Vec<u64> = blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Figure { figure, .. } => Some(figure.image_chunk_id),
            _ => None,
        })
        .collect();
    needed_images.sort_unstable();
    needed_images.dedup();

    let mut image_payloads: Vec<(u64, ImagePayload)> = Vec::new();
    for old_id in needed_images {
        let entry = source.chunk_by_id(old_id)?;
        if entry.chunk_type != ChunkType::Image {
            return Err(TesError::EditOp {
                message: format!("figure references chunk {old_id} which is not an image"),
            });
        }
        let raw = source.decode_payload(entry)?;
        let payload = ImagePayload::from_bytes(raw.as_ref())?;
        image_payloads.push((old_id, payload));
    }

    // Build into an ephemeral session path (encode_file only; no commit).
    let phantom = PathBuf::from("__tessera_edit_encode__.tes");
    let mut session = TesWriterSession::create(&phantom, doc_kind);
    session.set_catalog(catalog)?;

    let mut image_id_map = std::collections::HashMap::new();
    for (old_id, payload) in &image_payloads {
        let new_id = session.add_image_chunk(payload)?;
        image_id_map.insert(*old_id, new_id);
    }

    for block in blocks {
        match block {
            ContentBlock::Text { header, body, .. } => {
                session.add_text_chunk(header, body)?;
            }
            ContentBlock::Figure { figure, .. } => {
                let mut figure = figure.clone();
                let Some(&new_id) = image_id_map.get(&figure.image_chunk_id) else {
                    return Err(TesError::EditOp {
                        message: format!(
                            "missing image payload for chunk {}",
                            figure.image_chunk_id
                        ),
                    });
                };
                figure.image_chunk_id = new_id;
                session.add_figure(&figure)?;
            }
            ContentBlock::Cite { cite, .. } => {
                // Prefer full cite payload from source when id matches (keeps `source` bib).
                if let Some(id) = block.chunk_id()
                    && let Ok(entry) = source.chunk_by_id(id)
                    && entry.chunk_type == ChunkType::Cite
                {
                    let raw = source.decode_payload(entry)?;
                    let mut full = CitePayload::from_bytes(raw.as_ref())?;
                    full.quote = cite.quote.clone();
                    if cite.label.is_some() {
                        full.label = cite.label.clone();
                    }
                    if cite.target_doc_id.is_some() {
                        full.target_doc_id = cite.target_doc_id.clone();
                    }
                    if cite.target_chunk_id.is_some() {
                        full.target_chunk_id = cite.target_chunk_id;
                    }
                    if cite.page.is_some() {
                        full.page = cite.page;
                    }
                    session.add_cite_chunk(&full)?;
                    continue;
                }
                session.add_cite_chunk(cite)?;
            }
            ContentBlock::Slide { slide, .. } => {
                session.add_slide(slide)?;
            }
        }
    }

    session.encode_file()
}

fn sibling_temp_path(path: &Path, tag: &str) -> Result<PathBuf> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("document.tes");
    let pid = std::process::id();
    Ok(parent.join(format!(".{stem}.{tag}.{pid}")))
}

fn advisory_lock_path(target: &Path) -> PathBuf {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("document.tes");
    parent.join(format!(".{name}.lock"))
}

/// Advisory per-file lock via exclusive lock-file create.
struct AdvisoryLock {
    path: PathBuf,
}

impl AdvisoryLock {
    fn acquire(target: &Path) -> Result<Self> {
        let path = advisory_lock_path(target);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let _ = writeln!(file, "{}", std::process::id());
                let _ = file.sync_all();
                Ok(Self { path })
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(TesError::EditLocked {
                    path: path.display().to_string(),
                })
            }
            Err(err) => Err(err.into()),
        }
    }
}

impl Drop for AdvisoryLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

fn normalize_tessprek_for_diff(text: &str) -> String {
    text.lines()
        .map(|line| {
            if line.starts_with("<!-- tessera:") && line.contains("source-hash=") {
                // Ignore hash churn from re-encoding into a temp file.
                "<!-- tessera: format=tessprek version=1 source-hash=<hash> -->"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn simple_diff(before: &str, after: &str) -> String {
    if before == after {
        return String::from("(no changes)\n");
    }
    let mut out = String::from("--- before\n+++ after\n");
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    let max = before_lines.len().max(after_lines.len());
    for i in 0..max {
        let a = before_lines.get(i).copied();
        let b = after_lines.get(i).copied();
        match (a, b) {
            (Some(a), Some(b)) if a == b => {}
            (Some(a), Some(b)) => {
                let _ = writeln!(out, "-{a}");
                let _ = writeln!(out, "+{b}");
            }
            (Some(a), None) => {
                let _ = writeln!(out, "-{a}");
            }
            (None, Some(b)) => {
                let _ = writeln!(out, "+{b}");
            }
            (None, None) => {}
        }
    }
    out
}

use std::fmt::Write as _;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::chunk::TextHeader;
    use crate::layout::DocKind;
    use tempfile::tempdir;

    fn sample_note(dir: &Path) -> PathBuf {
        let path = dir.join("note.tes");
        let mut session = TesWriterSession::create(&path, DocKind::Note);
        session
            .set_catalog(DocumentCatalog::new(
                "550e8400-e29b-41d4-a716-446655440000",
                "Meeting notes",
                "2026-07-27T00:00:00Z",
                "2026-07-27T00:00:00Z",
                DocKind::Note,
            ))
            .unwrap();
        session
            .add_text_chunk(&TextHeader::heading(1), "Agenda")
            .unwrap();
        session
            .add_text_chunk(&TextHeader::paragraph(), "Ship Tessprek")
            .unwrap();
        session.commit().unwrap();
        path
    }

    #[test]
    fn edit_read_write_round_trip() {
        let dir = tempdir().unwrap();
        let path = sample_note(dir.path());
        let read = edit_read(&path).unwrap();
        assert!(read.tessprek.contains("Agenda"));
        assert_eq!(read.source_hash.len(), 64);

        let edited = read.tessprek.replace("Ship Tessprek", "Ship edit protocol");
        // Keep directives intact; only body changed.
        let report = edit_write(
            &path,
            &edited,
            &EditWriteOptions {
                source_hash: read.source_hash.clone(),
                dry_run: false,
            },
        )
        .unwrap();
        assert!(report.replaced);
        let again = edit_read(&path).unwrap();
        assert!(again.tessprek.contains("Ship edit protocol"));
        assert_ne!(again.source_hash, read.source_hash);
    }

    #[test]
    fn source_hash_mismatch_rejects() {
        let dir = tempdir().unwrap();
        let path = sample_note(dir.path());
        let read = edit_read(&path).unwrap();
        let err = edit_write(
            &path,
            &read.tessprek,
            &EditWriteOptions {
                source_hash: "deadbeef".into(),
                dry_run: false,
            },
        )
        .unwrap_err();
        assert!(matches!(err, TesError::SourceHashMismatch { .. }));
    }

    #[test]
    fn apply_ops_set_text_dry_run() {
        let dir = tempdir().unwrap();
        let path = sample_note(dir.path());
        let read = edit_read(&path).unwrap();
        let ops = parse_ops_json(
            r#"[{"op":"set_text","chunk_id":2,"body":"Updated body"},{"op":"set_title","title":"Renamed"}]"#,
        )
        .unwrap();
        let report = apply_ops(
            &path,
            &ops,
            &EditWriteOptions {
                source_hash: read.source_hash,
                dry_run: true,
            },
        )
        .unwrap();
        assert!(!report.replaced);
        assert!(report.diff.contains("Updated body") || report.diff.contains('+'));
        // Original unchanged.
        let file = TesFile::open(&path).unwrap();
        assert_eq!(file.catalog().unwrap().title, "Meeting notes");
    }
}
