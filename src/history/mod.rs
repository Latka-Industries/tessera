//! History operations: save drafts, log, structural diff, changelog (M10).
//!
//! Wire format lives in [`crate::catalog::history`]. This module snaps the live
//! sealed body into THST v1 revisions with an exact-hash payload store.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use crate::catalog::file::TesFile;
use crate::catalog::history::{
    ChunkManifest, HistoryV1, Revision, attach_footer, content_hash, revision_id,
    split_body_and_history,
};
use crate::error::Result;
use crate::layout::SuperblockV0;

/// Options for [`save_revision`].
#[derive(Debug, Clone, Default)]
pub struct SaveOptions {
    /// Named draft to update (also recorded on the revision).
    pub draft: Option<String>,
    /// Human message.
    pub message: Option<String>,
    /// Tool / actor label (default `tes save`).
    pub source: Option<String>,
}

/// Result of appending a revision.
#[derive(Debug, Clone)]
pub struct SaveReport {
    /// Path written.
    pub path: PathBuf,
    /// New revision id.
    pub revision_id: String,
    /// Draft name, if any.
    pub draft: Option<String>,
    /// Total revisions now stored.
    pub revision_count: usize,
}

/// One structural change between two revisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffEntry {
    /// Chunk present only in the right revision.
    Added {
        /// Chunk id.
        chunk_id: u64,
        /// Chunk type name.
        chunk_type: String,
        /// Payload hash.
        hash: String,
    },
    /// Chunk present only in the left revision.
    Removed {
        /// Chunk id.
        chunk_id: u64,
        /// Chunk type name.
        chunk_type: String,
        /// Payload hash.
        hash: String,
    },
    /// Same id, different payload hash.
    Changed {
        /// Chunk id.
        chunk_id: u64,
        /// Chunk type name (prefer right).
        chunk_type: String,
        /// Left hash.
        from_hash: String,
        /// Right hash.
        to_hash: String,
    },
}

/// Structural diff report.
#[derive(Debug, Clone)]
pub struct DiffReport {
    /// Left revision id.
    pub left: String,
    /// Right revision id.
    pub right: String,
    /// Ordered entries.
    pub entries: Vec<DiffEntry>,
    /// Optional Tessprek-ish text diff for changed text bodies.
    pub text_diff: String,
}

/// Append a content-addressed revision of the current sealed body to `path`.
///
/// # Errors
///
/// Returns I/O, decode, or history validation errors.
pub fn save_revision(path: impl AsRef<Path>, options: &SaveOptions) -> Result<SaveReport> {
    let path = path.as_ref();
    let bytes = fs::read(path)?;
    let sb = SuperblockV0::from_bytes(&bytes)?;
    let (body, existing) = split_body_and_history(&bytes, sb.has_history_footer())?;
    let mut history = existing.unwrap_or_else(HistoryV1::new);

    // Snapshot from a temporary open of the body (no footer).
    let tmp = path.with_extension("tes.save-body");
    fs::write(&tmp, &body)?;
    let snapshot = match snapshot_live(&tmp, &mut history) {
        Ok(s) => {
            let _ = fs::remove_file(&tmp);
            s
        }
        Err(err) => {
            let _ = fs::remove_file(&tmp);
            return Err(err);
        }
    };

    let parent = history.head.clone();
    // Content-identical to tip → refresh draft pointer only (no new revision).
    if let Some(head_id) = history.head.as_deref()
        && let Some(head) = history.revision(head_id)
        && head.catalog_hash == snapshot.catalog_hash
        && head.chunks == snapshot.chunks
    {
        let id = head_id.to_owned();
        if let Some(draft) = &options.draft {
            history.drafts.insert(draft.clone(), id.clone());
        }
        let out = attach_footer(body, &history)?;
        atomic_replace(path, &out)?;
        return Ok(SaveReport {
            path: path.to_path_buf(),
            revision_id: id,
            draft: options.draft.clone(),
            revision_count: history.revisions.len(),
        });
    }

    let id = revision_id(parent.as_deref(), &snapshot.catalog_hash, &snapshot.chunks);
    let at = chrono_like_now();
    let rev = Revision {
        id: id.clone(),
        parent,
        at,
        source: options.source.clone().unwrap_or_else(|| "tes save".into()),
        op: "save".into(),
        message: options.message.clone(),
        draft: options.draft.clone(),
        catalog_hash: snapshot.catalog_hash,
        chunks: snapshot.chunks,
    };
    history.revisions.push(rev);
    history.head = Some(id.clone());
    if let Some(draft) = &options.draft {
        history.drafts.insert(draft.clone(), id.clone());
    }
    history.validate()?;

    let out = attach_footer(body, &history)?;
    atomic_replace(path, &out)?;
    Ok(SaveReport {
        path: path.to_path_buf(),
        revision_id: id,
        draft: options.draft.clone(),
        revision_count: history.revisions.len(),
    })
}

