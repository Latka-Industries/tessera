//! Unit tests for print IR mapping.

use std::path::PathBuf;

use ariadnes_weave::{BreakHint, PrintBlock, TextAlign};

use super::*;
use crate::catalog::DocumentCatalog;
use crate::catalog::chunk::{CitePayload, InlineKind, InlineSpan, TextHeader};
use crate::catalog::file::TesFile;
use crate::catalog::session::TesWriterSession;
use crate::fixtures::samples::{
    encode_article_bands, encode_article_columns, encode_manuscript_chapters, encode_mixed_align,
};
use crate::fixtures::v0::{encode_note_one_chunk, encode_note_three_chunks, encode_research_cite};
use crate::io::bib::BibEntry;
use crate::layout::DocKind;

fn open_bytes(name: &str, bytes: Vec<u8>) -> TesFile {
    TesFile::from_bytes(PathBuf::from(name), bytes).expect("open fixture bytes")
}

#[test]
fn note_one_chunk_paragraph_with_code_span() {
    let file = open_bytes("note_one_chunk.tes", encode_note_one_chunk());
    let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();
    assert_eq!(doc.profile.as_label(), "print@0");
    assert_eq!(doc.blocks.len(), 1);
    match &doc.blocks[0] {
        PrintBlock::Paragraph { runs, .. } => {
            assert!(
                runs.iter().any(|r| r.style.code),
                "expected code run: {runs:?}"
            );
            let joined: String = runs.iter().map(|r| r.text.as_str()).collect();
            assert!(joined.contains("tes textconv"));
        }
        other => panic!("expected paragraph, got {other:?}"),
    }
}

#[test]
fn font_span_sets_text_run_face() {
    let mut session = TesWriterSession::create("font.tes", crate::layout::DocKind::Note);
    let body = "hello barev world";
    let start = body.find("barev").unwrap() as u32;
    let end = start + "barev".len() as u32;
    let mut para = TextHeader::paragraph();
    para.spans = vec![InlineSpan {
        start,
        end,
        kind: InlineKind::Font {
            font_id: "armenian".into(),
        },
    }];
    session.add_text_chunk(&para, body).unwrap();
    let file = open_bytes("font.tes", session.encode_file().unwrap());
    let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();
    let PrintBlock::Paragraph { runs, .. } = &doc.blocks[0] else {
        panic!("expected paragraph");
    };
    assert!(
        runs.iter()
            .any(|r| r.text == "barev" && r.face.as_deref() == Some("armenian")),
        "{runs:?}"
    );
}

#[test]
fn note_three_chunks_heading_paragraph_list() {
    let file = open_bytes("note_three_chunks.tes", encode_note_three_chunks());
    let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();
    assert!(matches!(
        doc.blocks[0],
        PrintBlock::Heading { level: 1, .. }
    ));
    assert!(matches!(doc.blocks[1], PrintBlock::Paragraph { .. }));
    match &doc.blocks[2] {
        PrintBlock::List {
            ordered: false,
            items,
            ..
        } => {
            assert_eq!(items.len(), 3);
        }
        other => panic!("expected bullet list, got {other:?}"),
    }
}

