//! `tes` CLI command surface (`docs/cli.md`).
//!
//! Layout:
//! - [`run`] — parse argv and dispatch (called from `src/bin/tes.rs`).
//! - `args` — clap types.
//! - `commands` — per-subcommand runners (`info`, `export`, `edit-*`, …).
//! - `util` — exit codes, stdout helpers, shared parsers.
//!
//! Commands cover `info`, `verify`, `export` (including `--pdf`), CommonMark/HTML
//! `import`, vault-aware `link`, loopback `serve`, Tessera Markdown
//! `edit-read` / `edit-write` / `apply`, history `save` / `log` / `diff` /
//! `changelog` / `export-revs` / `checkout` / `blame` / `pending` / `textconv` /
//! `merge-file`, and vault catalog `vault rebuild` / `vault list`.

mod args;
mod commands;
mod util;

use std::process::ExitCode;

use clap::Parser;

use args::{Cli, Commands};
use commands::{
    run_apply, run_blame, run_changelog, run_checkout, run_diff, run_edit_read, run_edit_write,
    run_export, run_export_revs, run_import, run_info, run_link, run_log, run_merge_file,
    run_pending, run_save, run_serve, run_textconv, run_vault, run_verify,
};
use util::result_exit;

/// Parse argv and dispatch a `tes` subcommand.
#[must_use]
pub fn run() -> ExitCode {
    match Cli::parse().command {
        Commands::Info { path, json, quiet } => result_exit(run_info(&path, json, quiet)),
        Commands::Verify {
            paths,
            deep,
            json,
            quiet,
        } => run_verify(&paths, deep, json, quiet),
        Commands::Export(args) => result_exit(run_export(args)),
        Commands::Import(args) => result_exit(run_import(args)),
        Commands::Link { vault, command } => run_link(&vault, command),
        Commands::Vault { vault, command } => result_exit(run_vault(&vault, command)),
        Commands::Serve(args) => result_exit(run_serve(args)),
        Commands::EditRead(args) => result_exit(run_edit_read(&args)),
        Commands::EditWrite(args) => result_exit(run_edit_write(args)),
        Commands::Apply(args) => result_exit(run_apply(args)),
        Commands::Save(args) => result_exit(run_save(args)),
        Commands::Log(args) => result_exit(run_log(&args)),
        Commands::Diff(args) => result_exit(run_diff(&args)),
        Commands::Changelog(args) => result_exit(run_changelog(&args)),
        Commands::ExportRevs(args) => result_exit(run_export_revs(&args)),
        Commands::Checkout(args) => result_exit(run_checkout(&args)),
        Commands::Blame(args) => result_exit(run_blame(&args)),
        Commands::Pending { path, command } => result_exit(run_pending(&path, command)),
        Commands::Textconv(args) => result_exit(run_textconv(&args)),
        Commands::MergeFile(args) => result_exit(run_merge_file(&args)),
    }
}
