use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{Result, TesError};

pub(super) fn sibling_temp_path(path: &Path, tag: &str) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("document.tes");
    let pid = std::process::id();
    parent.join(format!(".{stem}.{tag}.{pid}"))
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
pub(super) struct AdvisoryLock {
    path: PathBuf,
}

impl AdvisoryLock {
    pub(super) fn acquire(target: &Path) -> Result<Self> {
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
