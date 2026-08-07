//! Figure title/caption band + pack weave alignment tour (`figure_align.tes`).
//!
//! Pair with packs under `fixtures/packs/figure_*` and:
//! `tes export --pdf --backend native --template-root fixtures/packs --template figure_left …`

use crate::catalog::{TesWriterSession, TextHeader};
use crate::layout::DocKind;

use super::common::{add_flow_figure, add_swatch_image, catalog};

/// Figure alignment / caption-band smoke document (`figure_align.tes`).
///
/// Three figures with a visible 240×120 swatch plus short / wrapping titles and
/// captions so pack knobs (`[figure].align`, caption `text_align` / `band`) are
/// visible in native PDF.
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_figure_align() -> Vec<u8> {
    let mut session = TesWriterSession::create("figure_align.tes", DocKind::Note);
    let mut cat = catalog(
        "aa0e8400-e29b-41d4-a716-446655440106",
        "Figure alignment tour",
        "2026-08-07T00:00:00Z",
        "2026-08-07T00:00:00Z",
        DocKind::Note,
        &["sample", "figure", "weave", "browse"],
    );
    cat.language = Some("en".into());
    session.set_catalog(cat).expect("catalog");

    session
        .add_text_chunk(&TextHeader::heading(1), "Figure alignment tour")
        .expect("h1");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Same .tes, different pack weave.toml. Export with \
             --template-root fixtures/packs --template figure_left|figure_center|figure_right|figure_caption_justify \
             and --pdf --backend native. Caption band always match_figure (wraps under the image); \
             left/center/right use text_align=follow; figure_caption_justify is the same geometry with justify. \
             max_width_factor=0.55 makes left/center/right obvious. Swatch is 240×120.",
        )
        .expect("intro");

    let image_id = add_swatch_image(&mut session).expect("image");

    let figures = [
        (
            "Short title and caption",
            "Alignment swatch (short labels)",
            Some("Short title"),
            Some("Short caption under the image."),
        ),
        (
            "Wrapping caption",
            "Alignment swatch (wrapping caption)",
            Some("Wrapping caption sample"),
            Some(
                "This caption is long enough to wrap inside the figure band so follow \
                 (left / center / right) versus justify is easy to compare — mid-lines \
                 stretch under justify while the last line stays flush left.",
            ),
        ),
        (
            "Long title, short caption",
            "Alignment swatch (long title)",
            Some(
                "A longer figure title that can wrap within the title band — watch title_text_align",
            ),
            Some("Tiny caption."),
        ),
    ];
    for (heading, alt, title, caption) in figures {
        session
            .add_text_chunk(&TextHeader::heading(2), heading)
            .expect("h2");
        add_flow_figure(&mut session, image_id, alt, title, caption).expect("figure");
    }

    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "End of tour. Non-figure chunk captions are unchanged by these packs \
             (weave [caption] knobs apply to Figure.caption only).",
        )
        .expect("outro");

    session.encode_file().expect("figure_align")
}