#[test]
fn manuscript_chapters_profile_and_h1_breaks() {
    let file = open_bytes("manuscript_chapters.tes", encode_manuscript_chapters());
    let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();
    assert_eq!(doc.profile.as_label(), "manuscript@0");
    assert!(matches!(doc.blocks[0], PrintBlock::Paragraph { .. })); // front matter
    let h1s: Vec<_> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            PrintBlock::Heading {
                level: 1,
                break_before,
                runs,
                ..
            } => Some((
                runs.iter().map(|r| r.text.as_str()).collect::<String>(),
                *break_before,
            )),
            _ => None,
        })
        .collect();
    assert_eq!(h1s.len(), 3);
    assert!(h1s.iter().all(|(_, br)| *br == BreakHint::PageAlways));
    // THI-390: sealed `\toc` expands to title + TocEntry lines (section + dest).
    let toc_title = doc.blocks.iter().find_map(|b| match b {
        PrintBlock::Paragraph { runs, .. } => {
            let t: String = runs.iter().map(|r| r.text.as_str()).collect();
            (t == "Contents").then_some(t)
        }
        _ => None,
    });
    assert_eq!(toc_title.as_deref(), Some("Contents"));
    let toc_entries: Vec<_> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            PrintBlock::TocEntry {
                title,
                dest_id,
                page_label,
                ..
            } => Some((
                title.iter().map(|r| r.text.as_str()).collect::<String>(),
                dest_id.clone(),
                page_label.clone(),
            )),
            _ => None,
        })
        .collect();
    assert!(
        toc_entries.len() >= 3,
        "expected TOC TocEntry blocks, got: {toc_entries:?}"
    );
    assert!(
        toc_entries.iter().all(|(_, dest, page)| {
            dest.as_ref().is_some_and(|d| d.starts_with("h-")) && page.is_none()
        }),
        "expected dest_id h-* and page_label None (resolve): {toc_entries:?}"
    );
    assert!(
        toc_entries
            .iter()
            .any(|(t, _, _)| t.starts_with('1') && t.contains("Chapter")),
        "expected section-numbered chapter entry: {toc_entries:?}"
    );
    assert!(
        !doc.blocks
            .iter()
            .any(|b| matches!(b, PrintBlock::List { .. })),
        "TOC should not expand to a bullet List"
    );
}

#[test]
fn article_columns_maps_print_block_columns() {
    let file = open_bytes("article_columns.tes", encode_article_columns());
    let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();
    let cols: Vec<_> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            PrintBlock::Columns {
                count,
                gap,
                children,
                ..
            } => Some((*count, *gap, children.len())),
            _ => None,
        })
        .collect();
    assert_eq!(
        cols.len(),
        2,
        "expected 2-col then 3-col regions, got: {cols:?}"
    );
    assert_eq!(cols[0].0, 2);
    assert_eq!(cols[0].1, Some(16));
    assert!(
        cols[0].2 >= 5,
        "2-col region should carry several children, got {}",
        cols[0].2
    );
    assert_eq!(cols[1].0, 3);
    assert_eq!(cols[1].1, Some(12));
    assert!(
        cols[1].2 >= 4,
        "3-col region should carry several paragraphs, got {}",
        cols[1].2
    );
    assert!(
        doc.blocks
            .iter()
            .any(|b| matches!(b, PrintBlock::Heading { level: 1, .. })),
        "title heading should stay outside Columns"
    );
}

#[test]
fn mixed_align_maps_print_block_text_align() {
    let file = open_bytes("mixed_align.tes", encode_mixed_align());
    let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();

    let lead = doc.blocks.iter().find_map(|b| match b {
        PrintBlock::Paragraph {
            runs, text_align, ..
        } if runs.iter().any(|r| r.text.contains("Lead stays flush")) => Some(*text_align),
        _ => None,
    });
    assert_eq!(lead, Some(Some(TextAlign::Left)));

    let cols = doc.blocks.iter().find_map(|b| match b {
        PrintBlock::Columns {
            count,
            text_align,
            children,
            ..
        } => Some((*count, *text_align, children.len())),
        _ => None,
    });
    assert_eq!(
        cols.map(|(n, align, _)| (n, align)),
        Some((2, Some(TextAlign::Justify)))
    );
    let children = match &doc
        .blocks
        .iter()
        .find(|b| matches!(b, PrintBlock::Columns { .. }))
    {
        Some(PrintBlock::Columns { children, .. }) => children,
        _ => panic!("expected Columns region"),
    };
    assert!(
        children.iter().all(|c| match c {
            PrintBlock::Paragraph { text_align, .. } => text_align.is_none(),
            _ => true,
        }),
        "column children should omit text_align and inherit the region: {children:?}"
    );

    let center = doc.blocks.iter().find_map(|b| match b {
        PrintBlock::Paragraph {
            runs, text_align, ..
        } if runs.iter().any(|r| r.text.contains("centered aside")) => Some(*text_align),
        _ => None,
    });
    assert_eq!(center, Some(Some(TextAlign::Center)));

    let close = doc.blocks.iter().find_map(|b| match b {
        PrintBlock::Paragraph {
            runs, text_align, ..
        } if runs.iter().any(|r| r.text.contains("Closing full-measure")) => Some(*text_align),
        _ => None,
    });
    assert_eq!(close, Some(Some(TextAlign::Left)));
}

