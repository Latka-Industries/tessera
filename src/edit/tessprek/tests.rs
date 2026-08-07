use std::collections::BTreeMap;

use super::*;
use crate::catalog::TesFile;
use crate::catalog::chunk::{CitePayload, TextHeader, TextRole};
use crate::catalog::media::{FigureRef, ImagePlacement};
use crate::catalog::slide::{SlidePayload, SlideRegion};
use crate::catalog::{DocumentCatalog, TesWriterSession};
use crate::edit::ContentBlock;
use crate::error::TesError;
use crate::layout::DocKind;
use tempfile::tempdir;

#[test]
fn round_trip_text_classes() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("note.tes");
    let mut session = TesWriterSession::create(&path, DocKind::Note);
    session
        .set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440000",
            "Demo",
            "2026-07-27T00:00:00Z",
            "2026-07-27T00:00:00Z",
            DocKind::Note,
        ))
        .unwrap();
    let mut header = TextHeader::heading(1);
    header.classes = vec!["lead".into()];
    session.add_text_chunk(&header, "Hello").unwrap();
    session
        .add_text_chunk(&TextHeader::paragraph(), "Body")
        .unwrap();
    session.commit().unwrap();

    let file = TesFile::open(&path).unwrap();
    let text = encode_tessprek(&file, "abc").unwrap();
    assert!(text.contains("format=tessprek"), "{text}");
    assert!(text.contains("version=2"), "{text}");
    assert!(text.contains("source-hash=abc"), "{text}");
    assert!(
        text.contains("doc_id=550e8400-e29b-41d4-a716-446655440000"),
        "{text}"
    );
    assert!(
        text.contains("title=Demo") || text.contains("title=\"Demo\""),
        "{text}"
    );
    assert!(text.contains("class=\"lead\""), "{text}");
    assert!(text.contains("\\block{"), "{text}");
    assert!(text.contains("# Hello"), "{text}");
    let blocks = decode_tessprek(&text).unwrap();
    assert_eq!(blocks.len(), 2);
    match &blocks[0] {
        ContentBlock::Text { header, body, .. } => {
            assert_eq!(header.role, TextRole::Heading);
            assert_eq!(header.classes, vec!["lead"]);
            assert_eq!(body, "Hello");
        }
        _ => panic!("expected text"),
    }
}

#[test]
fn round_trip_figure_cite_slide_attachment() {
    let blocks = vec![
        ContentBlock::Text {
            chunk_id: Some(1),
            header: TextHeader::heading(1),
            body: "Doc".into(),
            pending_links: Vec::new(),
            pending_cites: Vec::new(),
            pending_fonts: Vec::new(),
        },
        ContentBlock::Figure {
            chunk_id: Some(2),
            figure: FigureRef {
                image_chunk_id: 3,
                alt_text: "A photo".into(),
                title: None,
                caption: Some("Cap".into()),
                placement: ImagePlacement::Flow,
            },
        },
        ContentBlock::Cite {
            chunk_id: Some(4),
            cite: CitePayload {
                quote: "Some quoted text".into(),
                target_doc_id: None,
                target_chunk_id: Some(1),
                target_byte_start: Some(0),
                target_byte_end: Some(4),
                label: Some("Smith2024".into()),
                page: None,
                source: None,
            },
        },
        ContentBlock::Slide {
            chunk_id: Some(5),
            slide: SlidePayload {
                layout_id: "title".into(),
                regions: vec![SlideRegion {
                    name: "body".into(),
                    chunk_id: 1,
                }],
            },
        },
        ContentBlock::Attachment {
            chunk_id: Some(6),
            filename: "notes.pdf".into(),
            media_type: "application/pdf".into(),
            caption: Some("Handout".into()),
            sha256: "deadbeef".into(),
        },
    ];
    let text = encode_content_blocks(&TessprekDocMeta::default(), &blocks, &[], &[]);
    assert!(text.contains("\\ids{1,2,4,5,6}"), "{text}");
    assert!(text.contains("id=3"), "{text}");
    assert!(text.contains("\\media{\n"), "{text}");
    assert!(text.contains("\\figure{"), "{text}");
    assert!(text.contains("\\quote{"), "{text}");
    assert!(text.contains("target_byte_start=0"), "{text}");
    assert!(text.contains("target_byte_end=4"), "{text}");
    assert!(text.contains("quote=\"Some quoted text\""), "{text}");
    assert!(!text.contains("> Some quoted text"), "{text}");
    assert!(text.contains("\\slide{"), "{text}");
    assert!(text.contains("\\attach{"), "{text}");
    let decoded = decode_tessprek(&text).unwrap();
    assert_eq!(decoded, blocks);
}

