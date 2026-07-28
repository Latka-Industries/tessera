//! Regenerate golden `.tes` fixtures under `fixtures/v0/`.
//!
//! ```bash
//! cargo run --example gen_v0_fixtures
//! cp fixtures/v0/*.tes fixtures/conformance/accept/
//! ```
//!
//! Values are fixed so byte-exact CI tests stay stable.

use std::fs;
use std::path::PathBuf;

use tessera_doc::catalog::{
    CitePayload, DocumentCatalog, FigureRef, ImagePayload, ImagePlacement, InlineKind, InlineSpan,
    LinkEntry, LinkKind, ListKind, SlidePayload, SlideRegion, TableCell, TableData, TableRow,
    TesWriterSession, TextAlign, TextHeader,
};
use tessera_doc::layout::DocKind;
use uuid::Uuid;

/// Minimal valid 1×1 PNG (red pixel).
const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x00, 0x03, 0x00, 0x01, 0x00, 0x05, 0xFE, 0x02, 0xFE, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/v0")
}

fn write_empty(dir: &std::path::Path) {
    let path = dir.join("empty.tes");
    let _ = fs::remove_file(&path);
    TesWriterSession::create(&path, DocKind::Note)
        .commit()
        .expect("write empty.tes");
    println!("wrote {}", path.display());
}

fn write_note_one_chunk(dir: &std::path::Path) {
    let path = dir.join("note_one_chunk.tes");
    let _ = fs::remove_file(&path);
    let mut session = TesWriterSession::create(&path, DocKind::Note);
    session
        .set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440000",
            "Meeting notes",
            "2026-06-05T12:00:00Z",
            "2026-06-05T12:30:00Z",
            DocKind::Note,
        ))
        .expect("catalog");
    session
        .add_text_chunk(&TextHeader::paragraph(), "Hello from Tessera.")
        .expect("chunk");
    session.commit().expect("write note_one_chunk.tes");
    println!("wrote {}", path.display());
}

fn write_note_three_chunks(dir: &std::path::Path) {
    let path = dir.join("note_three_chunks.tes");
    let _ = fs::remove_file(&path);
    let mut session = TesWriterSession::create(&path, DocKind::Note);
    session
        .set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440010",
            "Three chunk note",
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Note,
        ))
        .expect("catalog");
    session
        .add_text_chunk(&TextHeader::heading(1), "Agenda")
        .expect("heading");
    session
        .add_text_chunk(&TextHeader::paragraph(), "Ship layout wire.")
        .expect("p1");
    session
        .add_text_chunk(&TextHeader::list_item(ListKind::Bullet), "Add fixtures")
        .expect("li");
    session.commit().expect("write note_three_chunks.tes");
    println!("wrote {}", path.display());
}

