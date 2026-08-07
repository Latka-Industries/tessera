//! Umbrella Tessprek element tour for nvim / LSP (`tessprek_showcase.tes`).

use uuid::Uuid;

use crate::catalog::{
    AttachmentPayload, CitePayload, InlineKind, InlineSpan, LinkEntry, LinkKind, ListKind,
    TableData, TableRow, TesWriterSession, TextHeader, TextRole,
};
use crate::io::bib::BibEntry;
use crate::layout::DocKind;

use super::common::{add_flow_figure, add_swatch_image, catalog, cell, title_body_slide};

/// Umbrella Tessprek element tour for nvim / LSP (`tessprek_showcase.tes`).
///
/// Sealed **results** only — what native PDF / nvim should show after format.
/// Raw Tessprek macros live in `phrases_demo.tessprek` for format smoke.
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_tessprek_showcase() -> Vec<u8> {
    let mut session = TesWriterSession::create("tessprek_showcase.tes", DocKind::Document);
    let mut cat = catalog(
        "aa0e8400-e29b-41d4-a716-446655440105",
        "Tessprek showcase",
        "2026-08-05T00:00:00Z",
        "2026-08-05T00:00:00Z",
        DocKind::Document,
        &["sample", "showcase", "browse", "tessprek"],
    );
    cat.language = Some("en".into());
    cat.cite_style_id = Some("numeric".into());
    cat.template_id = Some("minimal".into());
    cat.theme_id = Some("draft".into());
    session.set_catalog(cat).expect("catalog");

    session
        .add_text_chunk(&TextHeader::heading(1), "Tessprek showcase")
        .expect("h1");
    let intro = session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "One sealed container for the usual Tessprek surfaces. Open in Neovim with tessera.nvim.",
        )
        .expect("intro");

    add_showcase_prose(&mut session);
    add_showcase_phrases(&mut session);
    add_showcase_fonts(&mut session);
    add_showcase_captioned(&mut session);
    let (slide_title, slide_body) = add_showcase_media_and_slide(&mut session);
    add_showcase_cite_family(&mut session, intro);
    add_showcase_links_and_attach(&mut session);

    session
        .add_slide(&title_body_slide(slide_title, slide_body))
        .expect("slide");
    session.encode_file().expect("tessprek_showcase")
}

/// Sealed result of pack phrase `yegourdoon` → italic body (what format emits).
fn add_showcase_phrases(session: &mut TesWriterSession) {
    session
        .add_text_chunk(&TextHeader::heading(2), "Pack phrases")
        .expect("h2 phrases");
    // minimal phrases.toml: yegourdoon = "*{arg}*" with arg "I am Yes"
    let body = "I am Yes";
    let mut para = TextHeader::paragraph();
    para.spans = vec![InlineSpan {
        start: 0,
        end: u32::try_from(body.len()).unwrap_or(0),
        kind: InlineKind::Emphasis,
    }];
    session.add_text_chunk(&para, body).expect("phrase result");
}

/// Sealed pack font pins — multi-script runs, not Tessprek source text.
fn add_showcase_fonts(session: &mut TesWriterSession) {
    session
        .add_text_chunk(&TextHeader::heading(2), "Pack fonts")
        .expect("h2 fonts");

    // Pack pins from minimal fonts.toml (real TTFs with those scripts).
    let segments: &[(&str, Option<&str>)] = &[
        ("Mixed scripts in one paragraph: ", None),
        ("բարև", Some("armenian")),
        (" · ", None),
        ("γεια", Some("greek")),
        (" · ", None),
        ("привет", Some("cyrillic")),
        (".", None),
    ];
    let mut body = String::new();
    let mut spans = Vec::new();
    for &(text, font_id) in segments {
        let start = u32::try_from(body.len()).unwrap_or(0);
        body.push_str(text);
        let end = u32::try_from(body.len()).unwrap_or(0);
        if let Some(font_id) = font_id {
            spans.push(InlineSpan {
                start,
                end,
                kind: InlineKind::Font {
                    font_id: font_id.into(),
                },
            });
        }
    }
    let mut para = TextHeader::paragraph();
    para.spans = spans;
    session.add_text_chunk(&para, &body).expect("font line");
}

fn add_showcase_prose(session: &mut TesWriterSession) {
    session
        .add_text_chunk(&TextHeader::heading(2), "Prose and spans")
        .expect("h2 prose");
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
    session
        .add_text_chunk(&TextHeader::heading(3), "Lists")
        .expect("h3");
    session
        .add_text_chunk(
            &TextHeader::list_item(ListKind::Bullet),
            "Bullet item (list_item chunk)",
        )
        .expect("b1");
    session
        .add_text_chunk(
            &TextHeader::list_item_at(ListKind::Bullet, 2),
            "Nested bullet",
        )
        .expect("b1n");
    session
        .add_text_chunk(
            &TextHeader::list_item(ListKind::Ordered),
            "Ordered first step",
        )
        .expect("o1");
    session
        .add_text_chunk(
            &TextHeader::with_role(TextRole::Blockquote),
            "Tessprek is a projection wire; packs own phrases and typography.",
        )
        .expect("quote role");
}

