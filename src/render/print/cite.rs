//! Cite chunks → print blocks + References appendix.

use std::collections::HashMap;

use ariadnes_weave::{BreakHint, InlineStyle, PrintBlock, TextRun};

use crate::catalog::chunk::CitePayload;
use crate::catalog::file::TesFile;
use crate::catalog::index::ChunkIndexEntry;
use crate::error::Result;
use crate::io::bib::{BibEntry, format_numeric_marker, format_reference_body};
use crate::io::cite::{CiteTessprekKind, classify_cite};
use crate::io::export::{decode_cite_entry, decode_numbered_cite};

use super::plain_paragraph;

pub(crate) fn push_cite_block(
    file: &TesFile,
    entry: &ChunkIndexEntry,
    cite_numbers: &HashMap<u64, usize>,
    blocks: &mut Vec<PrintBlock>,
    bib_items: &mut Vec<(usize, BibEntry)>,
) -> Result<()> {
    let cite = decode_cite_entry(file, entry)?;
    match classify_cite(&cite) {
        CiteTessprekKind::Biblio => {
            let (n, cite, bib) = decode_numbered_cite(file, entry, cite_numbers)?;
            let label = cite_stub_label(&cite, &bib);
            let marker = format_numeric_marker(n);
            blocks.push(PrintBlock::paragraph(vec![
                TextRun {
                    text: marker,
                    style: InlineStyle {
                        cite: true,
                        ..Default::default()
                    },
                    face: None,
                    link_uri: None,
                },
                TextRun::plain(format!(" {label}")),
            ]));
            bib_items.push((n, bib));
        }
        CiteTessprekKind::Quote => {
            let quote = cite.quote.trim();
            if !quote.is_empty() {
                blocks.push(PrintBlock::quote(vec![TextRun::plain(quote)]));
            }
        }
        CiteTessprekKind::Ref => {
            blocks.push(plain_paragraph(ref_marker_text(&cite)));
        }
    }
    Ok(())
}

fn cite_stub_label(cite: &CitePayload, bib: &BibEntry) -> String {
    let label = cite
        .label
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(bib.cite_key.as_str());
    if label.is_empty() {
        "unknown".into()
    } else {
        label.to_owned()
    }
}

fn ref_marker_text(cite: &CitePayload) -> String {
    let mut parts = Vec::new();
    if let Some(label) = cite.label.as_deref().filter(|s| !s.is_empty()) {
        parts.push(label.to_owned());
    }
    if let Some(doc) = cite.target_doc_id.as_deref() {
        parts.push(format!("doc:{doc}"));
    }
    if let Some(chunk) = cite.target_chunk_id {
        parts.push(format!("chunk:{chunk}"));
    }
    if parts.is_empty() {
        "ref".into()
    } else {
        format!("[ref: {}]", parts.join(" "))
    }
}

pub(crate) fn append_print_references(
    blocks: &mut Vec<PrintBlock>,
    bib_items: &mut [(usize, BibEntry)],
) {
    if bib_items.is_empty() {
        return;
    }
    bib_items.sort_by_key(|(n, _)| *n);
    blocks.push(PrintBlock::heading(
        2,
        vec![TextRun::plain("References")],
        BreakHint::KeepWithNext,
    ));
    for (n, entry) in bib_items.iter() {
        blocks.push(plain_paragraph(format!(
            "{n}. {}",
            format_reference_body(entry)
        )));
    }
}
