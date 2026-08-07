//! Byte-exact golden builders for `fixtures/v0/`.

use std::fs;
use std::path::Path;

use uuid::Uuid;

use crate::catalog::{
    AttachmentPayload, CitePayload, DocumentCatalog, FigureRef, ImagePayload, ImagePlacement,
    InlineKind, InlineSpan, LinkEntry, LinkKind, ListKind, SlidePayload, SlideRegion, TableCell,
    TableData, TableRow, TesWriterSession, TextAlign, TextHeader,
};
use crate::error::Result;
use crate::layout::DocKind;

/// Minimal valid 1×1 PNG (red pixel).
pub const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x00, 0x03, 0x00, 0x01, 0x00, 0x05, 0xFE, 0x02, 0xFE, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// Pixel width of [`PNG_SWATCH`].
pub const PNG_SWATCH_WIDTH: u32 = 240;
/// Pixel height of [`PNG_SWATCH`].
pub const PNG_SWATCH_HEIGHT: u32 = 120;

/// Visible RGB swatch for native PDF figure smoke (not a 1×1).
pub const PNG_SWATCH: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
    0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0xF0, 0x00, 0x00, 0x00, 0x78,
    0x08, 0x02, 0x00, 0x00, 0x00, 0x43, 0xD4, 0xE8, 0x70, 0x00, 0x00, 0x02,
    0xD7, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0xED, 0xD6, 0xB1, 0x51, 0x1C,
    0x41, 0x10, 0x86, 0xD1, 0x83, 0x52, 0x14, 0xD8, 0xD8, 0x0A, 0x47, 0x01,
    0x28, 0x00, 0x05, 0xA3, 0x00, 0x14, 0x80, 0xC2, 0x91, 0x4D, 0x30, 0x32,
    0x9A, 0xA2, 0xA0, 0xE0, 0x60, 0xF7, 0x76, 0x66, 0xA7, 0x7B, 0xE6, 0x3D,
    0xAB, 0xC7, 0xFB, 0x8D, 0xCF, 0x98, 0xBB, 0x87, 0x87, 0x87, 0x0B, 0xCC,
    0xE2, 0x7E, 0xF4, 0x00, 0x68, 0x49, 0xD0, 0x4C, 0xE5, 0xDB, 0xCB, 0xF5,
    0xF7, 0xF1, 0xF1, 0x72, 0xB9, 0x7C, 0xFF, 0xFD, 0x33, 0x9E, 0xFF, 0x7E,
    0xFD, 0x19, 0xB7, 0x0A, 0xF6, 0xF9, 0xF1, 0xF4, 0x14, 0xC7, 0xDD, 0xCB,
    0x1F, 0x3A, 0x82, 0x0E, 0xB2, 0xA6, 0x96, 0x2F, 0x82, 0x0E, 0xB2, 0xA6,
    0x8A, 0x4D, 0x41, 0x07, 0x59, 0x93, 0xDF, 0x8E, 0xA0, 0x83, 0xAC, 0xC9,
    0x6C, 0x77, 0xD0, 0x41, 0xD6, 0xE4, 0x74, 0x63, 0xD0, 0x41, 0xD6, 0x64,
    0x73, 0x28, 0xE8, 0x20, 0x6B, 0xF2, 0x68, 0x10, 0x74, 0x90, 0x35, 0x19,
    0x34, 0x0B, 0x3A, 0xC8, 0x9A, 0xB1, 0x1A, 0x07, 0x1D, 0x64, 0xCD, 0x28,
    0x5D, 0x82, 0x0E, 0xB2, 0xE6, 0x7C, 0x1D, 0x83, 0x0E, 0xB2, 0xE6, 0x4C,
    0xDD, 0x83, 0x0E, 0xB2, 0xE6, 0x1C, 0x27, 0x05, 0x1D, 0x64, 0x4D, 0x6F,
    0xA7, 0x06, 0x1D, 0x64, 0x4D, 0x3F, 0x03, 0x82, 0x0E, 0xB2, 0xA6, 0x87,
    0x61, 0x41, 0x07, 0x59, 0xD3, 0xD6, 0xE0, 0xA0, 0x83, 0xAC, 0x69, 0x25,
    0x45, 0xD0, 0x41, 0xD6, 0x1C, 0x97, 0x28, 0xE8, 0x20, 0x6B, 0x8E, 0x48,
    0x17, 0x74, 0x90, 0x35, 0xB7, 0x49, 0x1A, 0x74, 0x90, 0x35, 0x7B, 0xA5,
    0x0E, 0x3A, 0xC8, 0x9A, 0xED, 0x0A, 0x04, 0x1D, 0x64, 0xCD, 0x16, 0x65,
    0x82, 0x0E, 0xB2, 0xE6, 0x73, 0xC5, 0x82, 0x0E, 0xB2, 0xE6, 0x9A, 0x92,
    0x41, 0x07, 0x59, 0xF3, 0x5E, 0xE1, 0xA0, 0x83, 0xAC, 0x79, 0xAD, 0x7C,
    0xD0, 0x41, 0xD6, 0x84, 0x49, 0x82, 0x0E, 0xB2, 0x66, 0xAA, 0xA0, 0x83,
    0xAC, 0x57, 0x36, 0x61, 0xD0, 0x41, 0xD6, 0x6B, 0x9A, 0x36, 0xE8, 0x20,
    0xEB, 0xD5, 0x4C, 0x1E, 0x74, 0x90, 0xF5, 0x3A, 0x96, 0x08, 0x3A, 0xC8,
    0x7A, 0x05, 0x0B, 0x05, 0x1D, 0x64, 0x3D, 0xB7, 0xE5, 0x82, 0x0E, 0xB2,
    0x9E, 0xD5, 0xA2, 0x41, 0x07, 0x59, 0xCF, 0x67, 0xE9, 0xA0, 0x83, 0xAC,
    0x67, 0x22, 0xE8, 0x67, 0xB2, 0x9E, 0x83, 0xA0, 0xDF, 0x90, 0x75, 0x75,
    0x82, 0xFE, 0x80, 0xAC, 0xEB, 0x12, 0xF4, 0x55, 0xB2, 0xAE, 0x48, 0xD0,
    0x5F, 0x90, 0x75, 0x2D, 0x82, 0xDE, 0x44, 0xD6, 0x55, 0x08, 0x7A, 0x07,
    0x59, 0xE7, 0x27, 0xE8, 0xDD, 0x64, 0x9D, 0x99, 0xA0, 0x6F, 0x24, 0xEB,
    0x9C, 0x04, 0x7D, 0x88, 0xAC, 0xB3, 0x11, 0x74, 0x03, 0xB2, 0xCE, 0x43,
    0xD0, 0xCD, 0xC8, 0x3A, 0x03, 0x41, 0x37, 0x26, 0xEB, 0xB1, 0x04, 0xDD,
    0x85, 0xAC, 0x47, 0x11, 0x74, 0x47, 0xB2, 0x3E, 0x9F, 0xA0, 0xBB, 0x93,
    0xF5, 0x99, 0x04, 0x7D, 0x12, 0x59, 0x9F, 0x43, 0xD0, 0xA7, 0x92, 0x75,
    0x6F, 0x82, 0x1E, 0x40, 0xD6, 0xFD, 0x08, 0x7A, 0x18, 0x59, 0xF7, 0x20,
    0xE8, 0xC1, 0x64, 0xDD, 0x96, 0xA0, 0x53, 0x90, 0x75, 0x2B, 0x82, 0x4E,
    0x44, 0xD6, 0xC7, 0x09, 0x3A, 0x1D, 0x59, 0x1F, 0x21, 0xE8, 0xA4, 0x64,
    0x7D, 0x1B, 0x41, 0xA7, 0x26, 0xEB, 0xBD, 0x04, 0x5D, 0x80, 0xAC, 0xB7,
    0x13, 0x74, 0x19, 0xB2, 0xDE, 0x42, 0xD0, 0xC5, 0xC8, 0xFA, 0x73, 0x82,
    0x2E, 0x49, 0xD6, 0xD7, 0x08, 0xBA, 0x30, 0x59, 0xBF, 0x27, 0xE8, 0xF2,
    0x64, 0xFD, 0x9A, 0xA0, 0x27, 0x21, 0xEB, 0x20, 0xE8, 0xA9, 0xC8, 0x5A,
    0xD0, 0x13, 0x5A, 0x39, 0x6B, 0x41, 0x4F, 0x6B, 0xCD, 0xAC, 0x05, 0x3D,
    0xB9, 0xD5, 0xB2, 0x16, 0xF4, 0x12, 0xD6, 0xC9, 0x5A, 0xD0, 0x0B, 0x59,
    0x21, 0x6B, 0x41, 0x2F, 0x67, 0xEE, 0xAC, 0x05, 0xBD, 0xA8, 0x59, 0xB3,
    0x16, 0xF4, 0xD2, 0xE6, 0xCB, 0x5A, 0xD0, 0x4C, 0x95, 0xB5, 0xA0, 0x79,
    0x36, 0x47, 0xD6, 0x82, 0xE6, 0x8D, 0xEA, 0x59, 0x0B, 0x9A, 0x0F, 0xD4,
    0xCD, 0x5A, 0xD0, 0x5C, 0x55, 0x31, 0x6B, 0x41, 0xF3, 0x85, 0x5A, 0x59,
    0x0B, 0x9A, 0x4D, 0xAA, 0x64, 0x2D, 0x68, 0x76, 0xC8, 0x9F, 0xB5, 0xA0,
    0xD9, 0x2D, 0x73, 0xD6, 0x82, 0xE6, 0x46, 0x39, 0xB3, 0x16, 0x34, 0x87,
    0x64, 0xCB, 0x5A, 0xD0, 0x34, 0x90, 0x27, 0x6B, 0x41, 0xD3, 0x4C, 0x86,
    0xAC, 0x05, 0x4D, 0x63, 0x63, 0xB3, 0x16, 0x34, 0x5D, 0x8C, 0xCA, 0x5A,
    0xD0, 0x74, 0x74, 0x7E, 0xD6, 0x82, 0xA6, 0xBB, 0x33, 0xB3, 0x16, 0x34,
    0x27, 0x39, 0x27, 0x6B, 0x41, 0x73, 0xAA, 0xDE, 0x59, 0x0B, 0x9A, 0x01,
    0xFA, 0x65, 0x2D, 0x68, 0x86, 0xE9, 0x91, 0xB5, 0xA0, 0x19, 0xAC, 0x6D,
    0xD6, 0x82, 0x26, 0x85, 0x56, 0x59, 0x0B, 0x9A, 0x44, 0x8E, 0x67, 0x2D,
    0x68, 0xD2, 0x39, 0x92, 0xB5, 0xA0, 0x49, 0xEA, 0xB6, 0xAC, 0x05, 0x4D,
    0x6A, 0x7B, 0xB3, 0x16, 0x34, 0x05, 0x6C, 0xCF, 0x5A, 0xD0, 0x94, 0xB1,
    0x25, 0x6B, 0x41, 0x53, 0xCC, 0xE7, 0x59, 0x0B, 0x9A, 0x92, 0xAE, 0x65,
    0x2D, 0x68, 0x0A, 0x7B, 0x9F, 0xF5, 0x07, 0x41, 0xC3, 0x04, 0xEE, 0x47,
    0x0F, 0x80, 0x96, 0x04, 0xCD, 0x54, 0xFE, 0x03, 0x48, 0x26, 0xCF, 0xA8,
    0x35, 0x36, 0x5B, 0xFD, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44,
    0xAE, 0x42, 0x60, 0x82,
];