fn add_showcase_captioned(session: &mut TesWriterSession) {
    session
        .add_text_chunk(&TextHeader::heading(2), "Captioned blocks")
        .expect("h2 cap");
    let mut code = TextHeader::code_block(Some("rust"));
    code.title = Some("Listing".into());
    code.caption = Some("Tiny Rust sketch.".into());
    session
        .add_text_chunk(&code, "fn hello() {\n    println!(\"tessera\");\n}")
        .expect("code");
    let mut math = TextHeader::math();
    math.title = Some("Identity".into());
    math.caption = Some("Pythagoras.".into());
    session
        .add_text_chunk(&math, r"a^2 + b^2 = c^2")
        .expect("math");
    let mut table = TextHeader::table(TableData {
        rows: vec![
            TableRow {
                cells: vec![cell("Surface", true), cell("Tessprek", true)],
            },
            TableRow {
                cells: vec![cell("prose", false), cell("Markdown body", false)],
            },
            TableRow {
                cells: vec![
                    cell("figure", false),
                    cell("image + title + caption", false),
                ],
            },
            TableRow {
                cells: vec![
                    cell("cite family", false),
                    cell("citation, quote, ref", false),
                ],
            },
        ],
    });
    table.title = Some("Surfaces".into());
    table.caption = Some("What this showcase covers.".into());
    session.add_text_chunk(&table, "").expect("table");
}

fn add_showcase_media_and_slide(session: &mut TesWriterSession) -> (u64, u64) {
    session
        .add_text_chunk(&TextHeader::heading(2), "Media and slide")
        .expect("h2 media");
    let slide_title = session
        .add_text_chunk(&TextHeader::heading(1), "Slide title region")
        .expect("slide title");
    let slide_body = session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Slide body region — figure and attachment sit nearby in reading order.",
        )
        .expect("slide body");
    let image_id = add_swatch_image(session).expect("image");
    add_flow_figure(
        session,
        image_id,
        "Alignment swatch",
        Some("Fixture PNG"),
        Some("240×120 swatch standing in for a figure still."),
    )
    .expect("figure");
    (slide_title, slide_body)
}

fn add_showcase_cite_family(session: &mut TesWriterSession, quote_target: u64) {
    session
        .add_text_chunk(&TextHeader::heading(2), "Cite, quote, and ref")
        .expect("h2 cite");
    let bib_id = session
        .add_cite_chunk(&CitePayload {
            quote: String::new(),
            target_doc_id: None,
            target_chunk_id: None,
            target_byte_start: None,
            target_byte_end: None,
            label: Some("smith2024".into()),
            page: None,
            source: Some(BibEntry {
                cite_key: "smith2024".into(),
                entry_type: "article".into(),
                author: Some("Smith, Ada".into()),
                title: Some("Chunk Containers".into()),
                year: Some("2024".into()),
                ..BibEntry::default()
            }),
        })
        .expect("biblio cite");
    let body = "See smith2024 for the bibliography stub.";
    let key_start = u32::try_from(body.find("smith2024").expect("key")).unwrap_or(0);
    let key_end = key_start + u32::try_from("smith2024".len()).unwrap_or(0);
    let mut para = TextHeader::paragraph();
    para.spans = vec![InlineSpan {
        start: key_start,
        end: key_end,
        kind: InlineKind::Citation {
            cite_chunk_id: bib_id,
        },
    }];
    session.add_text_chunk(&para, body).expect("inline cite");
    session
        .add_cite_chunk(&CitePayload {
            quote: "One sealed container for the usual Tessprek surfaces.".into(),
            target_doc_id: Some("aa0e8400-e29b-41d4-a716-446655440105".into()),
            target_chunk_id: Some(quote_target),
            target_byte_start: Some(0),
            target_byte_end: Some(52),
            label: Some("intro-quote".into()),
            page: None,
            source: None,
        })
        .expect("quote");
    session
        .add_cite_chunk(&CitePayload {
            quote: String::new(),
            target_doc_id: Some("aa0e8400-e29b-41d4-a716-446655440105".into()),
            target_chunk_id: Some(quote_target),
            target_byte_start: None,
            target_byte_end: None,
            label: Some("see-intro".into()),
            page: None,
            source: None,
        })
        .expect("ref");
}

fn add_showcase_links_and_attach(session: &mut TesWriterSession) {
    session
        .add_text_chunk(&TextHeader::heading(2), "Links and attachment")
        .expect("h2 links");
    let mut para = TextHeader::paragraph();
    // Body: "Read the docs or email the studio."
    para.spans = vec![
        InlineSpan {
            start: 9,
            end: 13,
            kind: InlineKind::Link { link_id: 0 },
        },
        InlineSpan {
            start: 17,
            end: 33,
            kind: InlineKind::Link { link_id: 1 },
        },
    ];
    let link_chunk = session
        .add_text_chunk(&para, "Read the docs or email the studio.")
        .expect("links para");
    session
        .add_link(
            LinkEntry::external(
                link_chunk,
                9,
                13,
                "https://example.com/docs",
                LinkKind::Wiki,
            )
            .expect("https"),
        )
        .expect("https link");
    session
        .add_link(
            LinkEntry::external(
                link_chunk,
                17,
                33,
                "mailto:studio@example.com",
                LinkKind::Wiki,
            )
            .expect("mailto"),
        )
        .expect("mailto link");
    session
        .add_link(LinkEntry::new(
            link_chunk,
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
                "showcase-notes.pdf",
                b"%PDF-1.4 tessprek showcase".to_vec(),
                Some("Inert attachment fixture.".into()),
            )
            .expect("attachment"),
        )
        .expect("add attachment");
}
