//! Browse / demo `.tes` samples (not byte-golden).
//!
//! These are for exploring Tessprek chunk shapes in Neovim / CLI. Regenerate with
//! `cargo run --example gen_sample_fixtures`. Do not assert on-disk bytes in CI.

use std::fs;
use std::path::Path;

use uuid::Uuid;

use crate::catalog::{
    AttachmentPayload, CitePayload, DocumentCatalog, FigureRef, ImagePayload, ImagePlacement,
    InlineKind, InlineSpan, LinkEntry, LinkKind, ListKind, SlidePayload, SlideRegion, TableCell,
    TableData, TableRow, TesWriterSession, TextHeader, TextRole,
};
use crate::error::Result;
use crate::layout::DocKind;

use super::v0::PNG_1X1;

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

fn cell(text: &str, is_header: bool) -> TableCell {
    TableCell {
        text: text.into(),
        spans: Vec::new(),
        align: None,
        is_header,
        rowspan: None,
        colspan: None,
    }
}

fn title_body_slide(title_id: u64, body_id: u64) -> SlidePayload {
    SlidePayload {
        layout_id: "title_body".into(),
        regions: vec![
            SlideRegion {
                name: "title".into(),
                chunk_id: title_id,
            },
            SlideRegion {
                name: "body".into(),
                chunk_id: body_id,
            },
        ],
    }
}

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
    session
        .add_text_chunk(
            &TextHeader::code_block(Some("rust")),
            "fn chunk_roles() -> &'static [&'static str] {\n    &[\"heading\", \"paragraph\", \"list_item\"]\n}",
        )
        .expect("code");
    session
        .add_text_chunk(&TextHeader::math(), r"\sum_{i=1}^{n} i = \frac{n(n+1)}{2}")
        .expect("math");
    session
        .add_text_chunk(
            &TextHeader::table(TableData {
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
            }),
            "",
        )
        .expect("table");
}

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
    session
        .add_text_chunk(
            &TextHeader::table(TableData {
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
            }),
            "",
        )
        .expect("table");
    session
        .add_text_chunk(&TextHeader::heading(2), "Method note")
        .expect("h2m");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Chunk growth vs baseline size is summarized with a simple ratio once both encodes exist:",
        )
        .expect("method");
    session
        .add_text_chunk(
            &TextHeader::math(),
            r"\Delta = \frac{S_{\mathrm{after}} - S_{\mathrm{before}}}{S_{\mathrm{before}}}",
        )
        .expect("math");
    session
        .add_text_chunk(
            &TextHeader::code_block(Some("bash")),
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

/// Mixed media + deck regions (`studio_brief.tes`).
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_studio_brief() -> Vec<u8> {
    let mut session = TesWriterSession::create("studio_brief.tes", DocKind::Deck);
    session
        .set_catalog(catalog(
            "aa0e8400-e29b-41d4-a716-446655440103",
            "Studio brief — product walkthrough",
            "2026-07-25T16:00:00Z",
            "2026-07-25T18:00:00Z",
            DocKind::Deck,
            &["sample", "deck", "media", "browse"],
        ))
        .expect("catalog");
    add_studio_title_slide(&mut session);
    add_studio_visual_slide(&mut session);
    add_studio_assets_slide(&mut session);
    session.encode_file().expect("studio_brief")
}

fn add_studio_title_slide(session: &mut TesWriterSession) {
    session
        .add_text_chunk(&TextHeader::heading(1), "Studio brief")
        .expect("t1");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Walkthrough deck with figure, attachment, and external links in one container.",
        )
        .expect("b1");
    session.add_slide(&title_body_slide(1, 2)).expect("slide1");
}

fn add_studio_visual_slide(session: &mut TesWriterSession) {
    session
        .add_text_chunk(&TextHeader::heading(1), "Visual proof")
        .expect("t2");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Hero still is a 1×1 fixture PNG; real packs would point at theme assets off-doc.",
        )
        .expect("b2");
    let image_id = session
        .add_image_chunk(&ImagePayload {
            media_type: "image/png".into(),
            width_px: 1,
            height_px: 1,
            data: PNG_1X1.to_vec(),
        })
        .expect("image");
    session
        .add_figure(&FigureRef {
            image_chunk_id: image_id,
            alt_text: "Placeholder hero pixel".into(),
            caption: Some("Fixture PNG standing in for a hero still.".into()),
            placement: ImagePlacement::Flow,
        })
        .expect("figure");
    session.add_slide(&title_body_slide(4, 5)).expect("slide2");
}

fn add_studio_assets_slide(session: &mut TesWriterSession) {
    session
        .add_text_chunk(&TextHeader::heading(1), "Assets and links")
        .expect("t3");
    let mut para = TextHeader::paragraph();
    // Body: "Read the docs or email the studio about the brief."
    para.spans = vec![
        InlineSpan {
            start: 9,
            end: 13,
            kind: InlineKind::Link { link_id: 0 },
        },
        InlineSpan {
            start: 17,
            end: 32,
            kind: InlineKind::Link { link_id: 1 },
        },
    ];
    session
        .add_text_chunk(&para, "Read the docs or email the studio about the brief.")
        .expect("links para");
    // Chunk ids: 1–2 text, 3 slide, 4–5 text, 6 image, 7 figure, 8 slide, 9–10 text.
    session
        .add_link(
            LinkEntry::external(10, 9, 13, "https://example.com/docs", LinkKind::Wiki)
                .expect("https"),
        )
        .expect("https link");
    session
        .add_link(
            LinkEntry::external(10, 17, 32, "mailto:studio@example.com", LinkKind::Wiki)
                .expect("mailto"),
        )
        .expect("mailto link");
    session
        .add_link(LinkEntry::new(
            10,
            0,
            4,
            Uuid::parse_str("aa0e8400-e29b-41d4-a716-446655440101").expect("uuid"),
            1,
            LinkKind::Wiki,
        ))
        .expect("internal");
    session
        .add_attachment_chunk(
            &AttachmentPayload::new(
                "application/pdf",
                "brief-appendix.pdf",
                b"%PDF-1.4 sample brief appendix".to_vec(),
                Some("Appendix PDF (inert fixture)".into()),
            )
            .expect("attachment"),
        )
        .expect("add attachment");
    session.add_slide(&title_body_slide(9, 10)).expect("slide3");
}

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

/// Write every sample under `dir` (typically `fixtures/samples`).
///
/// # Errors
///
/// Returns I/O errors from creating the directory or writing files.
pub fn write_all(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    let files = [
        ("text_roles.tes", encode_text_roles()),
        ("field_notes.tes", encode_field_notes()),
        ("studio_brief.tes", encode_studio_brief()),
        ("manuscript_chapters.tes", encode_manuscript_chapters()),
    ];
    for (name, bytes) in files {
        fs::write(dir.join(name), bytes)?;
    }
    Ok(())
}
