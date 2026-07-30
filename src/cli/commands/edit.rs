//! `tes edit-read`, `tes format`, `tes edit-write`, and `tes apply`.

use std::fs;
use std::io;

use crate::edit::{
    EditWriteOptions, apply_ops, apply_patch, edit_read, edit_write, normalize_tessprek,
    parse_ops_json, tessprek_needs_format,
};
use crate::error::TesError;

use super::super::args::{ApplyArgs, EditReadArgs, EditWriteArgs, FormatArgs};
use super::super::util::{print_edit_write_report, print_out, read_edit_input, require_tessprek};

pub(in crate::cli) fn run_edit_read(args: &EditReadArgs) -> Result<(), TesError> {
    require_tessprek(&args.format)?;
    let report = edit_read(&args.path)?;
    eprintln!("source-hash={}", report.source_hash);
    if let Some(path) = args.output.as_ref() {
        fs::write(path, report.tessprek.as_bytes())?;
    } else {
        print_out(&report.tessprek)?;
    }
    Ok(())
}

pub(in crate::cli) fn run_format(args: &FormatArgs) -> Result<(), TesError> {
    require_tessprek(&args.format)?;
    let input = read_edit_input(args.stdin, args.input.as_ref())?;
    if args.check {
        return if tessprek_needs_format(&input)? {
            Err(TesError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                "Tessprek is not formatted (run `tes format`)",
            )))
        } else {
            Ok(())
        };
    }
    let out = normalize_tessprek(&input)?;
    if let Some(path) = args.output.as_ref() {
        fs::write(path, out.as_bytes())?;
    } else {
        print_out(&out)?;
    }
    Ok(())
}

pub(in crate::cli) fn run_edit_write(args: EditWriteArgs) -> Result<(), TesError> {
    require_tessprek(&args.format)?;
    let tessprek = read_edit_input(args.stdin, args.input.as_ref())?;
    let report = edit_write(
        &args.path,
        &tessprek,
        &EditWriteOptions::new(args.source_hash, args.dry_run),
    )?;
    print_edit_write_report(&report);
    Ok(())
}

pub(in crate::cli) fn run_apply(args: ApplyArgs) -> Result<(), TesError> {
    let options = EditWriteOptions::new(args.source_hash, args.dry_run);
    let report = if let Some(ops_path) = args.ops.as_ref() {
        let json = fs::read_to_string(ops_path)?;
        let ops = parse_ops_json(&json)?;
        apply_ops(&args.path, &ops, &options)?
    } else if let Some(patch_path) = args.patch.as_ref() {
        let patch = fs::read_to_string(patch_path)?;
        apply_patch(&args.path, &patch, &options)?
    } else {
        return Err(TesError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "select --ops or --patch",
        )));
    };
    print_edit_write_report(&report);
    Ok(())
}