#[test]
fn chapter_scope_excludes_siblings() {
    let file = open_bytes("manuscript_chapters.tes", encode_manuscript_chapters());
    let doc = build_print_document(
        &file,
        &PrintBuildOptions {
            chapter: Some(2),
            ..Default::default()
        },
    )
    .unwrap();
    let titles: Vec<String> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            PrintBlock::Heading { level: 1, runs, .. } => {
                Some(runs.iter().map(|r| r.text.as_str()).collect())
            }
            _ => None,
        })
        .collect();
    assert_eq!(titles.len(), 1);
    assert!(titles[0].contains("Two") || titles[0].contains("2") || !titles[0].is_empty());
    // No front-matter paragraph before the chapter H1.
    assert!(matches!(
        doc.blocks[0],
        PrintBlock::Heading { level: 1, .. }
    ));
}

#[test]
fn on_disk_note_one_chunk_fixture() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/v0/note_one_chunk.tes");
    let doc = build_print_document_from_path(&path, &PrintBuildOptions::default()).unwrap();
    assert_eq!(doc.blocks.len(), 1);
    assert!(matches!(doc.blocks[0], PrintBlock::Paragraph { .. }));
}

#[test]
fn research_cite_quote_maps_to_print_quote() {
    let file = open_bytes("research_cite.tes", encode_research_cite());
    let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();
    assert!(
        doc.blocks
            .iter()
            .any(|b| matches!(b, PrintBlock::Paragraph { .. })),
        "expected prose paragraph: {doc:?}"
    );
    match doc
        .blocks
        .iter()
        .find(|b| matches!(b, PrintBlock::Quote { .. }))
    {
        Some(PrintBlock::Quote { runs, .. }) => {
            let text: String = runs.iter().map(|r| r.text.as_str()).collect();
            assert!(text.contains("We measured"), "{text}");
        }
        other => panic!("expected Quote block for ranged cite, got {other:?}"),
    }
    assert!(
        !doc.blocks.iter().any(|b| matches!(
            b,
            PrintBlock::Heading {
                level: 2,
                runs,
                ..
            } if runs.iter().any(|r| r.text == "References")
        )),
        "quote-only fixture should not emit References"
    );
}

