//! Multi-column article body for THI-391 (`article_columns.tes`).

use crate::catalog::{TesWriterSession, TextHeader};
use crate::layout::DocKind;

use super::common::catalog;

/// Newspaper-style columns with mid-region heading (`article_columns.tes`).
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_article_columns() -> Vec<u8> {
    let mut session = TesWriterSession::create("article_columns.tes", DocKind::Document);
    let mut cat = catalog(
        "cc0e8400-e29b-41d4-a716-446655440301",
        "Harbor column smoke",
        "2026-08-12T00:00:00Z",
        "2026-08-12T00:00:00Z",
        DocKind::Document,
        &["sample", "columns", "print"],
    );
    cat.language = Some("en".into());
    session.set_catalog(cat).expect("catalog");

    session
        .add_text_chunk(&TextHeader::heading(1), "Harbor column smoke")
        .expect("h1");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Lead paragraph stays full measure before the column region opens.",
        )
        .expect("lead");

    session
        .add_text_chunk(&TextHeader::columns_with(2, Some(14)), "")
        .expect("columns open");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "First column-flow paragraph. Soft wrap should fill the left band before \
             spilling into the right.",
        )
        .expect("col p1");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Second paragraph continues the newspaper flow without a hard column break.",
        )
        .expect("col p2");
    session
        .add_text_chunk(&TextHeader::heading(2), "Mid heading spans")
        .expect("mid h2");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "After a spanning heading, body text resumes multi-column flow for the rest \
             of the region.",
        )
        .expect("col p3");
    session
        .add_text_chunk(&TextHeader::columns_end(), "")
        .expect("columns end");

    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Closing full-measure paragraph after the column region.",
        )
        .expect("closing");

    session.encode_file().expect("article_columns")
}
