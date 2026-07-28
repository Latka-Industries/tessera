//! `tes save`, `log`, `diff`, `changelog`, `export-revs`, `checkout`, `blame`,
//! `pending`, `textconv`, `merge-file`.

use std::io;

use crate::error::TesError;
use crate::history::{
    BlameOptions, PendingActionOptions, SaveOptions, SuggestOptions, accept_pending, blame_file,
    checkout_revision, diff_revisions, export_revision, format_blame, format_blame_json,
    format_changelog, format_diff, format_log, format_pending, list_pending, merge_files,
    pending_redline, reject_pending, save_revision, suggest_pending, textconv,
};

use super::super::args::{
    BlameArgs, ChangelogArgs, CheckoutArgs, DiffArgs, ExportRevsArgs, LogArgs, MergeFileArgs,
    PendingCommands, SaveArgs, TextconvArgs,
};
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

pub(in crate::cli) fn run_export_revs(args: &ExportRevsArgs) -> Result<(), TesError> {
    export_revision(&args.path, &args.rev, &args.output, args.keep_history)?;
    println!(
        "exported {}\trev={}\tkeep-history={}",
        args.output.display(),
        args.rev,
        args.keep_history
    );
    Ok(())
}

pub(in crate::cli) fn run_checkout(args: &CheckoutArgs) -> Result<(), TesError> {
    checkout_revision(&args.path, &args.rev)?;
    println!("checked out {}\trev={}", args.path.display(), args.rev);
    Ok(())
}

pub(in crate::cli) fn run_blame(args: &BlameArgs) -> Result<(), TesError> {
    let report = blame_file(
        &args.path,
        &BlameOptions {
            chunk: args.chunk,
            rev: args.rev.clone(),
        },
    )?;
    let out = if args.json {
        format_blame_json(&report)?
    } else {
        format_blame(&report)
    };
    print_out(&out)
}

pub(in crate::cli) fn run_pending(
    path: &std::path::Path,
    command: PendingCommands,
) -> Result<(), TesError> {
    match command {
        PendingCommands::List { json } => {
            let pending = list_pending(path)?;
            if json {
                print_out(&serde_json::to_string_pretty(&pending)?)
            } else {
                print_out(&format_pending(&pending))
            }
        }
        PendingCommands::Suggest {
            ops,
            source_hash,
            message,
        } => {
            let ops_json = std::fs::read_to_string(&ops)?;
            let report = suggest_pending(
                path,
                &ops_json,
                &SuggestOptions {
                    source_hash,
                    message,
                    ..SuggestOptions::default()
                },
            )?;
            println!(
                "suggested\tpending={}\tids={}",
                report.pending_count,
                report.ids.join(",")
            );
            Ok(())
        }
        PendingCommands::Redline { source_hash } => {
            print_out(&pending_redline(path, &source_hash)?)
        }
        PendingCommands::Accept(args) => {
            let report = accept_pending(
                path,
                &PendingActionOptions {
                    source_hash: args.source_hash,
                    ids: args.ids,
                },
            )?;
            println!(
                "accepted\tids={}\tremaining={}\tnew-source-hash={}",
                report.ids.join(","),
                report.pending_count,
                report.new_source_hash.as_deref().unwrap_or("-")
            );
            Ok(())
        }
        PendingCommands::Reject(args) => {
            let report = reject_pending(
                path,
                &PendingActionOptions {
                    source_hash: args.source_hash,
                    ids: args.ids,
                },
            )?;
            println!(
                "rejected\tids={}\tremaining={}",
                report.ids.join(","),
                report.pending_count
            );
            Ok(())
        }
    }
}

pub(in crate::cli) fn run_textconv(args: &TextconvArgs) -> Result<(), TesError> {
    let out = textconv(&args.path)?;
    print_out(&out)
}

pub(in crate::cli) fn run_merge_file(args: &MergeFileArgs) -> Result<(), TesError> {
    let report = merge_files(&args.base, &args.ours, &args.theirs)?;
    println!(
        "merged\t{}\tours={}\ttheirs={}\tunchanged={}",
        report.path.display(),
        report.from_ours.len(),
        report.from_theirs.len(),
        report.unchanged.len()
    );
    Ok(())
}