#[test]
fn cite_quote_ref_biblio_and_inline_markers() {
    let mut catalog = DocumentCatalog::new(
        "990e8400-e29b-41d4-a716-446655440099",
        "Print cite specimen",
        "2026-08-05T00:00:00Z",
        "2026-08-05T00:00:00Z",
        DocKind::Research,
    );
    catalog.cite_style_id = Some("numeric".into());
    let mut session = TesWriterSession::create("print_cites.tes", DocKind::Research);
    session.set_catalog(catalog).unwrap();

    let bib_id = session
        .add_cite_chunk(&CitePayload {
            quote: String::new(),
            target_doc_id: None,
            target_chunk_id: None,
            target_byte_start: None,
            target_byte_end: None,
            label: Some("keller2020".into()),
            page: None,
            source: Some(BibEntry {
                cite_key: "keller2020".into(),
                entry_type: "article".into(),
                author: Some("Keller, Ada".into()),
                title: Some("Chunk Containers".into()),
                year: Some("2020".into()),
                ..BibEntry::default()
            }),
        })
        .unwrap();

    let body = "See keller2020 for context.";
    let key_start = body.find("keller2020").unwrap() as u32;
    let key_end = key_start + "keller2020".len() as u32;
    let mut para = TextHeader::paragraph();
    para.spans = vec![InlineSpan {
        start: key_start,
        end: key_end,
        kind: InlineKind::Citation {
            cite_chunk_id: bib_id,
        },
    }];
    session.add_text_chunk(&para, body).unwrap();

    session
        .add_cite_chunk(&CitePayload {
            quote: "Quoted passage.".into(),
            target_doc_id: Some("aa0e8400-e29b-41d4-a716-446655440001".into()),
            target_chunk_id: Some(1),
            target_byte_start: Some(0),
            target_byte_end: Some(15),
            label: Some("keller2020".into()),
            page: Some(2),
            source: None,
        })
        .unwrap();

    session
        .add_cite_chunk(&CitePayload {
            quote: String::new(),
            target_doc_id: Some("aa0e8400-e29b-41d4-a716-446655440001".into()),
            target_chunk_id: Some(3),
            target_byte_start: None,
            target_byte_end: None,
            label: Some("see-also".into()),
            page: None,
            source: None,
        })
        .unwrap();

    let bytes = session.encode_file().unwrap();
    let file = open_bytes("print_cites.tes", bytes);
    let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();

    // Biblio stub: "[1] keller2020"
    let stub = doc.blocks.iter().find_map(|b| match b {
        PrintBlock::Paragraph { runs, .. }
            if runs.iter().any(|r| r.style.cite && r.text == "[1]") =>
        {
            Some(runs.iter().map(|r| r.text.as_str()).collect::<String>())
        }
        _ => None,
    });
    assert_eq!(stub.as_deref(), Some("[1] keller2020"));

    // Inline rewrite in prose
    let prose = doc.blocks.iter().find_map(|b| match b {
        PrintBlock::Paragraph { runs, .. }
            if runs.iter().any(|r| r.text.starts_with("See "))
                && runs.iter().any(|r| r.style.cite && r.text == "[1]") =>
        {
            Some(runs.iter().map(|r| r.text.as_str()).collect::<String>())
        }
        _ => None,
    });
    assert_eq!(prose.as_deref(), Some("See [1] for context."));

    assert!(
        doc.blocks.iter().any(|b| matches!(
            b,
            PrintBlock::Quote { runs, .. } if runs.iter().any(|r| r.text.contains("Quoted passage"))
        )),
        "expected quote block: {doc:?}"
    );
    assert!(
        doc.blocks.iter().any(|b| matches!(
            b,
            PrintBlock::Paragraph { runs, .. }
                if runs.iter().any(|r| r.text.contains("[ref:") && r.text.contains("see-also"))
        )),
        "expected ref paragraph: {doc:?}"
    );
    assert!(
        doc.blocks.iter().any(|b| matches!(
            b,
            PrintBlock::Heading {
                level: 2,
                runs,
                ..
            } if runs.iter().any(|r| r.text == "References")
        )),
        "expected References heading: {doc:?}"
    );
    assert!(
        doc.blocks.iter().any(|b| matches!(
            b,
            PrintBlock::Paragraph { runs, .. }
                if runs.iter().any(|r| r.text.contains("1. Keller") && r.text.contains("Chunk Containers"))
        )),
        "expected bibliography line: {doc:?}"
    );
}

#[test]
fn layout_place_frac_flush_maps_to_weave() {
    use crate::catalog::layout::{
        LayoutOp as TesLayoutOp, LayoutPayload, MeasureFrac as TesFrac, PlaceSkip as TesSkip,
        RuleWidth, VspaceAmount as TesVspace,
    };
    use ariadnes_weave::{LayoutOp, MeasureFrac, PlaceSkip, VspaceAmount};

    let mut session = TesWriterSession::create("layout_flush.tes", DocKind::Note);
    session
        .add_text_chunk(&TextHeader::paragraph(), "Before layout chunk.")
        .unwrap();
    session
        .add_layout(&LayoutPayload {
            ops: vec![
                TesLayoutOp::Place {
                    skip: TesSkip::Frac {
                        frac: TesFrac::FULL,
                    },
                    content: "▸".into(),
                    spans: vec![],
                },
                TesLayoutOp::Vspace {
                    amount: TesVspace::Med,
                },
                TesLayoutOp::Rule {
                    width: RuleWidth::frac(TesFrac::FULL),
                },
            ],
        })
        .unwrap();
    session
        .add_text_chunk(&TextHeader::paragraph(), "After layout chunk.")
        .unwrap();
    let file = open_bytes("layout_flush.tes", session.encode_file().unwrap());
    let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();
    assert_eq!(doc.blocks.len(), 3);
    match &doc.blocks[1] {
        PrintBlock::Layout { ops } => {
            assert_eq!(ops.len(), 3);
            assert!(matches!(
                &ops[0],
                LayoutOp::Place {
                    skip: PlaceSkip::Frac { frac },
                    runs,
                } if *frac == MeasureFrac::FULL && runs.iter().any(|r| r.text == "▸")
            ));
            assert!(matches!(
                &ops[1],
                LayoutOp::Vspace {
                    amount: VspaceAmount::Med
                }
            ));
            assert!(matches!(&ops[2], LayoutOp::Rule { .. }));
        }
        other => panic!("expected Layout block, got {other:?}"),
    }
}

