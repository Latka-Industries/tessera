//! Article front matter + titled bands for THI-411 / 412 / 414.

use crate::catalog::{TesWriterSession, TextHeader};
use crate::layout::DocKind;

use super::common::catalog;

/// Journal-shaped sample: authors, abstract, keywords, theorem, callout, 2-col body.
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_article_bands() -> Vec<u8> {
    let mut session = TesWriterSession::create("article_bands.tes", DocKind::Research);
    let mut cat = catalog(
        "dd0e8400-e29b-41d4-a716-446655440401",
        "On the trace of a weighted mean",
        "2026-08-31T00:00:00Z",
        "2026-08-31T00:00:00Z",
        DocKind::Research,
        &["sample", "article", "callout"],
    );
    cat.language = Some("en".into());
    cat.template_id = Some("article".into());
    cat.cite_style_id = Some("numeric".into());
    session.set_catalog(cat).expect("catalog");

    session
        .add_text_chunk(&TextHeader::heading(1), "On the trace of a weighted mean")
        .expect("h1");
    session
        .add_text_chunk(
            &TextHeader::callout("author", Some("A. Author".into())),
            "Lab One — a.author@example.org",
        )
        .expect("author");
    session
        .add_text_chunk(
            &TextHeader::callout("abstract", Some("Abstract".into())),
            "We record a named definition and a note so native print can paint one titled band.",
        )
        .expect("abstract");
    session
        .add_text_chunk(
            &TextHeader::callout("keywords", Some("Keywords".into())),
            "trace, weighted mean, native PDF",
        )
        .expect("keywords");
    session
        .add_text_chunk(
            &TextHeader::callout("definition", Some("Trace minimale".into())),
            "Une trace minimale est une trace dont le support ne peut pas être réduit.",
        )
        .expect("definition");
    session
        .add_text_chunk(
            &TextHeader::callout("proof", None),
            "Immediate from the definition of support.",
        )
        .expect("proof");
    session
        .add_text_chunk(
            &TextHeader::callout("note", Some("Note".into())),
            "The same band paints homework Q&A and rho note/info; kind is IR-only.",
        )
        .expect("note");
    session
        .add_text_chunk(
            &TextHeader::callout("question", Some("Question 1".into())),
            "What does the left rule mark?",
        )
        .expect("question");
    session
        .add_text_chunk(
            &TextHeader::callout("answer", Some("Answer".into())),
            "A titled band, not a publisher theorem class.",
        )
        .expect("answer");

    session
        .add_text_chunk(&TextHeader::columns_with(2, Some(16)), "")
        .expect("columns open");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Body columns start after the full-width bands. Pair this sample with pack `article` \
             (journal chrome) or `review` (line-number gutter). jimis is the wording witness, \
             not a PDF to clone.",
        )
        .expect("col body");
    session
        .add_text_chunk(&TextHeader::columns_end(), "")
        .expect("columns end");

    session.encode_file().expect("article_bands")
}
