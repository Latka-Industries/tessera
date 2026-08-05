use std::fs;
use std::path::Path;

use crate::error::Result;

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
