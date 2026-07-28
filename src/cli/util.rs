//! Shared CLI helpers: exit codes, stdout, template-root env, edit input.

use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use crate::error::TesError;
use crate::layout::DocKind;

pub(super) fn result_exit(result: Result<(), TesError>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            exit_for(&err)
        }
    }
}

pub(super) fn exit_for(err: &TesError) -> ExitCode {
    match err {
        TesError::Io(_) | TesError::PdfEngine { .. } => ExitCode::from(2),
        _ => ExitCode::from(1),
    }
}

pub(super) fn print_out(out: &str) -> Result<(), TesError> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(out.as_bytes())?;
    if !out.is_empty() && !out.ends_with('\n') {
        stdout.write_all(b"\n")?;
    }
    Ok(())
}

pub(super) fn resolve_template_root(cli: Option<PathBuf>) -> PathBuf {
    cli.or_else(|| env::var_os("TES_TEMPLATE_ROOT").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("templates"))
}

pub(super) fn require_tessprek(format: &str) -> Result<(), TesError> {
    if format == "tessprek" || format == "tessera-markdown" {
        Ok(())
    } else {
        Err(TesError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported edit format '{format}' (use tessprek)"),
        )))
    }
}

pub(super) fn read_edit_input(stdin: bool, input: Option<&PathBuf>) -> Result<String, TesError> {
    if stdin {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        Ok(buf)
    } else if let Some(path) = input {
        Ok(fs::read_to_string(path)?)
    } else {
        Err(TesError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "provide --stdin or -i/--input",
        )))
    }
}

pub(super) fn print_edit_write_report(report: &crate::edit::EditWriteReport) {
    if report.replaced {
        if let Some(hash) = report.new_source_hash.as_ref() {
            eprintln!("replaced\tnew-source-hash={hash}");
        } else {
            eprintln!("replaced");
        }
    } else {
        eprintln!("dry-run\t(no replace)");
        print!("{}", report.diff);
    }
}

pub(super) fn parse_doc_kind(value: &str) -> Result<DocKind, TesError> {
    match value {
        "note" => Ok(DocKind::Note),
        "document" => Ok(DocKind::Document),
        "manuscript" => Ok(DocKind::Manuscript),
        "research" => Ok(DocKind::Research),
        "deck" => Ok(DocKind::Deck),
        "wiki_page" => Ok(DocKind::WikiPage),
        "hub" => Ok(DocKind::Hub),
        "index" => Ok(DocKind::Index),
        _ => Err(TesError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown doc kind '{value}'"),
        ))),
    }
}