fn write_hub_links(dir: &std::path::Path) {
    let path = dir.join("hub_links.tes");
    let _ = fs::remove_file(&path);
    let mut session = TesWriterSession::create(&path, DocKind::Hub);
    session
        .set_catalog(DocumentCatalog::new(
            "770e8400-e29b-41d4-a716-446655440002",
            "Fixture hub",
            "2026-06-05T12:00:00Z",
            "2026-06-05T12:30:00Z",
            DocKind::Hub,
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
    session.commit().expect("write hub_links.tes");
    println!("wrote {}", path.display());
}

fn write_layout_v1_text(dir: &std::path::Path) {
    let path = dir.join("layout_v1_text.tes");
    let _ = fs::remove_file(&path);
    let mut session = TesWriterSession::create(&path, DocKind::Note);
    let mut catalog = DocumentCatalog::new(
        "550e8400-e29b-41d4-a716-446655440020",
        "Layout v1 text specimen",
        "2026-07-28T00:00:00Z",
        "2026-07-28T00:00:00Z",
        DocKind::Note,
    );
    catalog.language = Some("en".into());
    session.set_catalog(catalog).expect("catalog");

    session
        .add_text_chunk(&TextHeader::heading(1), "Layout wire")
        .expect("h1");

    let mut para = TextHeader::paragraph();
    para.lang = Some("en".into());
    para.align = Some(TextAlign::Start);
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
    ];
    session
        .add_text_chunk(&para, "Strong emphasis follows.")
        .expect("spans");

    session
        .add_text_chunk(&TextHeader::math(), r"E = mc^2")
        .expect("math");

    session
        .add_text_chunk(&TextHeader::code_block(Some("rust")), "fn main() {}")
        .expect("code");

    let table = TextHeader::table(TableData {
        rows: vec![
            TableRow {
                cells: vec![
                    TableCell {
                        text: "A".into(),
                        spans: Vec::new(),
                        align: None,
                        is_header: true,
                        rowspan: None,
                        colspan: None,
                    },
                    TableCell {
                        text: "B".into(),
                        spans: Vec::new(),
                        align: Some(TextAlign::Center),
                        is_header: true,
                        rowspan: None,
                        colspan: None,
                    },
                ],
            },
            TableRow {
                cells: vec![
                    TableCell {
                        text: "1".into(),
                        spans: Vec::new(),
                        align: None,
                        is_header: false,
                        rowspan: None,
                        colspan: None,
                    },
                    TableCell {
                        text: "2".into(),
                        spans: Vec::new(),
                        align: None,
                        is_header: false,
                        rowspan: None,
                        colspan: None,
                    },
                ],
            },
        ],
    });
    session.add_text_chunk(&table, "").expect("table");

    session.commit().expect("write layout_v1_text.tes");
    println!("wrote {}", path.display());
}

fn write_slide_deck(dir: &std::path::Path) {
    let path = dir.join("slide_deck.tes");
    let _ = fs::remove_file(&path);
    let mut session = TesWriterSession::create(&path, DocKind::Deck);
    session
        .set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440030",
            "Fixture deck",
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Deck,
        ))
        .expect("catalog");
    session
        .add_text_chunk(&TextHeader::heading(1), "Hello slides")
        .expect("title text");
    session
        .add_text_chunk(&TextHeader::paragraph(), "Region body copy.")
        .expect("body text");
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
        .expect("slide");
    session.commit().expect("write slide_deck.tes");
    println!("wrote {}", path.display());
}

fn write_research_cite(dir: &std::path::Path) {
    let path = dir.join("research_cite.tes");
    let _ = fs::remove_file(&path);
    let mut session = TesWriterSession::create(&path, DocKind::Research);
    let mut catalog = DocumentCatalog::new(
        "550e8400-e29b-41d4-a716-446655440040",
        "Research cite specimen",
        "2026-07-28T00:00:00Z",
        "2026-07-28T00:00:00Z",
        DocKind::Research,
    );
    catalog.cite_style_id = Some("numeric".into());
    session.set_catalog(catalog).expect("catalog");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "We measured the effect as described.",
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
    session.commit().expect("write research_cite.tes");
    println!("wrote {}", path.display());
}

fn write_figure_sample(dir: &std::path::Path) {
    let path = dir.join("figure_sample.tes");
    let _ = fs::remove_file(&path);
    let mut session = TesWriterSession::create(&path, DocKind::Note);
    session
        .set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440050",
            "Figure specimen",
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Note,
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
            caption: Some("Fixture PNG".into()),
            placement: ImagePlacement::Flow,
        })
        .expect("figure");
    session.commit().expect("write figure_sample.tes");
    println!("wrote {}", path.display());
}

fn write_external_links(dir: &std::path::Path) {
    let path = dir.join("external_links.tes");
    let _ = fs::remove_file(&path);
    let mut session = TesWriterSession::create(&path, DocKind::Note);
    session
        .set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440060",
            "External link specimen",
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Note,
        ))
        .expect("catalog");
    session
        .add_text_chunk(&TextHeader::heading(1), "External links")
        .expect("heading");

    let mut para = TextHeader::paragraph();
    // Body: "See the docs or email us."
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
        .add_text_chunk(&para, "See the docs or email us.")
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
    // Mixed table: keep an internal edge so v1 rows coexist with UUID targets.
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

    session.commit().expect("write external_links.tes");
    println!("wrote {}", path.display());
}

fn main() {
    let dir = fixtures_dir();
    fs::create_dir_all(&dir).expect("fixtures/v0");
    write_empty(&dir);
    write_note_one_chunk(&dir);
    write_note_three_chunks(&dir);
    write_hub_links(&dir);
    write_layout_v1_text(&dir);
    write_slide_deck(&dir);
    write_research_cite(&dir);
    write_figure_sample(&dir);
    write_external_links(&dir);
}
