//! History operations: save drafts, log, structural diff, changelog,
//! revision materialization, blame, pending-ops redline, and verified merge (M10).
//!
//! Wire format lives in [`crate::catalog::history`]. This module snaps the live
//! sealed body into THST v1 revisions with an exact-hash payload store.

mod blame;
mod diff;
mod log;
mod materialize;
mod merge;
mod pending;
mod save;
mod util;

#[cfg(test)]
mod tests;

pub use blame::{
    BlameOptions, BlameRegion, BlameReport, blame_file, format_blame, format_blame_json,
};
pub use diff::{DiffEntry, DiffReport, diff_revisions, format_changelog, format_diff};
pub use log::{format_log, read_history};
pub use materialize::{checkout_revision, export_revision, materialize_revision, textconv};
pub use merge::{MergeReport, merge_files};
pub use pending::{
    PendingActionOptions, PendingActionReport, PendingSuggestion, SuggestOptions, SuggestReport,
    accept_pending, format_pending, list_pending, pending_redline, reject_pending, suggest_pending,
};
pub use save::{SaveOptions, SaveReport, save_revision};
pub(crate) use util::{atomic_replace, chrono_like_now};
