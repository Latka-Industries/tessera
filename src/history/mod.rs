//! History operations: save drafts, log, structural diff, changelog,
//! revision materialization, blame, and pending-ops redline (M10).
//!
//! Wire format lives in [`crate::catalog::history`]. This module snaps the live
//! sealed body into THST v1 revisions with an exact-hash payload store.

mod pending;

pub use pending::{
    PendingActionOptions, PendingActionReport, PendingSuggestion, SuggestOptions, SuggestReport,
    accept_pending, format_pending, list_pending, pending_redline, reject_pending, suggest_pending,
};

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use crate::catalog::document::DocumentCatalog;
use crate::catalog::file::TesFile;
use crate::catalog::history::{
    ChunkManifest, HistoryV1, Revision, attach_footer, content_hash, revision_id,
    split_body_and_history,
};
use crate::catalog::index::ChunkType;
use crate::catalog::session::TesWriterSession;
use crate::error::{Result, TesError};
use crate::layout::{DocKind, SuperblockV0};

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

/// Options for [`blame_file`].
#[derive(Debug, Clone, Default)]
pub struct BlameOptions {
    /// Blame only this chunk id (all chunks when `None`).
    pub chunk: Option<u64>,
    /// Revision id or draft name (defaults to history `head`).
    pub rev: Option<String>,
}

/// One attributed region in a blame report.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BlameRegion {
    /// Chunk id.
    pub chunk_id: u64,
    /// Chunk type name.
    pub chunk_type: String,
    /// 1-based line within the text body (`None` for non-text / whole-chunk rows).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// Revision that introduced this region.
    pub revision_id: String,
    /// Revision timestamp.
    pub at: String,
    /// Tool / actor (`Revision.source`).
    pub source: String,
    /// Optional save message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Line text or a short non-text label.
    pub text: String,
}

/// Blame report for a tip revision.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BlameReport {
    /// Path inspected.
    pub path: String,
    /// Tip revision id blamed.
    pub revision_id: String,
    /// Ordered regions (reading-order chunks, then lines).
    pub regions: Vec<BlameRegion>,
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

struct LiveSnapshot {
    catalog_hash: String,
    chunks: Vec<ChunkManifest>,
}

fn snapshot_live(path: &Path, history: &mut HistoryV1) -> Result<LiveSnapshot> {
    let file = TesFile::open(path)?;
    let catalog_bytes = match file.catalog() {
        Some(cat) => cat.to_bytes()?,
        None => Vec::new(),
    };
    let catalog_hash = content_hash(&catalog_bytes);
    history.put_payload(&catalog_hash, &catalog_bytes);

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

/// Attribute current chunk / line text to the revision that last introduced it.
///
/// Defaults to history `head`. Text chunks emit one row per line; other chunk
/// types emit a single whole-chunk row.
///
/// # Errors
///
/// Returns [`TesError::InvalidHistory`] when no revisions / head exist,
/// [`TesError::RevisionNotFound`] for a bad `--rev`, or payload decode errors.
pub fn blame_file(path: impl AsRef<Path>, options: &BlameOptions) -> Result<BlameReport> {
    let path = path.as_ref();
    let history = read_history(path)?;
    if history.revisions.is_empty() {
        return Err(TesError::InvalidHistory {
            message: "no revisions to blame (run `tes save` first)".into(),
        });
    }
    let tip = if let Some(name) = &options.rev {
        history.resolve(name)?
    } else {
        let head = history
            .head
            .as_deref()
            .ok_or_else(|| TesError::InvalidHistory {
                message: "history has revisions but no head".into(),
            })?;
        history
            .revision(head)
            .ok_or_else(|| TesError::RevisionNotFound {
                id: head.to_owned(),
            })?
    };
    let chain = ancestry(&history, tip)?;
    let mut regions = Vec::new();
    for manifest in &tip.chunks {
        if options.chunk.is_some_and(|id| id != manifest.id) {
            continue;
        }
        let introducer = introducing_revision(&chain, manifest.id, &manifest.hash).unwrap_or(tip);
        if manifest.chunk_type == "text" {
            regions.extend(blame_text_lines(&history, &chain, tip, manifest)?);
        } else {
            regions.push(BlameRegion {
                chunk_id: manifest.id,
                chunk_type: manifest.chunk_type.clone(),
                line: None,
                revision_id: introducer.id.clone(),
                at: introducer.at.clone(),
                source: introducer.source.clone(),
                message: introducer.message.clone(),
                text: format!("[{}]", manifest.chunk_type),
            });
        }
    }
    Ok(BlameReport {
        path: path.display().to_string(),
        revision_id: tip.id.clone(),
        regions,
    })
}

/// Format a [`BlameReport`] for CLI output.
#[must_use]
pub fn format_blame(report: &BlameReport) -> String {
    let mut out = format!("# blame tip={}\n", report.revision_id);
    for region in &report.regions {
        let loc = match region.line {
            Some(line) => format!("{}:{line}", region.chunk_id),
            None => format!("{}", region.chunk_id),
        };
        let msg = region.message.as_deref().unwrap_or("");
        let _ = writeln!(
            out,
            "{loc}\t{}\t{}\t{}\t{msg}\t{}",
            region.revision_id, region.at, region.source, region.text
        );
    }
    out
}

/// Format blame as JSON.
///
/// # Errors
///
/// Returns JSON serialization errors.
pub fn format_blame_json(report: &BlameReport) -> Result<String> {
    Ok(serde_json::to_string_pretty(report)?)
}

fn ancestry<'a>(history: &'a HistoryV1, tip: &'a Revision) -> Result<Vec<&'a Revision>> {
    let mut chain = Vec::new();
    let mut current = tip;
    loop {
        chain.push(current);
        let Some(parent_id) = current.parent.as_deref() else {
            break;
        };
        let parent = history
            .revision(parent_id)
            .ok_or_else(|| TesError::RevisionNotFound {
                id: parent_id.to_owned(),
            })?;
        current = parent;
    }
    chain.reverse(); // oldest → tip
    Ok(chain)
}

fn chunk_hash(rev: &Revision, chunk_id: u64) -> Option<&str> {
    rev.chunks
        .iter()
        .find(|c| c.id == chunk_id)
        .map(|c| c.hash.as_str())
}

fn introducing_revision<'a>(
    chain: &[&'a Revision],
    chunk_id: u64,
    tip_hash: &str,
) -> Option<&'a Revision> {
    // Oldest revision whose hash for this chunk equals tip_hash.
    chain
        .iter()
        .copied()
        .find(|rev| chunk_hash(rev, chunk_id) == Some(tip_hash))
}