#[test]
fn chunk_title_and_caption_use_label_styles() {
    let mut session = TesWriterSession::create("caption.tes", DocKind::Note);
    let mut math = TextHeader::math();
    math.title = Some("Eq. label".into());
    math.caption = Some("A short caption.".into());
    session.add_text_chunk(&math, "E = mc^2").unwrap();
    let file = open_bytes("caption.tes", session.encode_file().unwrap());
    let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();
    assert_eq!(doc.blocks.len(), 3);
    match &doc.blocks[0] {
        PrintBlock::Paragraph { runs, .. } => {
            assert_eq!(runs.len(), 1);
            assert!(runs[0].style.strong, "{runs:?}");
            assert_eq!(runs[0].text, "Eq. label");
        }
        other => panic!("expected title paragraph, got {other:?}"),
    }
    assert!(matches!(doc.blocks[1], PrintBlock::Math { .. }));
    match &doc.blocks[2] {
        PrintBlock::Paragraph { runs, .. } => {
            assert_eq!(runs.len(), 1);
            assert!(runs[0].style.emphasis, "{runs:?}");
            assert_eq!(runs[0].text, "A short caption.");
        }
        other => panic!("expected caption paragraph, got {other:?}"),
    }
}

#[test]
fn inline_quote_and_math_map_emphasis_and_code() {
    let body = "say hello and x^2";
    let quote_start = body.find("hello").unwrap() as u32;
    let quote_end = quote_start + "hello".len() as u32;
    let math_start = body.find("x^2").unwrap() as u32;
    let math_end = math_start + "x^2".len() as u32;
    let mut para = TextHeader::paragraph();
    para.spans = vec![
        InlineSpan {
            start: quote_start,
            end: quote_end,
            kind: InlineKind::Quote,
        },
        InlineSpan {
            start: math_start,
            end: math_end,
            kind: InlineKind::Math { tex: "x^2".into() },
        },
    ];
    let mut session = TesWriterSession::create("inline_gaps.tes", DocKind::Note);
    session.add_text_chunk(&para, body).unwrap();
    let file = open_bytes("inline_gaps.tes", session.encode_file().unwrap());
    let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();
    let PrintBlock::Paragraph { runs, .. } = &doc.blocks[0] else {
        panic!("expected paragraph");
    };
    assert!(
        runs.iter()
            .any(|r| r.text == "hello" && r.style.emphasis && !r.style.code),
        "{runs:?}"
    );
    assert!(
        runs.iter().any(|r| r.text == "x^2" && r.style.code),
        "{runs:?}"
    );
}

#[test]
fn inline_underline_maps_style_underline() {
    let body = "under here";
    let start = body.find("under").unwrap() as u32;
    let end = start + "under".len() as u32;
    let mut para = TextHeader::paragraph();
    para.spans = vec![InlineSpan {
        start,
        end,
        kind: InlineKind::Underline,
    }];
    let mut session = TesWriterSession::create("underline.tes", DocKind::Note);
    session.add_text_chunk(&para, body).unwrap();
    let file = open_bytes("underline.tes", session.encode_file().unwrap());
    let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();
    let PrintBlock::Paragraph { runs, .. } = &doc.blocks[0] else {
        panic!("expected paragraph");
    };
    let under = runs.iter().find(|r| r.text == "under").expect("under run");
    assert!(
        under.style.underline && !under.style.cite,
        "underline must set InlineStyle.underline: {under:?}"
    );
}