fn catalog(
    doc_id: &str,
    title: &str,
    created: &str,
    modified: &str,
    kind: DocKind,
    tags: &[&str],
) -> DocumentCatalog {
    let mut catalog = DocumentCatalog::new(doc_id, title, created, modified, kind);
    catalog.tags = tags.iter().map(|s| (*s).to_owned()).collect();
    catalog
}

/// Superblock-only skeleton.
///
/// # Panics
///
/// Panics if encoding the empty document fails.
#[must_use]
pub fn encode_empty() -> Vec<u8> {
    TesWriterSession::create("empty.tes", DocKind::Note)
        .encode_file()
        .expect("empty")
}

/// Single paragraph note (`note_one_chunk.tes`).
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_note_one_chunk() -> Vec<u8> {
    let mut session = TesWriterSession::create("note_one_chunk.tes", DocKind::Note);
    session
        .set_catalog(catalog(
            "550e8400-e29b-41d4-a716-446655440000",
            "Meeting notes",
            "2026-06-05T12:00:00Z",
            "2026-06-05T12:30:00Z",
            DocKind::Note,
            &["notes", "demo"],
        ))
        .expect("catalog");
    // Body: "Hello from Tessera — use tes textconv for readable diffs."
    // UTF-8: em dash is 3 bytes; "tes textconv" is [27, 39).
    let mut para = TextHeader::paragraph();
    para.spans = vec![InlineSpan {
        start: 27,
        end: 39,
        kind: InlineKind::Code,
    }];
    session
        .add_text_chunk(
            &para,
            "Hello from Tessera — use tes textconv for readable diffs.",
        )
        .expect("chunk");
    session.encode_file().expect("note_one_chunk")
}

