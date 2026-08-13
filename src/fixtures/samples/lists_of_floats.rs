//! List of figures / list of tables smoke (`lists_of_floats.tes`; THI-395).

use crate::catalog::{TableData, TableRow, TesWriterSession, TextHeader};
use crate::layout::DocKind;

use super::common::{add_flow_figure, add_swatch_image, catalog, cell};

/// Document with `\lof` / `\lot` before titled figures and tables.
///
/// Default `source=title`: caption-only floats are omitted. Pair with
/// `page_chrome` so page digits resolve on the list lines.
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_lists_of_floats() -> Vec<u8> {
    let mut session = TesWriterSession::create("lists_of_floats.tes", DocKind::Document);
    let mut cat = catalog(
        "cc0e8400-e29b-41d4-a716-446655440395",
        "Lists of floats",
        "2026-08-12T00:00:00Z",
        "2026-08-12T00:00:00Z",
        DocKind::Document,
        &["sample", "lof", "lot", "print"],
    );
    cat.language = Some("en".into());
    session.set_catalog(cat).expect("catalog");

    session
        .add_text_chunk(&TextHeader::heading(1), "Lists of floats")
        .expect("h1");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Front matter lists expand from later float titles (Tessprek \\lof / \\lot, \
             default source=title). Untitled floats are omitted. Export with page_chrome \
             to see page digits.",
        )
        .expect("intro");

    session
        .add_text_chunk(&TextHeader::lof_titled("List of Figures"), "")
        .expect("lof");
    session
        .add_text_chunk(&TextHeader::lot_titled("List of Tables"), "")
        .expect("lot");

    session
        .add_text_chunk(&TextHeader::heading(1), "Figures")
        .expect("h1 figures");
    let image_id = add_swatch_image(&mut session).expect("image");
    add_flow_figure(
        &mut session,
        image_id,
        "Harbor swatch",
        Some("Harbor morning"),
        Some("Short caption under the first figure."),
    )
    .expect("figure 1");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Body between floats so the list page numbers can differ.",
        )
        .expect("between");
    add_flow_figure(
        &mut session,
        image_id,
        "Second swatch",
        Some("Second harbor view"),
        Some("Caption is ignored by default LOF."),
    )
    .expect("figure 2");
    add_flow_figure(
        &mut session,
        image_id,
        "No title swatch",
        None,
        Some("Caption-only — omitted unless source=caption."),
    )
    .expect("figure caption-only");

    session
        .add_text_chunk(&TextHeader::heading(1), "Tables")
        .expect("h1 tables");
    let mut table_a = TextHeader::table(TableData {
        rows: vec![
            TableRow {
                cells: vec![cell("Site", true), cell("Count", true)],
            },
            TableRow {
                cells: vec![cell("Dock", false), cell("12", false)],
            },
        ],
    });
    table_a.title = Some("Site counts".into());
    table_a.caption = Some("First table caption (ignored by default LOT).".into());
    session.add_text_chunk(&table_a, "").expect("table 1");

    let mut table_b = TextHeader::table(TableData {
        rows: vec![
            TableRow {
                cells: vec![cell("Metric", true), cell("Value", true)],
            },
            TableRow {
                cells: vec![cell("Depth", false), cell("3 m", false)],
            },
        ],
    });
    table_b.title = Some("Depth readings".into());
    table_b.caption = Some("Second table caption.".into());
    session.add_text_chunk(&table_b, "").expect("table 2");

    let mut table_c = TextHeader::table(TableData {
        rows: vec![TableRow {
            cells: vec![cell("X", true)],
        }],
    });
    table_c.caption = Some("Caption-only table — omitted unless source=caption.".into());
    session.add_text_chunk(&table_c, "").expect("table caption-only");

    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "End. Untitled floats are omitted from LOF/LOT (default source=title).",
        )
        .expect("outro");

    session.encode_file().expect("lists_of_floats")
}