/// Read history from a `.tes` file (empty if no footer).
///
/// # Errors
///
/// Returns open/decode errors when the flag is set but the footer is bad.
pub fn read_history(path: impl AsRef<Path>) -> Result<HistoryV1> {
    let bytes = fs::read(path.as_ref())?;
    let sb = SuperblockV0::from_bytes(&bytes)?;
    let (_body, history) = split_body_and_history(&bytes, sb.has_history_footer())?;
    Ok(history.unwrap_or_else(HistoryV1::new))
}

/// Human-readable revision log (newest last).
///
/// # Errors
///
/// Returns errors from [`read_history`].
pub fn format_log(path: impl AsRef<Path>, json: bool) -> Result<String> {
    let history = read_history(path)?;
    if json {
        return Ok(serde_json::to_string_pretty(&history)?);
    }
    let mut out = String::new();
    if history.revisions.is_empty() {
        out.push_str("(no revisions)\n");
        return Ok(out);
    }
    for rev in &history.revisions {
        let draft = rev.draft.as_deref().unwrap_or("-");
        let msg = rev.message.as_deref().unwrap_or("");
        let head = if history.head.as_deref() == Some(rev.id.as_str()) {
            " *"
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "{}\t{}\t{}\tdraft={}\tchunks={}{head}\t{msg}",
            rev.id,
            rev.at,
            rev.source,
            draft,
            rev.chunks.len()
        );
    }
    if !history.drafts.is_empty() {
        out.push_str("\nDrafts:\n");
        for (name, rev_id) in &history.drafts {
            let _ = writeln!(out, "  {name} → {rev_id}");
        }
    }
    Ok(out)
}

/// Structural diff between two revisions / draft names.
///
/// # Errors
///
/// Returns [`crate::error::TesError::RevisionNotFound`] or history decode errors.
pub fn diff_revisions(path: impl AsRef<Path>, left: &str, right: &str) -> Result<DiffReport> {
    let history = read_history(path.as_ref())?;
    let left_rev = history.resolve(left)?;
    let right_rev = history.resolve(right)?;
    let entries = structural_diff(&left_rev.chunks, &right_rev.chunks);
    let text_diff = text_diff_for_changes(&history, left_rev, right_rev, &entries)?;
    Ok(DiffReport {
        left: left_rev.id.clone(),
        right: right_rev.id.clone(),
        entries,
        text_diff,
    })
}

