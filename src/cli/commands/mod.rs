//! Command runners for [`crate::cli::run`].
//!
//! Each submodule owns one CLI surface (`inspect`, `io`, `edit`, …).

mod edit;
mod history;
mod inspect;
mod io;
mod link;
mod serve;

pub(super) use edit::{run_apply, run_edit_read, run_edit_write};
pub(super) use history::{run_changelog, run_diff, run_log, run_save};
pub(super) use inspect::{run_info, run_verify};
pub(super) use io::{run_export, run_import};
pub(super) use link::run_link;
pub(super) use serve::run_serve;
