//! Read and format the THST revision log (`tes log`).

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use crate::catalog::history::{HistoryV1, split_body_and_history};
use crate::error::Result;
use crate::layout::SuperblockV0;

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