/// Format a [`DiffReport`] for CLI output.
#[must_use]
pub fn format_diff(report: &DiffReport) -> String {
    let mut out = format!("--- {}\n+++ {}\n", report.left, report.right);
    if report.entries.is_empty() {
        out.push_str("(no structural changes)\n");
    } else {
        for entry in &report.entries {
            match entry {
                DiffEntry::Added {
                    chunk_id,
                    chunk_type,
                    hash,
                } => {
                    let _ = writeln!(out, "+ chunk {chunk_id} ({chunk_type}) {hash}");
                }
                DiffEntry::Removed {
                    chunk_id,
                    chunk_type,
                    hash,
                } => {
                    let _ = writeln!(out, "- chunk {chunk_id} ({chunk_type}) {hash}");
                }
                DiffEntry::Changed {
                    chunk_id,
                    chunk_type,
                    from_hash,
                    to_hash,
                } => {
                    let _ = writeln!(
                        out,
                        "~ chunk {chunk_id} ({chunk_type}) {from_hash} → {to_hash}"
                    );
                }
            }
        }
    }
    if !report.text_diff.is_empty() {
        out.push('\n');
        out.push_str(&report.text_diff);
    }
    out
}

/// Changelog summary between two revisions.
///
/// # Errors
///
/// Returns errors from [`diff_revisions`].
pub fn format_changelog(path: impl AsRef<Path>, left: &str, right: &str) -> Result<String> {
    let report = diff_revisions(path, left, right)?;
    let mut added = 0usize;
    let mut removed = 0usize;
    let mut changed = 0usize;
    for entry in &report.entries {
        match entry {
            DiffEntry::Added { .. } => added += 1,
            DiffEntry::Removed { .. } => removed += 1,
            DiffEntry::Changed { .. } => changed += 1,
        }
    }
    Ok(format!(
        "changelog {} → {}\n  added={added} removed={removed} changed={changed}\n{}",
        report.left,
        report.right,
        format_diff(&report)
    ))
}

struct LiveSnapshot {
    catalog_hash: String,
    chunks: Vec<ChunkManifest>,
}

fn snapshot_live(path: &Path, history: &mut HistoryV1) -> Result<LiveSnapshot> {
    let file = TesFile::open(path)?;
    let catalog_hash = match file.catalog() {
        Some(cat) => content_hash(&cat.to_bytes()?),
        None => content_hash(b""),
    };
    let mut chunks = Vec::new();
    for entry in file.chunks() {
        let payload = file.decode_payload(entry)?;
        let hash = content_hash(payload.as_ref());
        history.put_payload(&hash, payload.as_ref());
        chunks.push(ChunkManifest {
            id: entry.chunk_id,
            chunk_type: entry.chunk_type.as_str().to_owned(),
            hash,
        });
    }
    Ok(LiveSnapshot {
        catalog_hash,
        chunks,
    })
}

fn structural_diff(left: &[ChunkManifest], right: &[ChunkManifest]) -> Vec<DiffEntry> {
    let left_map: BTreeMap<u64, &ChunkManifest> = left.iter().map(|c| (c.id, c)).collect();
    let right_map: BTreeMap<u64, &ChunkManifest> = right.iter().map(|c| (c.id, c)).collect();
    let ids: BTreeSet<u64> = left_map.keys().chain(right_map.keys()).copied().collect();
    let mut entries = Vec::new();
    for id in ids {
        match (left_map.get(&id), right_map.get(&id)) {
            (None, Some(r)) => entries.push(DiffEntry::Added {
                chunk_id: id,
                chunk_type: r.chunk_type.clone(),
                hash: r.hash.clone(),
            }),
            (Some(l), None) => entries.push(DiffEntry::Removed {
                chunk_id: id,
                chunk_type: l.chunk_type.clone(),
                hash: l.hash.clone(),
            }),
            (Some(l), Some(r)) if l.hash != r.hash => entries.push(DiffEntry::Changed {
                chunk_id: id,
                chunk_type: r.chunk_type.clone(),
                from_hash: l.hash.clone(),
                to_hash: r.hash.clone(),
            }),
            _ => {}
        }
    }
    entries
}

