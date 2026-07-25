//! `tes` — command-line interface for the Tessera document format.
//!
//! See `docs/cli.md` for the full command surface. This binary currently ships
//! `info` and `verify`; `export` / `import` land in later milestones.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use tessera::catalog::{format_info_human, format_info_json, format_info_quiet, read_summary_v0};
use tessera::error::TesError;
use tessera::verify::{
    format_verify_human, format_verify_json, format_verify_quiet, verify_tes_file,
};

#[derive(Debug, Parser)]
#[command(
    name = "tes",
    version,
    about = "Tessera document format CLI (.tes)",
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Summarize a .tes file (catalog + chunk table)
    Info {
        /// Path to a .tes file
        path: PathBuf,
        /// Emit full JSON (superblock, catalog, index rows)
        #[arg(long)]
        json: bool,
        /// One line: title\tchunks=N\tbytes=M
        #[arg(short, long)]
        quiet: bool,
    },

    /// Validate on-disk layout (exit 1 on failure)
    Verify {
        /// Paths to .tes files
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Decode every payload (codec + UTF-8 validation)
        #[arg(long)]
        deep: bool,
        /// Machine-readable report
        #[arg(long)]
        json: bool,
        /// One line per file: status=ok or status=failed
        #[arg(short, long)]
        quiet: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Commands::Info { path, json, quiet } => match run_info(&path, json, quiet) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: {err}");
                exit_for(&err)
            }
        },
        Commands::Verify {
            paths,
            deep,
            json,
            quiet,
        } => run_verify(&paths, deep, json, quiet),
    }
}

fn run_info(path: &PathBuf, json: bool, quiet: bool) -> Result<(), TesError> {
    let report = read_summary_v0(path)?;
    let out = if json {
        format_info_json(&report)?
    } else if quiet {
        format_info_quiet(&report)
    } else {
        format_info_human(&report)
    };
    print!("{out}");
    if !out.ends_with('\n') {
        println!();
    }
    Ok(())
}

fn run_verify(paths: &[PathBuf], deep: bool, json: bool, quiet: bool) -> ExitCode {
    let mut failed = false;
    for path in paths {
        match verify_tes_file(path, deep) {
            Ok(report) => {
                if !report.ok {
                    failed = true;
                }
                let out = if json {
                    match format_verify_json(&report) {
                        Ok(s) => s,
                        Err(err) => {
                            eprintln!("error: {err}");
                            return ExitCode::from(2);
                        }
                    }
                } else if quiet {
                    format!("{}\t{}", path.display(), format_verify_quiet(&report))
                } else {
                    format_verify_human(&report)
                };
                println!("{}", out.trim_end());
            }
            Err(err) => {
                eprintln!("error: {}: {err}", path.display());
                return exit_for(&err);
            }
        }
    }
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn exit_for(err: &TesError) -> ExitCode {
    match err {
        TesError::Io(_) => ExitCode::from(2),
        _ => ExitCode::from(1),
    }
}
