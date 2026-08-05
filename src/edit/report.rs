//! Edit-read / edit-write option and report types.

use std::path::PathBuf;

use crate::verify::TesVerifyReport;

use super::EditMediaBag;

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
    /// New media payloads referenced by temporary chunk ids in Tessprek.
    pub media: EditMediaBag,
}

impl EditWriteOptions {
    /// Build options with an empty media bag.
    #[must_use]
    pub fn new(source_hash: impl Into<String>, dry_run: bool) -> Self {
        Self {
            source_hash: source_hash.into(),
            dry_run,
            media: EditMediaBag::default(),
        }
    }
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
