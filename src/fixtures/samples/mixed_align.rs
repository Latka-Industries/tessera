//! Mixed per-chunk / region text align for THI-398 (`mixed_align.tes`).
//!
//! Flush-left lead, `\columns{align=justify}` body, centered aside, flush-left
//! close — one native PDF with more than one alignment. Pack
//! `[paragraph] text_align` stays the document fallback (bundled left).

use crate::catalog::{TesWriterSession, TextAlign, TextHeader};
use crate::layout::DocKind;

use super::common::catalog;

const LOREM: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod \
tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis \
nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. \
Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore \
eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt \
in culpa qui officia deserunt mollit anim id est laborum.";

fn paragraph_align(align: TextAlign) -> TextHeader {
    let mut header = TextHeader::paragraph();
    header.align = Some(align);
    header
}

/// Mixed start / justify / center alignments in one document.
///
/// Pair with `tes export --pdf --backend native` (pack default is flush-left).
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_mixed_align() -> Vec<u8> {
    let mut session = TesWriterSession::create("mixed_align.tes", DocKind::Document);
    let mut cat = catalog(
        "cc0e8400-e29b-41d4-a716-446655440398",
        "Mixed chunk align",
        "2026-08-30T00:00:00Z",
        "2026-08-30T00:00:00Z",
        DocKind::Document,
        &["sample", "align", "print"],
    );
    cat.language = Some("en".into());
    session.set_catalog(cat).expect("catalog");

    session
        .add_text_chunk(&TextHeader::heading(1), "Mixed chunk align")
        .expect("h1");
    session
        .add_text_chunk(
            &paragraph_align(TextAlign::Start),
            "Lead stays flush start (explicit align=start). The two-column region below \
             sets a region default of justify; children omit their own align and inherit. \
             Closing paragraph is flush start again — one PDF, more than one alignment.",
        )
        .expect("lead");

    let mut columns = TextHeader::columns_with(2, Some(14));
    columns.align = Some(TextAlign::Justify);
    session.add_text_chunk(&columns, "").expect("columns open");
    session
        .add_text_chunk(&TextHeader::paragraph(), LOREM)
        .expect("col p1");
    session
        .add_text_chunk(&TextHeader::paragraph(), LOREM)
        .expect("col p2");
    session
        .add_text_chunk(&TextHeader::columns_end(), "")
        .expect("columns end");

    session
        .add_text_chunk(
            &paragraph_align(TextAlign::Center),
            "A centered aside after the column region (align=center on the chunk).",
        )
        .expect("center");
    session
        .add_text_chunk(
            &paragraph_align(TextAlign::Start),
            "Closing full-measure paragraph, flush start again.",
        )
        .expect("close");

    session.encode_file().expect("mixed_align")
}
