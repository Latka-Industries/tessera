//! Pending authored ops (THST `pending` slot) — suggest / redline / accept / reject.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::catalog::history::{HistoryV1, attach_footer, content_hash, split_body_and_history};
use crate::edit::{EditWriteOptions, TesOp, apply_ops, file_source_hash, parse_ops_json};
use crate::error::{Result, TesError};
use crate::layout::SuperblockV0;
use crate::verify::verify_bytes;

use super::{atomic_replace, chrono_like_now, read_history};

/// One pending suggestion stored in [`HistoryV1::pending`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingSuggestion {
    /// Stable id (`pend_` + short hex).
    pub id: String,
    /// Timestamp when suggested.
    pub at: String,
    /// Tool / actor.
    pub source: String,
    /// Optional human message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Typed mutation (same vocabulary as `tes apply --ops`).
    pub op: TesOp,
}

/// Options for [`suggest_pending`].
#[derive(Debug, Clone, Default)]
pub struct SuggestOptions {
    /// Expected live source hash (concurrency guard).
    pub source_hash: String,
    /// Optional message stored on each suggestion.
    pub message: Option<String>,
    /// Tool / actor label (default `tes pending suggest`).
    pub source: Option<String>,
}

/// Result of suggesting pending ops.
#[derive(Debug, Clone)]
pub struct SuggestReport {
    /// Path written.
    pub path: PathBuf,
    /// New pending ids (same order as input ops).
    pub ids: Vec<String>,
    /// Total pending suggestions now stored.
    pub pending_count: usize,
}

/// Options for accept / reject.
#[derive(Debug, Clone)]
pub struct PendingActionOptions {
    /// Expected live source hash.
    pub source_hash: String,
    /// Pending ids to accept or reject (empty = all).
    pub ids: Vec<String>,
}

/// Result of accept / reject.
#[derive(Debug, Clone)]
pub struct PendingActionReport {
    /// Path written.
    pub path: PathBuf,
    /// Ids that were acted on.
    pub ids: Vec<String>,
    /// Remaining pending count.
    pub pending_count: usize,
    /// New source hash after accept (`None` for reject).
    pub new_source_hash: Option<String>,
}

/// List pending suggestions.
///
/// # Errors
///
/// Returns history decode / pending parse errors.
pub fn list_pending(path: impl AsRef<Path>) -> Result<Vec<PendingSuggestion>> {
    let history = read_history(path)?;
    parse_pending(&history)
}

/// Human-readable pending list.
#[must_use]
pub fn format_pending(pending: &[PendingSuggestion]) -> String {
    if pending.is_empty() {
        return "(no pending ops)\n".into();
    }
    let mut out = String::new();
    for p in pending {
        let msg = p.message.as_deref().unwrap_or("");
        let op = serde_json::to_string(&p.op).unwrap_or_else(|_| "?".into());
        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!("{}\t{}\t{}\t{msg}\t{op}\n", p.id, p.at, p.source),
        );
    }
    out
}

/// Suggest ops into the THST `pending` slot without changing the sealed body.
///
/// # Errors
///
/// Returns hash mismatch, parse, or I/O errors.
pub fn suggest_pending(
    path: impl AsRef<Path>,
    ops_json: &str,
    options: &SuggestOptions,
) -> Result<SuggestReport> {
    let path = path.as_ref();
    require_source_hash(path, &options.source_hash)?;
    let ops = parse_ops_json(ops_json)?;
    if ops.is_empty() {
        return Err(TesError::EditOp {
            message: "suggest requires at least one op".into(),
        });
    }

    let (body, mut history) = load_body_and_history(path)?;
    let mut pending = parse_pending(&history)?;

    let at = chrono_like_now();
    let source = options
        .source
        .clone()
        .unwrap_or_else(|| "tes pending suggest".into());
    let mut ids = Vec::with_capacity(ops.len());
    for op in ops {
        let id = pending_id(&op, &at, pending.len());
        pending.push(PendingSuggestion {
            id: id.clone(),
            at: at.clone(),
            source: source.clone(),
            message: options.message.clone(),
            op,
        });
        ids.push(id);
    }

    commit_pending(path, body, &mut history, &pending, None)?;
    Ok(SuggestReport {
        path: path.to_path_buf(),
        ids,
        pending_count: pending.len(),
    })
}

/// Project sealed body + pending ops as a Tessprek redline (dry-run; no write).
///
/// # Errors
///
/// Returns hash mismatch, decode, or apply errors.
pub fn pending_redline(path: impl AsRef<Path>, source_hash: &str) -> Result<String> {
    let path = path.as_ref();
    let pending = list_pending(path)?;
    if pending.is_empty() {
        return Ok("(no pending ops)\n".into());
    }
    let ops: Vec<TesOp> = pending.iter().map(|p| p.op.clone()).collect();
    let report = apply_ops(
        path,
        &ops,
        &EditWriteOptions {
            source_hash: source_hash.to_owned(),
            dry_run: true,
        },
    )?;
    let mut out = String::from("# pending redline (dry-run)\n");
    out.push_str(&report.diff);
    Ok(out)
}