fn blame_text_lines(
    history: &HistoryV1,
    chain: &[&Revision],
    tip: &Revision,
    manifest: &ChunkManifest,
) -> Result<Vec<BlameRegion>> {
    use crate::catalog::chunk::decode_text_payload;

    let tip_bytes = history.get_payload(&manifest.hash)?;
    let tip_body = decode_text_payload(&tip_bytes).map_or_else(
        |_| String::from_utf8_lossy(&tip_bytes).into_owned(),
        |(_, body)| body,
    );
    let tip_lines: Vec<String> = tip_body.lines().map(str::to_owned).collect();
    if tip_lines.is_empty() {
        let introducer = introducing_revision(chain, manifest.id, &manifest.hash).unwrap_or(tip);
        return Ok(vec![BlameRegion {
            chunk_id: manifest.id,
            chunk_type: manifest.chunk_type.clone(),
            line: Some(1),
            revision_id: introducer.id.clone(),
            at: introducer.at.clone(),
            source: introducer.source.clone(),
            message: introducer.message.clone(),
            text: String::new(),
        }]);
    }

    // Collect bodies for revisions that contain this chunk (oldest → tip).
    let mut versions: Vec<(&Revision, Vec<String>)> = Vec::new();
    for rev in chain {
        let Some(hash) = chunk_hash(rev, manifest.id) else {
            continue;
        };
        let bytes = history.get_payload(hash)?;
        let body = decode_text_payload(&bytes).map_or_else(
            |_| String::from_utf8_lossy(&bytes).into_owned(),
            |(_, body)| body,
        );
        versions.push((rev, body.lines().map(str::to_owned).collect()));
    }
    if versions.is_empty() {
        return Ok(Vec::new());
    }

    let n = tip_lines.len();
    let mut owners: Vec<Option<&Revision>> = vec![None; n];
    // Map tip line index → index in the current (child) body while walking back.
    let mut map: Vec<Option<usize>> = (0..n).map(Some).collect();

    for window in versions.windows(2).rev() {
        let (_parent_rev, parent_lines) = &window[0];
        let (child_rev, child_lines) = &window[1];
        let matching = lcs_child_to_parent(parent_lines, child_lines);
        for (tip_i, slot) in map.iter_mut().enumerate() {
            let Some(child_i) = *slot else {
                continue;
            };
            if let Some(parent_i) = matching.get(child_i).copied().flatten() {
                *slot = Some(parent_i);
            } else {
                owners[tip_i] = Some(*child_rev);
                *slot = None;
            }
        }
    }
    let oldest = versions[0].0;
    for (tip_i, slot) in map.iter().enumerate() {
        if slot.is_some() {
            owners[tip_i] = Some(oldest);
        }
    }

    let mut regions = Vec::with_capacity(n);
    for (i, line) in tip_lines.iter().enumerate() {
        let rev = owners[i].unwrap_or(tip);
        regions.push(BlameRegion {
            chunk_id: manifest.id,
            chunk_type: manifest.chunk_type.clone(),
            line: Some((i + 1) as u32),
            revision_id: rev.id.clone(),
            at: rev.at.clone(),
            source: rev.source.clone(),
            message: rev.message.clone(),
            text: line.clone(),
        });
    }
    Ok(regions)
}

