//! Byte-exact golden fixtures for layout v0 / additive layout-v1 text.

use std::fs;
use std::path::PathBuf;

use crate::catalog::{
    AttachmentPayload, CitePayload, DocumentCatalog, FigureRef, ImagePayload, ImagePlacement,
    InlineKind, InlineSpan, LinkEntry, LinkKind, ListKind, SlidePayload, SlideRegion, TableCell,
    TableData, TableRow, TesWriterSession, TextAlign, TextHeader,
};
use crate::layout::{DocKind, SUPERBLOCK_LEN, SuperblockV0};
use crate::verify::verify_tes_file;
use uuid::Uuid;

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

fn assert_matches_encoder(name: &str, expected: Vec<u8>) {
    let path = fixtures_dir().join(name);
    let on_disk = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert_eq!(
        on_disk, expected,
        "{name} drifted — run: cargo run --example gen_v0_fixtures"
    );
    let report = verify_tes_file(&path, true).unwrap();
    assert!(report.ok, "{name} deep verify failed: {report:?}");
}

fn expected_empty() -> Vec<u8> {
    TesWriterSession::create("empty.tes", DocKind::Note)
        .encode_file()
        .unwrap()
}

fn expected_note_one_chunk() -> Vec<u8> {
    let mut session = TesWriterSession::create("note_one_chunk.tes", DocKind::Note);
    session
        .set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440000",
            "Meeting notes",
            "2026-06-05T12:00:00Z",
            "2026-06-05T12:30:00Z",
            DocKind::Note,
        ))
        .unwrap();
    session
        .add_text_chunk(&TextHeader::paragraph(), "Hello from Tessera.")
        .unwrap();
    session.encode_file().unwrap()
}

fn expected_note_three_chunks() -> Vec<u8> {
    let mut session = TesWriterSession::create("note_three_chunks.tes", DocKind::Note);
    session
        .set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440010",
            "Three chunk note",
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Note,
        ))
        .unwrap();
    session
        .add_text_chunk(&TextHeader::heading(1), "Agenda")
        .unwrap();
    session
        .add_text_chunk(&TextHeader::paragraph(), "Ship layout wire.")
        .unwrap();
    session
        .add_text_chunk(&TextHeader::list_item(ListKind::Bullet), "Add fixtures")
        .unwrap();
    session.encode_file().unwrap()
}

fn expected_hub_links() -> Vec<u8> {
    let mut session = TesWriterSession::create("hub_links.tes", DocKind::Hub);
    session
        .set_catalog(DocumentCatalog::new(
            "770e8400-e29b-41d4-a716-446655440002",
            "Fixture hub",
            "2026-06-05T12:00:00Z",
            "2026-06-05T12:30:00Z",
            DocKind::Hub,
        ))
        .unwrap();
    session
        .add_text_chunk(&TextHeader::paragraph(), "Meeting notes")
        .unwrap();
    session
        .add_link(LinkEntry::new(
            1,
            0,
            13,
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            1,
            LinkKind::Wiki,
        ))
        .unwrap();
    session.encode_file().unwrap()
}

fn expected_layout_v1_text() -> Vec<u8> {
    let mut session = TesWriterSession::create("layout_v1_text.tes", DocKind::Note);
    let mut catalog = DocumentCatalog::new(
        "550e8400-e29b-41d4-a716-446655440020",
        "Layout v1 text specimen",
        "2026-07-28T00:00:00Z",
        "2026-07-28T00:00:00Z",
        DocKind::Note,
    );
    catalog.language = Some("en".into());
    session.set_catalog(catalog).unwrap();
    session
        .add_text_chunk(&TextHeader::heading(1), "Layout wire")
        .unwrap();
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
        .unwrap();
    session
        .add_text_chunk(&TextHeader::math(), r"E = mc^2")
        .unwrap();
    session
        .add_text_chunk(&TextHeader::code_block(Some("rust")), "fn main() {}")
        .unwrap();
    session
        .add_text_chunk(
            &TextHeader::table(TableData {
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
            }),
            "",
        )
        .unwrap();
    session.encode_file().unwrap()
}

fn expected_slide_deck() -> Vec<u8> {
    let mut session = TesWriterSession::create("slide_deck.tes", DocKind::Deck);
    session
        .set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440030",
            "Fixture deck",
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Deck,
        ))
        .unwrap();
    session
        .add_text_chunk(&TextHeader::heading(1), "Hello slides")
        .unwrap();
    session
        .add_text_chunk(&TextHeader::paragraph(), "Region body copy.")
        .unwrap();
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
        .unwrap();
    session.encode_file().unwrap()
}

fn expected_research_cite() -> Vec<u8> {
    let mut session = TesWriterSession::create("research_cite.tes", DocKind::Research);
    let mut catalog = DocumentCatalog::new(
        "550e8400-e29b-41d4-a716-446655440040",
        "Research cite specimen",
        "2026-07-28T00:00:00Z",
        "2026-07-28T00:00:00Z",
        DocKind::Research,
    );
    catalog.cite_style_id = Some("numeric".into());
    session.set_catalog(catalog).unwrap();
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "We measured the effect as described.",
        )
        .unwrap();
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
        .unwrap();
    session.encode_file().unwrap()
}

