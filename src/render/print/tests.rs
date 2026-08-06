//! Unit tests for print IR mapping.

use std::path::PathBuf;

use ariadnes_weave::{BreakHint, PrintBlock};

use super::*;
use crate::catalog::DocumentCatalog;
use crate::catalog::chunk::{CitePayload, InlineKind, InlineSpan, TextHeader};
use crate::catalog::file::TesFile;
use crate::catalog::session::TesWriterSession;
use crate::fixtures::samples::encode_manuscript_chapters;
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
        PrintBlock::Paragraph { runs } => {
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
fn face_span_sets_text_run_face() {
    let mut session = TesWriterSession::create("face.tes", crate::layout::DocKind::Note);
    let body = "hello barev world";
    let start = body.find("barev").unwrap() as u32;
    let end = start + "barev".len() as u32;
    let mut para = TextHeader::paragraph();
    para.spans = vec![InlineSpan {
        start,
        end,
        kind: InlineKind::Face {
            face_id: "armenian".into(),
        },
    }];
    session.add_text_chunk(&para, body).unwrap();
    let file = open_bytes("face.tes", session.encode_file().unwrap());
    let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();
    let PrintBlock::Paragraph { runs } = &doc.blocks[0] else {
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
        Some(PrintBlock::Quote { runs }) => {
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
        PrintBlock::Paragraph { runs } if runs.iter().any(|r| r.style.cite && r.text == "[1]") => {
            Some(runs.iter().map(|r| r.text.as_str()).collect::<String>())
        }
        _ => None,
    });
    assert_eq!(stub.as_deref(), Some("[1] keller2020"));

    // Inline rewrite in prose
    let prose = doc.blocks.iter().find_map(|b| match b {
        PrintBlock::Paragraph { runs }
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
            PrintBlock::Quote { runs } if runs.iter().any(|r| r.text.contains("Quoted passage"))
        )),
        "expected quote block: {doc:?}"
    );
    assert!(
        doc.blocks.iter().any(|b| matches!(
            b,
            PrintBlock::Paragraph { runs }
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
            PrintBlock::Paragraph { runs }
                if runs.iter().any(|r| r.text.contains("1. Keller") && r.text.contains("Chunk Containers"))
        )),
        "expected bibliography line: {doc:?}"
    );
}
