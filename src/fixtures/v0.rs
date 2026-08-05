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