fn text_diff_for_changes(
    history: &HistoryV1,
    left: &Revision,
    right: &Revision,
    entries: &[DiffEntry],
) -> Result<String> {
    use crate::catalog::chunk::decode_text_payload;

    let mut out = String::new();
    for entry in entries {
        let DiffEntry::Changed {
            chunk_id,
            chunk_type,
            from_hash,
            to_hash,
        } = entry
        else {
            continue;
        };
        if chunk_type != "text" {
            continue;
        }
        let left_bytes = history.get_payload(from_hash)?;
        let right_bytes = history.get_payload(to_hash)?;
        let left_body = decode_text_payload(&left_bytes).map_or_else(
            |_| String::from_utf8_lossy(&left_bytes).into_owned(),
            |(_, body)| body,
        );
        let right_body = decode_text_payload(&right_bytes).map_or_else(
            |_| String::from_utf8_lossy(&right_bytes).into_owned(),
            |(_, body)| body,
        );
        let _ = writeln!(
            out,
            "@@ chunk {chunk_id} ({} → {}) @@\n{}",
            left.id,
            right.id,
            line_diff(&left_body, &right_body)
        );
    }
    Ok(out)
}

fn line_diff(before: &str, after: &str) -> String {
    if before == after {
        return String::from("(no text changes)\n");
    }
    let mut out = String::new();
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

fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp = dir.join(format!(
        ".{}.history-{}.tmp",
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("doc.tes"),
        std::process::id()
    ));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // Keep deterministic enough for tests; not a full chrono dependency.
    format!("unix:{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{DocumentCatalog, TesWriterSession, TextHeader};
    use crate::edit::{TesOp, apply_ops, file_source_hash};
    use crate::layout::DocKind;
    use crate::verify::verify_tes_file;
    use tempfile::tempdir;

    fn sample(dir: &Path) -> PathBuf {
        let path = dir.join("note.tes");
        let mut s = TesWriterSession::create(&path, DocKind::Note);
        s.set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440000",
            "History note",
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Note,
        ))
        .unwrap();
        s.add_text_chunk(&TextHeader::paragraph(), "First body")
            .unwrap();
        s.commit().unwrap();
        path
    }

    #[test]
    fn save_log_diff_round_trip() {
        let dir = tempdir().unwrap();
        let path = sample(dir.path());

        let r1 = save_revision(
            &path,
            &SaveOptions {
                draft: Some("outline".into()),
                message: Some("initial".into()),
                ..SaveOptions::default()
            },
        )
        .unwrap();
        assert!(verify_tes_file(&path, true).unwrap().ok);
        let log = format_log(&path, false).unwrap();
        assert!(log.contains(&r1.revision_id));
        assert!(log.contains("outline"));

        let hash = file_source_hash(&path).unwrap();
        apply_ops(
            &path,
            &[TesOp::SetText {
                chunk_id: 1,
                body: "Second body".into(),
                role: None,
                level: None,
                class: None,
            }],
            &crate::edit::EditWriteOptions {
                source_hash: hash,
                dry_run: false,
            },
        )
        .unwrap();

        let r2 = save_revision(
            &path,
            &SaveOptions {
                draft: Some("outline".into()),
                message: Some("edit".into()),
                ..SaveOptions::default()
            },
        )
        .unwrap();
        assert_ne!(r1.revision_id, r2.revision_id);

        let diff = diff_revisions(&path, &r1.revision_id, &r2.revision_id).unwrap();
        assert!(!diff.entries.is_empty());
        let text = format_diff(&diff);
        assert!(text.contains('~') || text.contains("Second body") || text.contains('-'));
        let changelog = format_changelog(&path, "outline", &r1.revision_id).unwrap();
        assert!(changelog.contains("changelog"));

        // Identical content save must not mint another revision.
        let before = read_history(&path).unwrap().revisions.len();
        let again = save_revision(
            &path,
            &SaveOptions {
                draft: Some("outline".into()),
                message: Some("noop".into()),
                ..SaveOptions::default()
            },
        )
        .unwrap();
        assert_eq!(again.revision_id, r2.revision_id);
        assert_eq!(read_history(&path).unwrap().revisions.len(), before);
    }
}
