use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use crate::catalog::history::{ChunkManifest, HistoryV1, Revision};
use crate::error::Result;

use super::read_history;

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
