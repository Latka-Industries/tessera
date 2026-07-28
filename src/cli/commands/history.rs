//! `tes save`, `tes log`, `tes diff`, and `tes changelog`.

use std::io;

use crate::error::TesError;
use crate::history::{
    SaveOptions, diff_revisions, format_changelog, format_diff, format_log, save_revision,
};

use super::super::args::{ChangelogArgs, DiffArgs, LogArgs, SaveArgs};
use super::super::util::print_out;

pub(in crate::cli) fn run_save(args: SaveArgs) -> Result<(), TesError> {
    let report = save_revision(
        &args.path,
        &SaveOptions {
            draft: args.draft,
            message: args.message,
            ..SaveOptions::default()
        },
    )?;
    println!(
        "saved {}\trev={}\tdraft={}\trevisions={}",
        report.path.display(),
        report.revision_id,
        report.draft.as_deref().unwrap_or("-"),
        report.revision_count
    );
    Ok(())
}

pub(in crate::cli) fn run_log(args: &LogArgs) -> Result<(), TesError> {
    let out = format_log(&args.path, args.json)?;
    print_out(&out)
}

pub(in crate::cli) fn run_diff(args: &DiffArgs) -> Result<(), TesError> {
    let report = diff_revisions(&args.path, &args.left, &args.right)?;
    print_out(&format_diff(&report))
}

pub(in crate::cli) fn run_changelog(args: &ChangelogArgs) -> Result<(), TesError> {
    if args.between.len() != 2 {
        return Err(TesError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--between requires LEFT RIGHT",
        )));
    }
    let out = format_changelog(&args.path, &args.between[0], &args.between[1])?;
    print_out(&out)
}