/// For each child line index, the matched parent line index (LCS), or `None` if new.
fn lcs_child_to_parent(parent: &[String], child: &[String]) -> Vec<Option<usize>> {
    let n = parent.len();
    let m = child.len();
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in 0..n {
        for j in 0..m {
            if parent[i] == child[j] {
                dp[i + 1][j + 1] = dp[i][j] + 1;
            } else {
                dp[i + 1][j + 1] = dp[i + 1][j].max(dp[i][j + 1]);
            }
        }
    }
    let mut matching = vec![None; m];
    let mut i = n;
    let mut j = m;
    while i > 0 && j > 0 {
        if parent[i - 1] == child[j - 1] {
            matching[j - 1] = Some(i - 1);
            i -= 1;
            j -= 1;
        } else if dp[i][j - 1] >= dp[i - 1][j] {
            j -= 1;
        } else {
            i -= 1;
        }
    }
    matching
}

pub(crate) fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<()> {
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

pub(crate) fn chrono_like_now() -> String {
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

    #[test]
    fn export_checkout_textconv_round_trip() {
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

        let bytes_after_r1 = fs::read(&path).unwrap();
        let sb = SuperblockV0::from_bytes(&bytes_after_r1).unwrap();
        let (body_r1, _) =
            split_body_and_history(&bytes_after_r1, sb.has_history_footer()).unwrap();

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

        let exported = dir.path().join("old.tes");
        export_revision(&path, &r1.revision_id, &exported, false).unwrap();
        assert_eq!(fs::read(&exported).unwrap(), body_r1);
        assert!(verify_tes_file(&exported, true).unwrap().ok);

        let hist_before = read_history(&path).unwrap();
        let head_before = hist_before.head.clone();
        let drafts_before = hist_before.drafts.clone();
        checkout_revision(&path, &r1.revision_id).unwrap();
        assert!(verify_tes_file(&path, true).unwrap().ok);
        let hist_after = read_history(&path).unwrap();
        assert_eq!(hist_after.head, head_before);
        assert_eq!(hist_after.drafts, drafts_before);
        assert_eq!(hist_after.revisions.len(), hist_before.revisions.len());

        let bytes = fs::read(&path).unwrap();
        let sb = SuperblockV0::from_bytes(&bytes).unwrap();
        let (body_now, _) = split_body_and_history(&bytes, sb.has_history_footer()).unwrap();
        assert_eq!(body_now, body_r1);

        let tessprek = textconv(&path).unwrap();
        assert!(!tessprek.trim().is_empty());
        assert!(tessprek.contains("First body") || tessprek.contains("History note"));
    }

    #[test]
    fn blame_attributes_separate_paragraph_edits() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("two.tes");
        let mut s = TesWriterSession::create(&path, DocKind::Note);
        s.set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440099",
            "Blame note",
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Note,
        ))
        .unwrap();
        s.add_text_chunk(&TextHeader::paragraph(), "Alpha paragraph")
            .unwrap();
        s.add_text_chunk(&TextHeader::paragraph(), "Beta paragraph")
            .unwrap();
        s.commit().unwrap();

        let r1 = save_revision(
            &path,
            &SaveOptions {
                message: Some("initial".into()),
                ..SaveOptions::default()
            },
        )
        .unwrap();

        let hash = file_source_hash(&path).unwrap();
        apply_ops(
            &path,
            &[TesOp::SetText {
                chunk_id: 1,
                body: "Alpha edited".into(),
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
                message: Some("edit alpha".into()),
                ..SaveOptions::default()
            },
        )
        .unwrap();

        let hash = file_source_hash(&path).unwrap();
        apply_ops(
            &path,
            &[TesOp::SetText {
                chunk_id: 2,
                body: "Beta edited".into(),
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
        let r3 = save_revision(
            &path,
            &SaveOptions {
                message: Some("edit beta".into()),
                ..SaveOptions::default()
            },
        )
        .unwrap();
        assert_ne!(r1.revision_id, r2.revision_id);
        assert_ne!(r2.revision_id, r3.revision_id);

        let report = blame_file(&path, &BlameOptions::default()).unwrap();
        assert_eq!(report.revision_id, r3.revision_id);
        let alpha = report
            .regions
            .iter()
            .find(|r| r.chunk_id == 1)
            .expect("alpha");
        let beta = report
            .regions
            .iter()
            .find(|r| r.chunk_id == 2)
            .expect("beta");
        assert_eq!(alpha.revision_id, r2.revision_id);
        assert_eq!(alpha.text, "Alpha edited");
        assert_eq!(beta.revision_id, r3.revision_id);
        assert_eq!(beta.text, "Beta edited");

        let text = format_blame(&report);
        assert!(text.contains(&r2.revision_id));
        assert!(text.contains(&r3.revision_id));
        assert!(text.contains("Alpha edited"));
        assert!(text.contains("Beta edited"));
    }

    #[test]
    fn pending_suggest_redline_accept_reject() {
        use crate::history::{
            PendingActionOptions, SuggestOptions, accept_pending, format_pending, list_pending,
            pending_redline, reject_pending, suggest_pending,
        };
        use crate::verify::verify_tes_file;

        let dir = tempdir().unwrap();
        let path = sample(dir.path());
        let hash = file_source_hash(&path).unwrap();

        let report = suggest_pending(
            &path,
            r#"[{"op":"set_text","chunk_id":1,"body":"Pending body"}]"#,
            &SuggestOptions {
                source_hash: hash.clone(),
                message: Some("try this".into()),
                ..SuggestOptions::default()
            },
        )
        .unwrap();
        assert_eq!(report.ids.len(), 1);
        assert!(verify_tes_file(&path, true).unwrap().ok);

        // Body unchanged until accept.
        let raw = crate::catalog::TesFile::open(&path).unwrap();
        let entry = raw.chunk_by_id(1).unwrap();
        let decoded = raw.decode_payload(entry).unwrap();
        let (_, body) = crate::catalog::chunk::decode_text_payload(decoded.as_ref()).unwrap();
        assert_eq!(body, "First body");

        let pending = list_pending(&path).unwrap();
        assert_eq!(pending.len(), 1);
        assert!(
            format_pending(&pending).contains("Pending body")
                || format_pending(&pending).contains("set_text")
        );

        // Footer rewrite changes the on-disk source hash.
        let hash = file_source_hash(&path).unwrap();
        let redline = pending_redline(&path, &hash).unwrap();
        assert!(redline.contains("Pending body") || redline.contains('+') || redline.contains('-'));

        // Reject restores empty pending; body still original.
        let rejected = reject_pending(
            &path,
            &PendingActionOptions {
                source_hash: hash,
                ids: report.ids.clone(),
            },
        )
        .unwrap();
        assert_eq!(rejected.pending_count, 0);
        assert!(list_pending(&path).unwrap().is_empty());

        let hash = file_source_hash(&path).unwrap();
        let again = suggest_pending(
            &path,
            r#"[{"op":"set_text","chunk_id":1,"body":"Accepted body"}]"#,
            &SuggestOptions {
                source_hash: hash,
                message: Some("ship it".into()),
                ..SuggestOptions::default()
            },
        )
        .unwrap();
        let hash = file_source_hash(&path).unwrap();
        let accepted = accept_pending(
            &path,
            &PendingActionOptions {
                source_hash: hash,
                ids: again.ids.clone(),
            },
        )
        .unwrap();
        assert_eq!(accepted.pending_count, 0);
        assert!(verify_tes_file(&path, true).unwrap().ok);

        let raw = crate::catalog::TesFile::open(&path).unwrap();
        let entry = raw.chunk_by_id(1).unwrap();
        let decoded = raw.decode_payload(entry).unwrap();
        let (_, body) = crate::catalog::chunk::decode_text_payload(decoded.as_ref()).unwrap();
        assert_eq!(body, "Accepted body");
        assert!(list_pending(&path).unwrap().is_empty());
    }
}
