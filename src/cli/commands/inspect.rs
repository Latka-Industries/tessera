//! `tes info`, `tes verify`, and `tes repair`.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::catalog::{format_info_human, format_info_json, format_info_quiet, read_summary_v0};
use crate::error::TesError;
use crate::repair::{RepairOptions, format_repair_json, format_repair_text, repair_tes_file};
use crate::verify::{
    format_verify_human, format_verify_json, format_verify_quiet, verify_tes_file,
};

use super::super::args::RepairArgs;
use super::super::util::{exit_for, print_out};

pub(in crate::cli) fn run_info(path: &PathBuf, json: bool, quiet: bool) -> Result<(), TesError> {
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

pub(in crate::cli) fn run_verify(
    paths: &[PathBuf],
    deep: bool,
    json: bool,
    quiet: bool,
) -> ExitCode {
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

pub(in crate::cli) fn run_repair(args: RepairArgs) -> ExitCode {
    let options = RepairOptions {
        dry_run: args.dry_run,
        apply: args.apply,
        apply_all: args.apply_all,
        output: args.output,
    };
    match repair_tes_file(&args.path, &options) {
        Ok(report) => {
            let out = if args.json {
                match format_repair_json(&report) {
                    Ok(s) => s,
                    Err(err) => {
                        eprintln!("error: {err}");
                        return ExitCode::from(2);
                    }
                }
            } else {
                format_repair_text(&report)
            };
            println!("{}", out.trim_end());
            if report.verify_after_ok == Some(false) {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(err) => {
            eprintln!("error: {err}");
            exit_for(&err)
        }
    }
}