fn expected_figure_sample() -> Vec<u8> {
    let mut session = TesWriterSession::create("figure_sample.tes", DocKind::Note);
    session
        .set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440050",
            "Figure specimen",
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Note,
        ))
        .unwrap();
    let image_id = session
        .add_image_chunk(&ImagePayload {
            media_type: "image/png".into(),
            width_px: 1,
            height_px: 1,
            data: PNG_1X1.to_vec(),
        })
        .unwrap();
    session
        .add_text_chunk(&TextHeader::heading(1), "Gallery")
        .unwrap();
    session
        .add_figure(&FigureRef {
            image_chunk_id: image_id,
            alt_text: "One red pixel".into(),
            caption: Some("Fixture PNG".into()),
            placement: ImagePlacement::Flow,
        })
        .unwrap();
    session.encode_file().unwrap()
}

fn expected_attachment_sample() -> Vec<u8> {
    let mut session = TesWriterSession::create("attachment_sample.tes", DocKind::Note);
    session
        .set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440070",
            "Attachment specimen",
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Note,
        ))
        .unwrap();
    session
        .add_text_chunk(&TextHeader::heading(1), "Attachments")
        .unwrap();
    session
        .add_attachment_chunk(
            &AttachmentPayload::new(
                "application/pdf",
                "notes.pdf",
                b"%PDF-1.4 fixture".to_vec(),
                Some("Sample notes".into()),
            )
            .unwrap(),
        )
        .unwrap();
    session.encode_file().unwrap()
}

fn expected_external_links() -> Vec<u8> {
    let mut session = TesWriterSession::create("external_links.tes", DocKind::Note);
    session
        .set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440060",
            "External link specimen",
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Note,
        ))
        .unwrap();
    session
        .add_text_chunk(&TextHeader::heading(1), "External links")
        .unwrap();
    let mut para = TextHeader::paragraph();
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
        .unwrap();
    session
        .add_link(
            LinkEntry::external(2, 4, 12, "https://example.com/docs", LinkKind::Wiki).unwrap(),
        )
        .unwrap();
    session
        .add_link(
            LinkEntry::external(2, 16, 24, "mailto:docs@example.com", LinkKind::Wiki).unwrap(),
        )
        .unwrap();
    session
        .add_link(LinkEntry::new(
            2,
            0,
            3,
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            1,
            LinkKind::Wiki,
        ))
        .unwrap();
    session.encode_file().unwrap()
}

#[test]
fn empty_tes_matches_encoder() {
    let path = fixtures_dir().join("empty.tes");
    let on_disk = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let expected = expected_empty();
    assert_eq!(on_disk.len(), SUPERBLOCK_LEN);
    assert_eq!(on_disk, expected);
    let sb = SuperblockV0::from_bytes(&on_disk).unwrap();
    assert_eq!(sb.doc_kind, DocKind::Note);
    assert!(!sb.catalog.is_present());
    assert!(!sb.chunk_index.is_present());
}

#[test]
fn note_one_chunk_tes_matches_encoder() {
    assert_matches_encoder("note_one_chunk.tes", expected_note_one_chunk());
}

#[test]
fn note_three_chunks_tes_matches_encoder() {
    assert_matches_encoder("note_three_chunks.tes", expected_note_three_chunks());
}

#[test]
fn hub_links_tes_matches_encoder() {
    assert_matches_encoder("hub_links.tes", expected_hub_links());
    let on_disk = fs::read(fixtures_dir().join("hub_links.tes")).unwrap();
    let sb = SuperblockV0::from_bytes(&on_disk).unwrap();
    assert_eq!(sb.doc_kind, DocKind::Hub);
    assert!(sb.link_table.is_present());
}

#[test]
fn layout_v1_text_tes_matches_encoder() {
    assert_matches_encoder("layout_v1_text.tes", expected_layout_v1_text());
}

#[test]
fn slide_deck_tes_matches_encoder() {
    assert_matches_encoder("slide_deck.tes", expected_slide_deck());
}

#[test]
fn research_cite_tes_matches_encoder() {
    assert_matches_encoder("research_cite.tes", expected_research_cite());
}

#[test]
fn figure_sample_tes_matches_encoder() {
    assert_matches_encoder("figure_sample.tes", expected_figure_sample());
}

#[test]
fn attachment_sample_tes_matches_encoder() {
    assert_matches_encoder("attachment_sample.tes", expected_attachment_sample());
}

#[test]
fn external_links_tes_matches_encoder() {
    assert_matches_encoder("external_links.tes", expected_external_links());
    let on_disk = fs::read(fixtures_dir().join("external_links.tes")).unwrap();
    let sb = SuperblockV0::from_bytes(&on_disk).unwrap();
    assert!(sb.link_table.is_present());
    // TLNK v1: version byte after magic.
    let region = sb.link_table.slice(&on_disk, "link_table").unwrap();
    assert_eq!(&region[..4], b"TLNK");
    assert_eq!(region[4], 1);
}