#[test]
fn layout_place_vspace_rule_round_trip() {
    use crate::catalog::layout::{
        LayoutOp, LayoutPayload, MeasureFrac, PlaceSkip, RuleWidth, VspaceAmount,
    };

    let blocks = vec![
        ContentBlock::Text {
            chunk_id: Some(1),
            header: TextHeader::paragraph(),
            body: "Before".into(),
            pending_links: Vec::new(),
            pending_cites: Vec::new(),
            pending_fonts: Vec::new(),
        },
        ContentBlock::Layout {
            chunk_id: Some(2),
            layout: LayoutPayload {
                ops: vec![
                    LayoutOp::Place {
                        skip: PlaceSkip::Frac {
                            frac: MeasureFrac::FULL,
                        },
                        content: "▸".into(),
                        spans: vec![],
                    },
                    LayoutOp::Vspace {
                        amount: VspaceAmount::Med,
                    },
                    LayoutOp::Rule {
                        width: RuleWidth::frac(MeasureFrac::FULL),
                    },
                ],
            },
        },
        ContentBlock::Text {
            chunk_id: Some(3),
            header: TextHeader::paragraph(),
            body: "After".into(),
            pending_links: Vec::new(),
            pending_cites: Vec::new(),
            pending_fonts: Vec::new(),
        },
    ];
    let text = encode_content_blocks(&TessprekDocMeta::default(), &blocks, &[], &[]);
    assert!(text.contains("\\layout{"), "{text}");
    assert!(text.contains("place frac=1"), "{text}");
    assert!(text.contains("content=\"▸\""), "{text}");
    assert!(text.contains("vspace=med"), "{text}");
    assert!(text.contains("rule frac=1"), "{text}");
    let decoded = decode_tessprek(&text).unwrap();
    assert_eq!(decoded, blocks);
}

#[test]
fn layout_rejects_unknown_op_in_tessprek() {
    let text = "\\tessera{format=tessprek version=2}\n\\ids{1}\n\n\\layout{\n  gauge value=1\n}\n";
    let err = decode_tessprek(text).unwrap_err();
    assert!(
        matches!(err, crate::error::TesError::EditParse { .. }),
        "{err}"
    );
}

#[test]
fn decode_skips_rich_media_header() {
    let text = "\
\\tessera{format=tessprek version=2}\n\
\\ids{1,2}\n\
\\media{\n\
  id=9\n\
  media_type=image/png\n\
  sha256=deadbeef\n\
  width=1\n\
  height=1\n\
}\n\
\n\
# Title\n\
\n\
\\figure{\n\
  image=9\n\
  placement=flow\n\
  alt=\"alt\"\n\
}\n\
";
    let blocks = decode_tessprek(text).unwrap();
    assert_eq!(blocks.len(), 2);
    match &blocks[1] {
        ContentBlock::Figure { figure, .. } => {
            assert_eq!(figure.image_chunk_id, 9);
            assert_eq!(figure.alt_text, "alt");
        }
        other => panic!("expected figure, got {other:?}"),
    }
}

#[test]
fn encode_media_blank_line_between_payloads() {
    let blocks = vec![
        ContentBlock::Figure {
            chunk_id: Some(1),
            figure: FigureRef {
                image_chunk_id: 2,
                alt_text: "a".into(),
                title: None,
                caption: None,
                placement: ImagePlacement::Flow,
            },
        },
        ContentBlock::Figure {
            chunk_id: Some(3),
            figure: FigureRef {
                image_chunk_id: 4,
                alt_text: "b".into(),
                title: None,
                caption: None,
                placement: ImagePlacement::Flow,
            },
        },
    ];
    let media = vec![
        TessprekMediaEntry {
            chunk_id: 2,
            media_type: Some("image/png".into()),
            sha256: Some("aa".into()),
            width_px: Some(1),
            height_px: Some(1),
        },
        TessprekMediaEntry {
            chunk_id: 4,
            media_type: Some("image/jpeg".into()),
            sha256: Some("bb".into()),
            width_px: Some(2),
            height_px: Some(2),
        },
    ];
    let text = encode_content_blocks(&TessprekDocMeta::default(), &blocks, &[], &media);
    assert!(
        text.contains(
            "\\media{\n  id=2\n  media_type=image/png\n  sha256=aa\n  width=1\n  height=1\n\n  id=4\n"
        ),
        "{text}"
    );
}

