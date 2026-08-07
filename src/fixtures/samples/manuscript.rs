//! Multi-chapter manuscript for `--chapter` / beta-reader PDF (`manuscript_chapters.tes`).

use crate::catalog::{TesWriterSession, TextHeader};
use crate::layout::DocKind;

use super::common::catalog;

/// Multi-chapter manuscript for `--chapter` / beta-reader PDF (`manuscript_chapters.tes`).
///
/// Conventions: H1 = chapter, H2 = scene. Front matter before the first H1 is
/// excluded when exporting with `--chapter N`.
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_manuscript_chapters() -> Vec<u8> {
    let mut session = TesWriterSession::create("manuscript_chapters.tes", DocKind::Manuscript);
    let mut cat = catalog(
        "bb0e8400-e29b-41d4-a716-446655440201",
        "Harbor Lights",
        "2026-07-30T00:00:00Z",
        "2026-07-30T00:00:00Z",
        DocKind::Manuscript,
        &["sample", "manuscript", "fiction"],
    );
    cat.language = Some("en".into());
    cat.theme_id = Some("manuscript".into());
    session.set_catalog(cat).expect("catalog");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "A working draft for beta readers. Front matter is not part of chapter 1.",
        )
        .expect("front matter");
    session
        .add_text_chunk(&TextHeader::heading(1), "Chapter 1 — The Quay")
        .expect("ch1");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Rain stitched the harbor into a single grey sheet. Mara counted the crates again.",
        )
        .expect("ch1p1");
    session
        .add_text_chunk(&TextHeader::heading(2), "Scene: Warehouse")
        .expect("ch1s1");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Inside, the air smelled of salt and tar. Someone had moved the ledger.",
        )
        .expect("ch1p2");
    session
        .add_text_chunk(&TextHeader::heading(1), "Chapter 2 — The Signal")
        .expect("ch2");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "At midnight a lantern blinked twice from the far pier. Mara did not answer.",
        )
        .expect("ch2p1");
    session
        .add_text_chunk(&TextHeader::heading(1), "Chapter 3 — Tide Turn")
        .expect("ch3");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "By dawn the quay was empty. Only the ledger remained, open to a blank page.",
        )
        .expect("ch3p1");
    session.encode_file().expect("manuscript_chapters")
}
