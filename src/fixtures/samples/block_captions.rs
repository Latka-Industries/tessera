//! Every caption surface in one note (`block_captions.tes`).

use crate::catalog::{AttachmentPayload, TableData, TableRow, TesWriterSession, TextHeader};
use crate::layout::DocKind;

use super::common::{add_flow_figure, add_swatch_image, catalog, cell};

/// Every caption surface in one note (`block_captions.tes`).
///
/// Covers `\block{caption=…}` targets (table / math / code / mermaid) plus
/// figure and attachment captions so Tessprek edit-read shows the full set.
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_block_captions() -> Vec<u8> {
    let mut session = TesWriterSession::create("block_captions.tes", DocKind::Note);
    let mut cat = catalog(
        "aa0e8400-e29b-41d4-a716-446655440104",
        "Block captions",
        "2026-08-04T00:00:00Z",
        "2026-08-04T00:00:00Z",
        DocKind::Note,
        &["sample", "captions", "browse"],
    );
    cat.language = Some("en".into());
    session.set_catalog(cat).expect("catalog");

    session
        .add_text_chunk(&TextHeader::heading(1), "Block captions")
        .expect("h1");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "One container for every caption-bearing chunk type: table, math, code (incl. mermaid), figure, and attachment.",
        )
        .expect("intro");

    let mut code = TextHeader::code_block(Some("rust"));
    code.title = Some("Listing: greet".into());
    code.caption = Some("Prints a short hello.".into());
    session
        .add_text_chunk(&code, "fn greet() {\n    println!(\"hi\");\n}")
        .expect("code");

    let mut mermaid = TextHeader::code_block(Some("mermaid"));
    mermaid.title = Some("Encode path".into());
    mermaid.caption = Some("Author saves; Tessera returns a .tes.".into());
    session
        .add_text_chunk(
            &mermaid,
            "sequenceDiagram\n    participant A as Author\n    participant T as Tessera\n    A->>T: save\n    T-->>A: .tes",
        )
        .expect("mermaid");

    let mut math = TextHeader::math();
    math.title = Some("Pythagoras".into());
    math.caption = Some("Right-triangle identity.".into());
    session
        .add_text_chunk(&math, r"a^2 + b^2 = c^2")
        .expect("math");

    let mut table = TextHeader::table(TableData {
        rows: vec![
            TableRow {
                cells: vec![cell("Surface", true), cell("Wire", true)],
            },
            TableRow {
                cells: vec![
                    cell("table / math / code", false),
                    cell("TextHeader.title + caption", false),
                ],
            },
            TableRow {
                cells: vec![
                    cell("figure", false),
                    cell("FigureRef.title + caption", false),
                ],
            },
            TableRow {
                cells: vec![
                    cell("attachment", false),
                    cell("AttachmentPayload.caption", false),
                ],
            },
        ],
    });
    table.title = Some("Caption fields".into());
    table.caption = Some("Title sits above; caption sits below.".into());
    session.add_text_chunk(&table, "").expect("table");

    let image_id = add_swatch_image(&mut session).expect("image");
    add_flow_figure(
        &mut session,
        image_id,
        "Alignment swatch",
        Some("Fixture PNG"),
        Some("Stands in for a figure still."),
    )
    .expect("figure");
    session
        .add_attachment_chunk(
            &AttachmentPayload::new(
                "application/pdf",
                "caption-notes.pdf",
                b"%PDF-1.4 block captions".to_vec(),
                Some("Attachment: sample notes".into()),
            )
            .expect("attachment"),
        )
        .expect("add attachment");

    session.encode_file().expect("block_captions")
}
