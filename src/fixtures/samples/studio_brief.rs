//! Mixed media + deck regions (`studio_brief.tes`).

use uuid::Uuid;

use crate::catalog::{
    AttachmentPayload, FigureRef, ImagePayload, ImagePlacement, InlineKind, InlineSpan, LinkEntry,
    LinkKind, TesWriterSession, TextHeader,
};
use crate::layout::DocKind;

use super::super::v0::PNG_1X1;
use super::common::{catalog, title_body_slide};

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
            title: None,
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