/// Heading + paragraph + list items (`note_three_chunks.tes`).
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_note_three_chunks() -> Vec<u8> {
    let mut session = TesWriterSession::create("note_three_chunks.tes", DocKind::Note);
    session
        .set_catalog(catalog(
            "550e8400-e29b-41d4-a716-446655440010",
            "Three chunk note",
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Note,
            &["notes", "agenda"],
        ))
        .expect("catalog");
    session
        .add_text_chunk(&TextHeader::heading(1), "Agenda")
        .expect("heading");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Ship spans, links, media, feature flags, and vault TOC.",
        )
        .expect("p1");
    session
        .add_text_chunk(
            &TextHeader::list_item(ListKind::Bullet),
            "Catalog feature flags (optional vs required)",
        )
        .expect("li1");
    session
        .add_text_chunk(
            &TextHeader::list_item(ListKind::Bullet),
            "GitHub PR Tessprek preview",
        )
        .expect("li2");
    session
        .add_text_chunk(
            &TextHeader::list_item(ListKind::Bullet),
            "Optional vault.tes index",
        )
        .expect("li3");
    session.encode_file().expect("note_three_chunks")
}

/// Hub with internal wiki link (`hub_links.tes`).
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_hub_links() -> Vec<u8> {
    let mut session = TesWriterSession::create("hub_links.tes", DocKind::Hub);
    session
        .set_catalog(catalog(
            "770e8400-e29b-41d4-a716-446655440002",
            "Fixture hub",
            "2026-06-05T12:00:00Z",
            "2026-06-05T12:30:00Z",
            DocKind::Hub,
            &["hub", "links"],
        ))
        .expect("catalog");
    session
        .add_text_chunk(&TextHeader::paragraph(), "Meeting notes")
        .expect("chunk");
    session
        .add_link(LinkEntry::new(
            1,
            0,
            13,
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").expect("uuid"),
            1,
            LinkKind::Wiki,
        ))
        .expect("link");
    session.encode_file().expect("hub_links")
}

