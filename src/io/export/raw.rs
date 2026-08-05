//! `--raw` text-body concatenation.

use std::fmt::Write as _;

use crate::catalog::file::TesFile;
use crate::error::Result;

use super::ExportOptions;
use super::common::{decode_text_entry, selected_text_entries};

pub(super) fn export_raw(file: &TesFile, options: &ExportOptions) -> Result<String> {
    let entries = selected_text_entries(file, options)?;
    let mut out = String::new();
    for (i, entry) in entries.iter().enumerate() {
        let (header, body) = decode_text_entry(file, entry)?;
        if options.include_headers {
            let _ = writeln!(
                out,
                "[chunk_id={} role={}{}]",
                entry.chunk_id,
                header.role.as_str(),
                header
                    .level
                    .map(|l| format!(" level={l}"))
                    .unwrap_or_default()
            );
        }
        out.push_str(&body);
        if i + 1 < entries.len() {
            out.push_str("\n\n");
        }
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}