/// Accept pending ops: apply to sealed body (deep verify), then drop them from `pending`.
///
/// # Errors
///
/// Returns hash mismatch, missing id, apply, or verify errors.
pub fn accept_pending(
    path: impl AsRef<Path>,
    options: &PendingActionOptions,
) -> Result<PendingActionReport> {
    let path = path.as_ref();
    require_source_hash(path, &options.source_hash)?;

    let history = read_history(path)?;
    let mut pending = parse_pending(&history)?;
    let (ids, ops) = select_pending_ops(&pending, &options.ids, "accept")?;

    let apply_report = apply_ops(
        path,
        &ops,
        &EditWriteOptions {
            source_hash: options.source_hash.clone(),
            dry_run: false,
        },
    )?;

    // Body updated; strip accepted ids from pending (footer preserved by apply_ops).
    let (body, mut history) = load_body_and_history(path)?;
    pending = parse_pending(&history)?;
    pending.retain(|p| !ids.contains(&p.id));
    commit_pending(
        path,
        body,
        &mut history,
        &pending,
        Some("post-accept pending strip failed verify"),
    )?;

    Ok(PendingActionReport {
        path: path.to_path_buf(),
        ids,
        pending_count: pending.len(),
        new_source_hash: apply_report.new_source_hash,
    })
}

/// Reject pending ops (drop from footer; sealed body unchanged).
///
/// # Errors
///
/// Returns hash mismatch, missing id, or I/O errors.
pub fn reject_pending(
    path: impl AsRef<Path>,
    options: &PendingActionOptions,
) -> Result<PendingActionReport> {
    let path = path.as_ref();
    require_source_hash(path, &options.source_hash)?;

    let (body, mut history) = load_body_and_history(path)?;
    let mut pending = parse_pending(&history)?;
    let (ids, _) = select_pending_ops(&pending, &options.ids, "reject")?;
    pending.retain(|p| !ids.contains(&p.id));
    commit_pending(
        path,
        body,
        &mut history,
        &pending,
        Some("reject pending failed verify"),
    )?;

    Ok(PendingActionReport {
        path: path.to_path_buf(),
        ids,
        pending_count: pending.len(),
        new_source_hash: None,
    })
}

fn require_source_hash(path: &Path, expected: &str) -> Result<()> {
    let found = file_source_hash(path)?;
    if found == expected {
        Ok(())
    } else {
        Err(TesError::SourceHashMismatch {
            expected: expected.to_owned(),
            found,
        })
    }
}

fn load_body_and_history(path: &Path) -> Result<(Vec<u8>, HistoryV1)> {
    let bytes = fs::read(path)?;
    let sb = SuperblockV0::from_bytes(&bytes)?;
    let (body, existing) = split_body_and_history(&bytes, sb.has_history_footer())?;
    Ok((body, existing.unwrap_or_else(HistoryV1::new)))
}

fn commit_pending(
    path: &Path,
    body: Vec<u8>,
    history: &mut HistoryV1,
    pending: &[PendingSuggestion],
    verify_fail_message: Option<&str>,
) -> Result<()> {
    write_pending(history, pending)?;
    let out = attach_footer(body, history)?;
    let report = verify_bytes(path, &out, true);
    if !report.ok {
        let message = verify_fail_message.map_or_else(
            || {
                report
                    .findings
                    .iter()
                    .find(|f| matches!(f.severity, crate::verify::Severity::Error))
                    .map_or_else(|| "verify failed".into(), |f| f.message.clone())
            },
            str::to_owned,
        );
        return Err(TesError::EditVerifyFailed { message });
    }
    atomic_replace(path, &out)
}

fn parse_pending(history: &HistoryV1) -> Result<Vec<PendingSuggestion>> {
    let mut out = Vec::with_capacity(history.pending.len());
    for (i, value) in history.pending.iter().enumerate() {
        let suggestion: PendingSuggestion =
            serde_json::from_value(value.clone()).map_err(|err| TesError::InvalidHistory {
                message: format!("pending[{i}] is not a PendingSuggestion: {err}"),
            })?;
        out.push(suggestion);
    }
    Ok(out)
}

fn write_pending(history: &mut HistoryV1, pending: &[PendingSuggestion]) -> Result<()> {
    history.pending = pending
        .iter()
        .map(serde_json::to_value)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(())
}

fn pending_id(op: &TesOp, at: &str, salt: usize) -> String {
    let payload = format!(
        "{at}|{salt}|{}",
        serde_json::to_string(op).unwrap_or_default()
    );
    let hash = content_hash(payload.as_bytes());
    format!("pend_{}", &hash[..16])
}

fn select_pending_ops(
    pending: &[PendingSuggestion],
    ids: &[String],
    action: &str,
) -> Result<(Vec<String>, Vec<TesOp>)> {
    let selected = if ids.is_empty() {
        pending.iter().collect::<Vec<_>>()
    } else {
        let mut selected = Vec::with_capacity(ids.len());
        for id in ids {
            let found = pending
                .iter()
                .find(|p| p.id == *id)
                .ok_or_else(|| TesError::EditOp {
                    message: format!("pending id '{id}' not found"),
                })?;
            selected.push(found);
        }
        selected
    };
    if selected.is_empty() {
        return Err(TesError::EditOp {
            message: format!("no pending ops to {action}"),
        });
    }
    Ok((
        selected.iter().map(|p| p.id.clone()).collect(),
        selected.iter().map(|p| p.op.clone()).collect(),
    ))
}