/// Spans / math / code / mermaid / table + captions (`layout_v1_text.tes`).
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_layout_v1_text() -> Vec<u8> {
    let mut session = TesWriterSession::create("layout_v1_text.tes", DocKind::Note);
    let mut cat = catalog(
        "550e8400-e29b-41d4-a716-446655440020",
        "Layout v1 text specimen",
        "2026-07-28T00:00:00Z",
        "2026-07-28T00:00:00Z",
        DocKind::Note,
        &["layout", "spans", "captions"],
    );
    cat.language = Some("en".into());
    session.set_catalog(cat).expect("catalog");

    session
        .add_text_chunk(&TextHeader::heading(1), "Layout wire")
        .expect("h1");
    session
        .add_text_chunk(&layout_v1_span_paragraph(), "Strong emphasis and code.")
        .expect("spans");

    let mut math = TextHeader::math();
    math.title = Some("Relativity".into());
    math.caption = Some("Mass–energy equivalence".into());
    session.add_text_chunk(&math, r"E = mc^2").expect("math");

    let mut code = TextHeader::code_block(Some("rust"));
    code.title = Some("Listing 1".into());
    code.caption = Some("Hello Tessera".into());
    session
        .add_text_chunk(&code, "fn main() {\n    println!(\"tessera\");\n}")
        .expect("code");

    let mut mermaid = TextHeader::code_block(Some("mermaid"));
    mermaid.title = Some("Pipeline".into());
    mermaid.caption = Some("Authoring flow".into());
    session
        .add_text_chunk(
            &mermaid,
            "flowchart LR\n    A[Author] --> B[.tes]\n    B --> C[Export]",
        )
        .expect("mermaid");

    let mut table = TextHeader::table(layout_v1_feature_table());
    table.title = Some("Features".into());
    table.caption = Some("Layout feature ids".into());
    session.add_text_chunk(&table, "").expect("table");

    session.encode_file().expect("layout_v1_text")
}

