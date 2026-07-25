//! `tes` — command-line interface for the Tessera document format.
//!
//! See `docs/cli.md` for the full command surface. This binary currently ships
//! `info`, `verify`, and `export`; `import` lands in a later milestone.

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgGroup, Parser, Subcommand};
use tessera::catalog::{format_info_human, format_info_json, format_info_quiet, read_summary_v0};
use tessera::error::TesError;
use tessera::export::{ExportOptions, ExportView, export_view};
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

    /// Write a decoded view to stdout or -o PATH
    #[command(group(
        ArgGroup::new("view")
            .required(true)
            .args(["raw", "linear", "ai_text", "chunks_jsonl"])
    ))]
    Export {
        /// Path to a .tes file
        path: PathBuf,
        /// Concatenate text chunk bodies
        #[arg(long)]
        raw: bool,
        /// Reading-order prose with light structure markers
        #[arg(long)]
        linear: bool,
        /// LLM-oriented plain text (no exporter markup)
        #[arg(long = "ai-text")]
        ai_text: bool,
        /// One JSON object per reading-order chunk
        #[arg(long = "chunks-jsonl")]
        chunks_jsonl: bool,
        /// Restrict to a single chunk id
        #[arg(long = "chunk")]
        chunk: Option<u64>,
        /// Prefix each --raw chunk with a debug header
        #[arg(long = "include-headers")]
        include_headers: bool,
        /// Prefix each --ai-text chunk with <!-- chunk:N -->
        #[arg(long)]
        annotate: bool,
        /// Include non-text rows in --chunks-jsonl
        #[arg(long = "all-types")]
        all_types: bool,
        /// Omit cite expansion from --ai-text
        #[arg(long = "no-cites")]
        no_cites: bool,
        /// Write to PATH instead of stdout
        #[arg(short = 'o', long = "output")]
        output: Option<PathBuf>,
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
        Commands::Export {
            path,
            raw,
            linear,
            ai_text,
            chunks_jsonl,
            chunk,
            include_headers,
            annotate,
            all_types,
            no_cites,
            output,
        } => match run_export(
            &path,
            raw,
            linear,
            ai_text,
            chunks_jsonl,
            chunk,
            include_headers,
            annotate,
            all_types,
            no_cites,
            output.as_ref(),
        ) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: {err}");
                exit_for(&err)
            }
        },
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
    print_out(&out)?;
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

#[allow(clippy::too_many_arguments)]
fn run_export(
    path: &PathBuf,
    raw: bool,
    linear: bool,
    ai_text: bool,
    chunks_jsonl: bool,
    chunk: Option<u64>,
    include_headers: bool,
    annotate: bool,
    all_types: bool,
    no_cites: bool,
    output: Option<&PathBuf>,
) -> Result<(), TesError> {
    let view = if raw {
        ExportView::Raw
    } else if linear {
        ExportView::Linear
    } else if ai_text {
        ExportView::AiText
    } else if chunks_jsonl {
        ExportView::ChunksJsonl
    } else {
        return Err(TesError::ExportViewRequired);
    };

    let options = ExportOptions {
        chunk_id: chunk,
        include_headers,
        annotate,
        all_types,
        no_cites,
    };
    let out = export_view(path, view, &options)?;
    if let Some(path) = output {
        fs::write(path, out.as_bytes())?;
    } else {
        print_out(&out)?;
    }
    Ok(())
}

fn print_out(out: &str) -> Result<(), TesError> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(out.as_bytes())?;
    if !out.is_empty() && !out.ends_with('\n') {
        stdout.write_all(b"\n")?;
    }
    Ok(())
}

fn exit_for(err: &TesError) -> ExitCode {
    match err {
        TesError::Io(_) => ExitCode::from(2),
        _ => ExitCode::from(1),
    }
}
