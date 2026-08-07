//! Multi-section research-style note with cite + table (`field_notes.tes`).

use crate::catalog::{
    CitePayload, ListKind, TableData, TableRow, TesWriterSession, TextHeader, TextRole,
};
use crate::layout::DocKind;

use super::common::{catalog, cell};

/// Multi-section research-style note with cite + table (`field_notes.tes`).
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_field_notes() -> Vec<u8> {
    let mut session = TesWriterSession::create("field_notes.tes", DocKind::Research);
    let mut cat = catalog(
        "aa0e8400-e29b-41d4-a716-446655440102",
        "Encoding field notes — week 1",
        "2026-07-20T14:00:00Z",
        "2026-07-22T09:30:00Z",
        DocKind::Research,
        &["sample", "research", "browse"],
    );
    cat.cite_style_id = Some("numeric".into());
    cat.language = Some("en".into());
    session.set_catalog(cat).expect("catalog");
    add_field_notes_questions(&mut session);
    add_field_notes_observations(&mut session);
    add_field_notes_scorecard_method_cite(&mut session);
    session.encode_file().expect("field_notes")
}

fn add_field_notes_questions(session: &mut TesWriterSession) {
    session
        .add_text_chunk(&TextHeader::heading(1), "Encoding field notes — week 1")
        .expect("h1");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Notes from the first week of Tessprek chunk-boundary experiments. Section and list structure stay intentional so later apply ops can target a single finding.",
        )
        .expect("intro");
    session
        .add_text_chunk(&TextHeader::heading(2), "Questions")
        .expect("h2q");
    session
        .add_text_chunk(
            &TextHeader::list_item(ListKind::Ordered),
            "Which text role maps cleanly to a single edit-read marker?",
        )
        .expect("q1");
    session
        .add_text_chunk(
            &TextHeader::list_item(ListKind::Ordered),
            "Do captioned tables need a separate apply path from plain paragraphs?",
        )
        .expect("q2");
    session
        .add_text_chunk(
            &TextHeader::list_item(ListKind::Ordered),
            "Where should cite chunks sit relative to the quoting prose?",
        )
        .expect("q3");
}

fn add_field_notes_observations(session: &mut TesWriterSession) {
    session
        .add_text_chunk(&TextHeader::heading(2), "Observations")
        .expect("h2o");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Heading-then-list boundaries were stable under rewrite. Math and code captions survived round-trip; blockquotes were easier to target when they owned their own chunk.",
        )
        .expect("obs");
    session
        .add_text_chunk(
            &TextHeader::with_role(TextRole::Blockquote),
            "If the marker names the role, I stop guessing which apply op to send.",
        )
        .expect("quote");
    session
        .add_text_chunk(
            &TextHeader::list_item(ListKind::Bullet),
            "Open question: nested lists vs flat index ids",
        )
        .expect("b1");
    session
        .add_text_chunk(
            &TextHeader::list_item(ListKind::Bullet),
            "Open question: cite label vs inline span ownership",
        )
        .expect("b2");
    session
        .add_text_chunk(
            &TextHeader::list_item(ListKind::Bullet),
            "Shared win: one container can carry table, math, code, and cite",
        )
        .expect("b3");
}

fn add_field_notes_scorecard_method_cite(session: &mut TesWriterSession) {
    session
        .add_text_chunk(&TextHeader::heading(2), "Rough scorecard")
        .expect("h2t");
    let mut scorecard = TextHeader::table(TableData {
        rows: vec![
            TableRow {
                cells: vec![
                    cell("Surface", true),
                    cell("Trials", true),
                    cell("Next step", true),
                ],
            },
            TableRow {
                cells: vec![
                    cell("Headings", false),
                    cell("3", false),
                    cell("Keep H1/H2 split", false),
                ],
            },
            TableRow {
                cells: vec![
                    cell("Tables", false),
                    cell("1", false),
                    cell("Caption round-trip check", false),
                ],
            },
            TableRow {
                cells: vec![
                    cell("Cites", false),
                    cell("2", false),
                    cell("Label + page stub", false),
                ],
            },
        ],
    });
    scorecard.caption = Some("Week-1 encoding scorecard".into());
    session.add_text_chunk(&scorecard, "").expect("table");
    session
        .add_text_chunk(&TextHeader::heading(2), "Method note")
        .expect("h2m");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Chunk growth vs baseline size is summarized with a simple ratio once both encodes exist:",
        )
        .expect("method");
    let mut math = TextHeader::math();
    math.caption = Some("Relative size delta".into());
    session
        .add_text_chunk(
            &math,
            r"\Delta = \frac{S_{\mathrm{after}} - S_{\mathrm{before}}}{S_{\mathrm{before}}}",
        )
        .expect("math");
    let mut code = TextHeader::code_block(Some("bash"));
    code.caption = Some("Export and verify".into());
    session
        .add_text_chunk(
            &code,
            "tes export field_notes.tes --markdown -o /tmp/field_notes.md\ntes verify --deep field_notes.tes",
        )
        .expect("code");
    session
        .add_text_chunk(&TextHeader::heading(2), "Citation stub")
        .expect("h2c");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Earlier container pilots reported similar gains when cites stayed adjacent to the quoting paragraph.",
        )
        .expect("cite prose");
    session
        .add_cite_chunk(&CitePayload {
            quote: "Adjacent cite chunks cut mis-apply rates on quote-heavy notes.".into(),
            target_doc_id: Some("660e8400-e29b-41d4-a716-446655440001".into()),
            target_chunk_id: Some(3),
            target_byte_start: Some(0),
            target_byte_end: Some(64),
            label: Some("TessprekNotes2024".into()),
            page: Some(12),
            source: None,
        })
        .expect("cite");
}