fn layout_v1_span_paragraph() -> TextHeader {
    let mut para = TextHeader::paragraph();
    para.lang = Some("en".into());
    para.align = Some(TextAlign::Start);
    // Body: "Strong emphasis and code."
    para.spans = vec![
        InlineSpan {
            start: 0,
            end: 6,
            kind: InlineKind::Strong,
        },
        InlineSpan {
            start: 7,
            end: 15,
            kind: InlineKind::Emphasis,
        },
        InlineSpan {
            start: 20,
            end: 24,
            kind: InlineKind::Code,
        },
    ];
    para
}

fn layout_cell(text: &str, is_header: bool, align: Option<TextAlign>) -> TableCell {
    TableCell {
        text: text.into(),
        spans: Vec::new(),
        align,
        is_header,
        rowspan: None,
        colspan: None,
    }
}

fn layout_v1_feature_table() -> TableData {
    TableData {
        rows: vec![
            TableRow {
                cells: vec![
                    layout_cell("Feature", true, None),
                    layout_cell("Id", true, Some(TextAlign::Center)),
                ],
            },
            TableRow {
                cells: vec![
                    layout_cell("Text spans", false, None),
                    layout_cell("text_spans", false, None),
                ],
            },
            TableRow {
                cells: vec![
                    layout_cell("External URIs", false, None),
                    layout_cell("external_uris", false, None),
                ],
            },
            TableRow {
                cells: vec![
                    layout_cell("Block captions", false, None),
                    layout_cell("caption", false, None),
                ],
            },
        ],
    }
}

/// Two-slide deck (`slide_deck.tes`).
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_slide_deck() -> Vec<u8> {
    let mut session = TesWriterSession::create("slide_deck.tes", DocKind::Deck);
    session
        .set_catalog(catalog(
            "550e8400-e29b-41d4-a716-446655440030",
            "Fixture deck",
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Deck,
            &["slides"],
        ))
        .expect("catalog");
    session
        .add_text_chunk(&TextHeader::heading(1), "Hello slides")
        .expect("title1");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Region body copy for the title slide.",
        )
        .expect("body1");
    session
        .add_slide(&SlidePayload {
            layout_id: "title_body".into(),
            regions: vec![
                SlideRegion {
                    name: "title".into(),
                    chunk_id: 1,
                },
                SlideRegion {
                    name: "body".into(),
                    chunk_id: 2,
                },
            ],
        })
        .expect("slide1");
    session
        .add_text_chunk(&TextHeader::heading(1), "Feature flags")
        .expect("title2");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Optional catalog features stay on layout_version 0.",
        )
        .expect("body2");
    session
        .add_slide(&SlidePayload {
            layout_id: "title_body".into(),
            regions: vec![
                SlideRegion {
                    name: "title".into(),
                    chunk_id: 4,
                },
                SlideRegion {
                    name: "body".into(),
                    chunk_id: 5,
                },
            ],
        })
        .expect("slide2");
    session.encode_file().expect("slide_deck")
}

/// Cite chunk + citation TLNK (`research_cite.tes`).
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_research_cite() -> Vec<u8> {
    let mut session = TesWriterSession::create("research_cite.tes", DocKind::Research);
    let mut cat = catalog(
        "550e8400-e29b-41d4-a716-446655440040",
        "Research cite specimen",
        "2026-07-28T00:00:00Z",
        "2026-07-28T00:00:00Z",
        DocKind::Research,
        &["research", "citations"],
    );
    cat.cite_style_id = Some("numeric".into());
    session.set_catalog(cat).expect("catalog");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "We measured the effect as described in the fixture corpus.",
        )
        .expect("prose");
    session
        .add_cite_chunk(&CitePayload {
            quote: "We measured …".into(),
            target_doc_id: Some("660e8400-e29b-41d4-a716-446655440001".into()),
            target_chunk_id: Some(12),
            target_byte_start: Some(0),
            target_byte_end: Some(42),
            label: Some("Smith2024".into()),
            page: Some(7),
            source: None,
        })
        .expect("cite");
    session.encode_file().expect("research_cite")
}

