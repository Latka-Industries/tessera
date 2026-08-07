//! Every common text role in one note (`text_roles.tes`).

use crate::catalog::{
    InlineKind, InlineSpan, ListKind, TableData, TableRow, TesWriterSession, TextHeader, TextRole,
};
use crate::layout::DocKind;

use super::common::{catalog, cell};

/// Every common text role in one note (`text_roles.tes`).
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_text_roles() -> Vec<u8> {
    let mut session = TesWriterSession::create("text_roles.tes", DocKind::Note);
    let mut cat = catalog(
        "aa0e8400-e29b-41d4-a716-446655440101",
        "Text roles tour",
        "2026-07-29T00:00:00Z",
        "2026-07-29T00:00:00Z",
        DocKind::Note,
        &["sample", "roles", "browse"],
    );
    cat.language = Some("en".into());
    session.set_catalog(cat).expect("catalog");
    add_text_roles_intro(&mut session);
    add_text_roles_lists(&mut session);
    add_text_roles_quote_code_math_table(&mut session);
    session.encode_file().expect("text_roles")
}

fn add_text_roles_intro(session: &mut TesWriterSession) {
    session
        .add_text_chunk(&TextHeader::heading(1), "Text roles tour")
        .expect("h1");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "One document covering the usual reading-order text roles so Tessprek markers are easy to compare.",
        )
        .expect("intro");
    session
        .add_text_chunk(&TextHeader::heading(2), "Headings and prose")
        .expect("h2");
    let mut spanned = TextHeader::paragraph();
    // Body: "Strong, emphasis, underline, and code in one paragraph."
    spanned.spans = vec![
        InlineSpan {
            start: 0,
            end: 6,
            kind: InlineKind::Strong,
        },
        InlineSpan {
            start: 8,
            end: 16,
            kind: InlineKind::Emphasis,
        },
        InlineSpan {
            start: 18,
            end: 27,
            kind: InlineKind::Underline,
        },
        InlineSpan {
            start: 33,
            end: 37,
            kind: InlineKind::Code,
        },
    ];
    session
        .add_text_chunk(
            &spanned,
            "Strong, emphasis, underline, and code in one paragraph.",
        )
        .expect("spans");
}

fn add_text_roles_lists(session: &mut TesWriterSession) {
    session
        .add_text_chunk(&TextHeader::heading(3), "Lists")
        .expect("h3");
    session
        .add_text_chunk(
            &TextHeader::list_item(ListKind::Bullet),
            "Bullet: change control is per list_item chunk",
        )
        .expect("b1");
    session
        .add_text_chunk(
            &TextHeader::list_item_at(ListKind::Bullet, 2),
            "Nested bullet under the first item",
        )
        .expect("b1n");
    session
        .add_text_chunk(
            &TextHeader::list_item(ListKind::Bullet),
            "Bullet: nested structure uses list_depth on the header",
        )
        .expect("b2");
    session
        .add_text_chunk(
            &TextHeader::list_item(ListKind::Ordered),
            "Ordered first step",
        )
        .expect("o1");
    session
        .add_text_chunk(
            &TextHeader::list_item_at(ListKind::Ordered, 2),
            "Nested ordered under the first step",
        )
        .expect("o1n");
    session
        .add_text_chunk(
            &TextHeader::list_item(ListKind::Ordered),
            "Ordered second step",
        )
        .expect("o2");
}

fn add_text_roles_quote_code_math_table(session: &mut TesWriterSession) {
    session
        .add_text_chunk(&TextHeader::heading(2), "Quote, code, math, table")
        .expect("h2b");
    session
        .add_text_chunk(
            &TextHeader::with_role(TextRole::Blockquote),
            "Tessprek is a projection wire, not the authoring UX.",
        )
        .expect("quote");
    let mut code = TextHeader::code_block(Some("rust"));
    code.caption = Some("Role enum sketch".into());
    session
        .add_text_chunk(
            &code,
            "fn chunk_roles() -> &'static [&'static str] {\n    &[\"heading\", \"paragraph\", \"list_item\"]\n}",
        )
        .expect("code");
    let mut mermaid = TextHeader::code_block(Some("mermaid"));
    mermaid.caption = Some("Role pipeline".into());
    session
        .add_text_chunk(
            &mermaid,
            "flowchart TD\n    MD[Markdown] --> TP[Tessprek]\n    TP --> TES[.tes]",
        )
        .expect("mermaid");
    let mut math = TextHeader::math();
    math.caption = Some("Gauss sum".into());
    session
        .add_text_chunk(&math, r"\sum_{i=1}^{n} i = \frac{n(n+1)}{2}")
        .expect("math");
    let mut table = TextHeader::table(TableData {
        rows: vec![
            TableRow {
                cells: vec![cell("Role", true), cell("Markdown cue", true)],
            },
            TableRow {
                cells: vec![cell("heading", false), cell("# Title", false)],
            },
            TableRow {
                cells: vec![cell("list_item", false), cell("- / 1.", false)],
            },
            TableRow {
                cells: vec![cell("code_block", false), cell("```lang", false)],
            },
            TableRow {
                cells: vec![cell("math", false), cell("$$ … $$", false)],
            },
        ],
    });
    table.caption = Some("Role ↔ Markdown cues".into());
    session.add_text_chunk(&table, "").expect("table");
}