#[test]
fn encode_projects_catalog_meta_into_header() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("meta.tes");
    let mut session = TesWriterSession::create(&path, DocKind::Note);
    let mut catalog = DocumentCatalog::new(
        "550e8400-e29b-41d4-a716-446655440099",
        "Text roles tour",
        "2026-07-27T00:00:00Z",
        "2026-07-27T00:00:00Z",
        DocKind::Note,
    );
    catalog.language = Some("en".into());
    catalog.cite_style_id = Some("numeric".into());
    catalog.theme_id = Some("default".into());
    catalog.template_id = Some("article".into());
    catalog.slug = Some("text-roles".into());
    session.set_catalog(catalog).unwrap();
    session
        .add_text_chunk(&TextHeader::paragraph(), "Hi")
        .unwrap();
    session.commit().unwrap();

    let file = TesFile::open(&path).unwrap();
    let text = encode_tessprek(&file, "deadbeef").unwrap();
    assert!(text.contains("\\tessera{\n"), "{text}");
    assert!(text.contains("  format=tessprek\n"), "{text}");
    assert!(text.contains("  version=2\n"), "{text}");
    assert!(text.contains("  source-hash=deadbeef\n"), "{text}");
    assert!(
        text.contains("  doc_id=550e8400-e29b-41d4-a716-446655440099\n"),
        "{text}"
    );
    assert!(text.contains("  doc_kind=note\n"), "{text}");
    assert!(text.contains("  title=\"Text roles tour\"\n"), "{text}");
    assert!(text.contains("  language=en\n"), "{text}");
    assert!(text.contains("  cite_style_id=numeric\n"), "{text}");
    assert!(text.contains("  theme_id=default\n"), "{text}");
    assert!(text.contains("  template_id=article\n"), "{text}");
    assert!(text.contains("  slug=text-roles\n"), "{text}");
    assert!(text.contains("}\n\\ids{"), "{text}");
    assert_eq!(decode_tessprek(&text).unwrap().len(), 1);
}

#[test]
fn decode_accepts_multiline_and_single_line_header() {
    let multi = "\
\\tessera{\n\
  format=tessprek\n\
  version=2\n\
  title=\"Hello\"\n\
}\n\
\\ids{1}\n\
\n\
# Hello\n\
";
    assert_eq!(decode_tessprek(multi).unwrap().len(), 1);
    let single = "\
\\tessera{format=tessprek version=2 title=\"Hello\"}\n\
\\ids{1}\n\
\n\
# Hello\n\
";
    assert_eq!(decode_tessprek(single).unwrap().len(), 1);
}

#[test]
fn unknown_header_keys_are_listed() {
    let mut map = BTreeMap::new();
    map.insert("format".into(), "tessprek".into());
    map.insert("bogus".into(), "x".into());
    map.insert("tags".into(), "a,b".into());
    let unknown = TessprekDocMeta::unknown_keys(&map);
    assert_eq!(unknown, vec!["bogus".to_string(), "tags".to_string()]);
}

#[test]
fn decode_rejects_missing_header() {
    let err = decode_tessprek("# Title\n").unwrap_err();
    assert!(matches!(err, TesError::EditParse { .. }));
}

#[test]
fn decode_rejects_id_count_mismatch() {
    let text = "\\tessera{format=tessprek version=2}\n\\ids{1,2}\n\n# Title\n";
    let err = decode_tessprek(text).unwrap_err();
    match err {
        TesError::EditParse { message, .. } => {
            assert!(message.contains("id(s)"), "{message}");
            assert!(
                message.contains("TesseraFormat") || message.contains("tes format"),
                "{message}"
            );
        }
        other => panic!("expected EditParse, got {other:?}"),
    }
}

#[test]
fn decode_rejects_v1_version() {
    let text = "\\tessera{format=tessprek version=1}\n\\ids{}\n";
    let err = decode_tessprek(text).unwrap_err();
    match err {
        TesError::EditParse { message, .. } => {
            assert!(message.contains("v1"), "{message}");
        }
        other => panic!("expected EditParse, got {other:?}"),
    }
}