#[test]
fn figure_title_and_caption_use_figure_fields() {
    use crate::catalog::media::{FigureRef, ImagePayload, ImagePlacement};
    use crate::fixtures::v0::PNG_1X1;

    let mut session = TesWriterSession::create("fig_title.tes", DocKind::Note);
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
            alt_text: "alt".into(),
            title: Some("Hero".into()),
            caption: Some("A still".into()),
            placement: ImagePlacement::Flow,
        })
        .expect("figure");
    let file = open_bytes("fig_title.tes", session.encode_file().unwrap());
    let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();
    match &doc.blocks[0] {
        PrintBlock::Figure {
            title,
            caption,
            dest_id,
            ..
        } => {
            assert_eq!(title.len(), 1);
            assert_eq!(title[0].text, "Hero");
            assert!(
                !title[0].style.strong,
                "title runs are plain; weave styles band"
            );
            assert_eq!(caption.len(), 1);
            assert_eq!(caption[0].text, "A still");
            assert!(
                !caption[0].style.emphasis,
                "caption runs are plain; weave [caption] knobs own italic: {caption:?}"
            );
            assert!(
                dest_id.as_ref().is_some_and(|d| d.starts_with("f-")),
                "expected figure dest_id f-*: {dest_id:?}"
            );
        }
        other => panic!("expected Figure, got {other:?}"),
    }
}

#[test]
fn lof_and_lot_expand_to_toc_entries() {
    use crate::catalog::chunk::{TableCell, TableData, TableRow};
    use crate::catalog::media::{FigureRef, ImagePayload, ImagePlacement};
    use crate::fixtures::v0::PNG_1X1;

    let mut session = TesWriterSession::create("floats.tes", DocKind::Note);
    session
        .add_text_chunk(&TextHeader::lof_titled("Figures"), "")
        .expect("lof");
    session
        .add_text_chunk(&TextHeader::lot_titled("Tables"), "")
        .expect("lot");
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
            alt_text: "alt".into(),
            title: Some("Hero".into()),
            caption: Some("A still".into()),
            placement: ImagePlacement::Flow,
        })
        .expect("figure");
    let mut table = TextHeader::table(TableData {
        rows: vec![TableRow {
            cells: vec![TableCell {
                text: "A".into(),
                spans: Vec::new(),
                align: None,
                is_header: true,
                rowspan: None,
                colspan: None,
            }],
        }],
    });
    table.title = Some("Grid".into());
    table.caption = Some("ignored by default".into());
    session.add_text_chunk(&table, "").expect("table");

    let file = open_bytes("floats.tes", session.encode_file().unwrap());
    let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();

    let entries: Vec<_> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            PrintBlock::TocEntry {
                title,
                dest_id,
                page_label,
                ..
            } => Some((
                title.iter().map(|r| r.text.as_str()).collect::<String>(),
                dest_id.clone(),
                page_label.clone(),
            )),
            _ => None,
        })
        .collect();
    assert!(
        entries.iter().any(|(t, d, p)| {
            t.starts_with("Figure 1. Hero")
                && d.as_ref().is_some_and(|id| id.starts_with("f-"))
                && p.is_none()
        }),
        "expected LOF TocEntry from title: {entries:?}"
    );
    assert!(
        entries.iter().any(|(t, d, p)| {
            t.starts_with("Table 1. Grid")
                && d.as_ref().is_some_and(|id| id.starts_with("t-"))
                && p.is_none()
        }),
        "expected LOT TocEntry from title: {entries:?}"
    );
}

#[test]
fn row_pane_icon_macro_becomes_font_run() {
    use crate::catalog::chunk::TableCell;
    use crate::catalog::icon_by_name;

    let icon = icon_by_name("github").expect("github icon");
    let mut session = TesWriterSession::create("row_icon.tes", DocKind::Note);
    session
        .add_text_chunk(
            &TextHeader::row(vec![
                TableCell {
                    text: "UBLX \\icon{github}".into(),
                    spans: Vec::new(),
                    align: None,
                    is_header: false,
                    rowspan: None,
                    colspan: None,
                },
                TableCell {
                    text: "right".into(),
                    spans: Vec::new(),
                    align: None,
                    is_header: false,
                    rowspan: None,
                    colspan: None,
                },
            ]),
            "",
        )
        .expect("row");
    let file = open_bytes("row_icon.tes", session.encode_file().unwrap());
    let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();
    match &doc.blocks[0] {
        PrintBlock::Row { panes, .. } => {
            let joined: String = panes[0].iter().map(|r| r.text.as_str()).collect();
            assert!(
                !joined.contains("\\icon{"),
                "row pane must expand \\icon, got {panes:?}"
            );
            assert!(
                panes[0]
                    .iter()
                    .any(|r| r.face.as_deref() == Some(icon.face)
                        && r.text == icon.glyph.to_string()),
                "expected fab/fas glyph run, got {panes:?}"
            );
        }
        other => panic!("expected Row, got {other:?}"),
    }
}

