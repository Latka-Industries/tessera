//! Shared filesystem helpers for history save / merge / pending.

use std::fs;
use std::path::Path;

use crate::error::Result;

/// Write `bytes` via a sibling temp file, then rename over `path`.
///
/// # Errors
///
/// Returns [`TesError::Io`](crate::error::TesError::Io) on write or rename failure.
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

/// Timestamp string for revision metadata (`unix:{secs}`).
///
/// Avoids a `chrono` dependency; stable enough for tests and log display.
#[must_use]
pub(crate) fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!("unix:{secs}")
}