/// Image + figure (`figure_sample.tes`).
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_figure_sample() -> Vec<u8> {
    let mut session = TesWriterSession::create("figure_sample.tes", DocKind::Note);
    session
        .set_catalog(catalog(
            "550e8400-e29b-41d4-a716-446655440050",
            "Figure specimen",
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Note,
            &["media", "figures"],
        ))
        .expect("catalog");
    let image_id = session
        .add_image_chunk(&ImagePayload {
            media_type: "image/png".into(),
            width_px: 1,
            height_px: 1,
            data: PNG_1X1.to_vec(),
        })
        .expect("image");
    session
        .add_text_chunk(&TextHeader::heading(1), "Gallery")
        .expect("heading");
    session
        .add_figure(&FigureRef {
            image_chunk_id: image_id,
            alt_text: "One red pixel".into(),
            title: None,
            caption: Some("Fixture PNG used by figure + features.figures stamp.".into()),
            placement: ImagePlacement::Flow,
        })
        .expect("figure");
    session.encode_file().expect("figure_sample")
}

/// Inert PDF attachment (`attachment_sample.tes`).
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_attachment_sample() -> Vec<u8> {
    let mut session = TesWriterSession::create("attachment_sample.tes", DocKind::Note);
    session
        .set_catalog(catalog(
            "550e8400-e29b-41d4-a716-446655440070",
            "Attachment specimen",
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Note,
            &["media", "attachments"],
        ))
        .expect("catalog");
    session
        .add_text_chunk(&TextHeader::heading(1), "Attachments")
        .expect("heading");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Inert payloads stay out of reading-order export views.",
        )
        .expect("para");
    session
        .add_attachment_chunk(
            &AttachmentPayload::new(
                "application/pdf",
                "notes.pdf",
                b"%PDF-1.4 fixture".to_vec(),
                Some("Sample notes".into()),
            )
            .expect("attachment"),
        )
        .expect("add attachment");
    session.encode_file().expect("attachment_sample")
}

/// TLNK v1 external URI heap (`external_links.tes`).
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_external_links() -> Vec<u8> {
    let mut session = TesWriterSession::create("external_links.tes", DocKind::Note);
    session
        .set_catalog(catalog(
            "550e8400-e29b-41d4-a716-446655440060",
            "External link specimen",
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Note,
            &["links", "external"],
        ))
        .expect("catalog");
    session
        .add_text_chunk(&TextHeader::heading(1), "External links")
        .expect("heading");

    let mut para = TextHeader::paragraph();
    // Body: "See the docs or email us about Tessera."
    para.spans = vec![
        InlineSpan {
            start: 4,
            end: 12,
            kind: InlineKind::Link { link_id: 0 },
        },
        InlineSpan {
            start: 16,
            end: 24,
            kind: InlineKind::Link { link_id: 1 },
        },
    ];
    session
        .add_text_chunk(&para, "See the docs or email us about Tessera.")
        .expect("para");
    session
        .add_link(
            LinkEntry::external(2, 4, 12, "https://example.com/docs", LinkKind::Wiki)
                .expect("https"),
        )
        .expect("https link");
    session
        .add_link(
            LinkEntry::external(2, 16, 24, "mailto:docs@example.com", LinkKind::Wiki)
                .expect("mailto"),
        )
        .expect("mailto link");
    session
        .add_link(LinkEntry::new(
            2,
            0,
            3,
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").expect("uuid"),
            1,
            LinkKind::Wiki,
        ))
        .expect("internal link");

    session.encode_file().expect("external_links")
}

/// Write every golden under `dir` (typically `fixtures/v0`).
///
/// # Errors
///
/// Returns IO errors while creating the directory or writing files.
pub fn write_all(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    let files: &[(&str, Vec<u8>)] = &[
        ("empty.tes", encode_empty()),
        ("note_one_chunk.tes", encode_note_one_chunk()),
        ("note_three_chunks.tes", encode_note_three_chunks()),
        ("hub_links.tes", encode_hub_links()),
        ("layout_v1_text.tes", encode_layout_v1_text()),
        ("slide_deck.tes", encode_slide_deck()),
        ("research_cite.tes", encode_research_cite()),
        ("figure_sample.tes", encode_figure_sample()),
        ("attachment_sample.tes", encode_attachment_sample()),
        ("external_links.tes", encode_external_links()),
    ];
    for (name, bytes) in files {
        let path = dir.join(name);
        fs::write(&path, bytes)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}