#[test]
fn inline_font_round_trips_in_tessprek() {
    let input = "\
\\tessera{format=tessprek version=2}\n\
\\ids{1}\n\
\n\
Say \\font{armenian}{barev} now.\n\
";
    let blocks = decode_tessprek(input).unwrap();
    assert_eq!(blocks.len(), 1);
    match &blocks[0] {
        ContentBlock::Text {
            body,
            pending_fonts,
            ..
        } => {
            assert_eq!(body, "Say barev now.");
            assert_eq!(pending_fonts.len(), 1);
            assert_eq!(pending_fonts[0].font_id, "armenian");
            assert_eq!(
                &body[pending_fonts[0].start as usize..pending_fonts[0].end as usize],
                "barev"
            );
        }
        other => panic!("expected text, got {other:?}"),
    }
    let out = encode_content_blocks(&TessprekDocMeta::default(), &blocks, &[], &[]);
    assert!(
        out.contains("\\font{armenian}{barev}"),
        "expected font macro in encode:\n{out}"
    );
}

#[test]
fn inline_cite_key_round_trips_in_tessprek() {
    let input = "\
\\tessera{format=tessprek version=2 cite_style_id=numeric}\n\
\\ids{1,2}\n\
\n\
Prior work \\cite{keller2020} established the baseline.\n\
\n\
\\cite{label=keller2020}
";
    let blocks = decode_tessprek(input).unwrap();
    assert_eq!(blocks.len(), 2);
    match &blocks[0] {
        ContentBlock::Text {
            body,
            pending_cites,
            ..
        } => {
            assert!(body.contains("keller2020"), "{body}");
            assert!(!body.contains("\\cite{"), "{body}");
            assert_eq!(pending_cites.len(), 1);
            assert_eq!(pending_cites[0].key, "keller2020");
        }
        other => panic!("expected text, got {other:?}"),
    }
    match &blocks[1] {
        ContentBlock::Cite { cite, .. } => {
            assert_eq!(cite.label.as_deref(), Some("keller2020"));
            assert!(cite.quote.is_empty(), "{:?}", cite.quote);
            assert!(cite.target_chunk_id.is_none());
        }
        other => panic!("expected cite, got {other:?}"),
    }
    let out = encode_content_blocks(
        &TessprekDocMeta {
            cite_style_id: Some("numeric".into()),
            ..TessprekDocMeta::default()
        },
        &blocks,
        &[],
        &[],
    );
    assert!(
        out.contains("\\cite{keller2020}") || out.contains("label=keller2020"),
        "{out}"
    );
    assert!(out.contains("Prior work"), "{out}");
    assert!(!out.contains("\\quote{"), "{out}");
}

#[test]
fn quote_and_ref_round_trip_in_tessprek() {
    let input = "\
\\tessera{format=tessprek version=2}\n\
\\ids{1,2}\n\
\n\
\\quote{\n\
  label=Smith2024\n\
  target_chunk=3\n\
  target_byte_start=0\n\
  target_byte_end=12\n\
  quote=\"Hello world\"\n\
}\n\
\n\
\\ref{\n\
  target_doc=550e8400-e29b-41d4-a716-446655440040\n\
  target_chunk=1\n\
}\n\
";
    let blocks = decode_tessprek(input).unwrap();
    assert_eq!(blocks.len(), 2);
    match &blocks[0] {
        ContentBlock::Cite { cite, .. } => {
            assert_eq!(cite.quote, "Hello world");
            assert_eq!(cite.target_chunk_id, Some(3));
        }
        other => panic!("expected quote cite, got {other:?}"),
    }
    match &blocks[1] {
        ContentBlock::Cite { cite, .. } => {
            assert!(cite.quote.is_empty());
            assert_eq!(cite.target_chunk_id, Some(1));
        }
        other => panic!("expected ref cite, got {other:?}"),
    }
    let out = encode_content_blocks(&TessprekDocMeta::default(), &blocks, &[], &[]);
    assert!(out.contains("\\quote{"), "{out}");
    assert!(out.contains("\\ref{"), "{out}");
    assert!(out.contains("quote=\"Hello world\""), "{out}");
    assert!(!out.contains("> Hello"), "{out}");
}