#[test]
fn footnote_span_maps_to_weave_note() {
    use crate::catalog::chunk::{NOTE_MARKER, NoteKind};
    let catalog = DocumentCatalog::new(
        "00000000-0000-0000-0000-000000000396",
        "Footnote specimen",
        "2026-01-01T00:00:00Z",
        "2026-01-01T00:00:00Z",
        DocKind::Note,
    );
    let mut session = TesWriterSession::create("print_notes.tes", DocKind::Note);
    session.set_catalog(catalog).unwrap();
    let marker_start = u32::try_from("See ".len()).unwrap();
    let body = format!("See {NOTE_MARKER} now.");
    let marker_end = marker_start + u32::try_from(NOTE_MARKER.len()).unwrap();
    let mut header = TextHeader::paragraph();
    header.spans.push(InlineSpan {
        start: marker_start,
        end: marker_end,
        kind: InlineKind::Note {
            kind: NoteKind::Footnote,
            body: "A clarification.".into(),
        },
    });
    session.add_text_chunk(&header, &body).unwrap();
    let file = open_bytes("print_notes.tes", session.encode_file().unwrap());
    let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();
    assert!(
        doc.blocks.iter().any(|b| matches!(
            b,
            PrintBlock::Paragraph { runs, .. }
                if runs.iter().any(|r| r.note_id.is_some())
        )),
        "expected a run with note_id, got {:?}",
        doc.blocks
    );
    assert!(
        doc.blocks
            .iter()
            .any(|b| matches!(b, PrintBlock::Note { .. })),
        "expected PrintBlock::Note def, got {:?}",
        doc.blocks
    );
}

#[test]
fn theorem_and_callout_map_to_same_print_callout() {
    let mut session = TesWriterSession::create("bands.tes", crate::layout::DocKind::Document);
    session
        .add_text_chunk(
            &TextHeader::callout("definition", Some("Trace minimale".into())),
            "Une trace minimale est une trace.",
        )
        .unwrap();
    session
        .add_text_chunk(
            &TextHeader::callout("note", Some("Note".into())),
            "A rho-shaped aside.",
        )
        .unwrap();
    let file = open_bytes("bands.tes", session.encode_file().unwrap());
    let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();
    let kinds: Vec<_> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            PrintBlock::Callout {
                callout_kind,
                title,
                body,
            } => {
                let title_joined: String = title.iter().map(|r| r.text.as_str()).collect();
                let body_joined: String = body.iter().map(|r| r.text.as_str()).collect();
                Some((callout_kind.as_str(), title_joined, body_joined))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds.len(),
        2,
        "expected two Callout blocks, got {:?}",
        doc.blocks
    );
    assert_eq!(kinds[0].0, "definition");
    assert!(
        kinds[0].1.contains("Definition (Trace minimale)"),
        "{}",
        kinds[0].1
    );
    assert!(kinds[0].2.contains("trace minimale"));
    assert_eq!(kinds[1].0, "note");
    assert_eq!(kinds[1].1, "Note");
}

#[test]
fn article_bands_maps_callouts_and_columns() {
    let file = open_bytes("article_bands.tes", encode_article_bands());
    let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();
    assert!(
        doc.blocks.iter().any(|b| matches!(
            b,
            PrintBlock::Callout { callout_kind, .. } if callout_kind.as_str() == "definition"
        )),
        "expected definition callout"
    );
    assert!(
        doc.blocks
            .iter()
            .any(|b| matches!(b, PrintBlock::Columns { count: 2, .. })),
        "expected a 2-column region"
    );
}
