use super::*;
use crate::catalog::index::ChunkType;
use crate::catalog::{DocumentCatalog, ListKind, TesWriterSession, TextHeader};
use crate::layout::DocKind;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

fn write_note(dir: &std::path::Path) -> PathBuf {
    let path = dir.join("note.tes");
    let mut s = TesWriterSession::create(&path, DocKind::Note);
    s.set_catalog(DocumentCatalog::new(
        "550e8400-e29b-41d4-a716-446655440000",
        "Meeting notes",
        "2026-06-05T12:00:00Z",
        "2026-06-05T12:30:00Z",
        DocKind::Note,
    ))
    .unwrap();
    s.add_text_chunk(&TextHeader::paragraph(), "Hello from Tessera.")
        .unwrap();
    s.commit().unwrap();
    path
}

fn write_article(dir: &std::path::Path) -> PathBuf {
    let path = dir.join("article.tes");
    let mut s = TesWriterSession::create(&path, DocKind::Document);
    s.set_catalog(DocumentCatalog::new(
        "660e8400-e29b-41d4-a716-446655440001",
        "Methods",
        "2026-06-05T12:00:00Z",
        "2026-06-05T12:00:00Z",
        DocKind::Document,
    ))
    .unwrap();
    s.add_text_chunk(&TextHeader::heading(1), "Methods")
        .unwrap();
    s.add_text_chunk(
        &TextHeader::paragraph(),
        "We measured temperature at 15 stations.",
    )
    .unwrap();
    s.add_text_chunk(&TextHeader::list_item(ListKind::Bullet), "Calibrate first")
        .unwrap();
    s.commit().unwrap();
    path
}

#[test]
fn raw_note_one_chunk_fixture() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/v0/note_one_chunk.tes");
    let out = export_view(&path, ExportView::Raw, &ExportOptions::default()).unwrap();
    assert_eq!(
        out,
        "Hello from Tessera — use tes textconv for readable diffs.\n"
    );
}

#[test]
fn ai_text_has_no_exporter_markup() {
    let dir = tempdir().unwrap();
    let path = write_article(dir.path());
    let out = export_view(&path, ExportView::AiText, &ExportOptions::default()).unwrap();
    assert!(!out.contains('#'));
    assert!(!out.contains("<"));
    assert!(!out.contains("**"));
    assert!(out.contains("We measured temperature at 15 stations."));
    assert!(out.contains("Calibrate first"));
    // Heading body is included as plain text, without # markers.
    assert!(out.contains("Methods"));
}

#[test]
fn linear_emits_heading_markers() {
    let dir = tempdir().unwrap();
    let path = write_article(dir.path());
    let out = export_view(&path, ExportView::Linear, &ExportOptions::default()).unwrap();
    assert!(out.starts_with("# Methods\n"));
    assert!(out.contains("\n- Calibrate first\n"));
}

#[test]
fn markdown_preserves_block_structure_lossily() {
    let dir = tempdir().unwrap();
    let path = write_article(dir.path());
    let out = export_view(&path, ExportView::Markdown, &ExportOptions::default()).unwrap();
    assert!(out.starts_with("# Methods\n\n"));
    assert!(out.contains("\n\n- Calibrate first\n"));
}

#[test]
fn chunks_jsonl_line_count_matches_reading_order() {
    let dir = tempdir().unwrap();
    let path = write_article(dir.path());
    let out = export_view(&path, ExportView::ChunksJsonl, &ExportOptions::default()).unwrap();
    let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 3);
    let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(first["role"], "heading");
    assert_eq!(first["text"], "Methods");
    assert_eq!(first["doc_title"], "Methods");
}

#[test]
fn raw_chunk_filter() {
    let dir = tempdir().unwrap();
    let path = write_article(dir.path());
    let opts = ExportOptions {
        chunk_id: Some(2),
        ..Default::default()
    };
    let out = export_view(&path, ExportView::Raw, &opts).unwrap();
    assert_eq!(out, "We measured temperature at 15 stations.\n");
}

#[test]
fn chapter_filter_excludes_front_matter_and_other_chapters() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("ms.tes");
    fs::write(
        &path,
        crate::fixtures::samples::encode_manuscript_chapters(),
    )
    .unwrap();

    let ch2 = export_view(
        &path,
        ExportView::Markdown,
        &ExportOptions {
            chapter: Some(2),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(ch2.contains("Chapter 2 — The Signal"));
    assert!(ch2.contains("lantern blinked"));
    assert!(!ch2.contains("Chapter 1"));
    assert!(!ch2.contains("Chapter 3"));
    assert!(!ch2.contains("Front matter"));
    assert!(!ch2.contains("beta readers"));

    let ch1 = export_view(
        &path,
        ExportView::Markdown,
        &ExportOptions {
            chapter: Some(1),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(ch1.contains("Chapter 1 — The Quay"));
    assert!(ch1.contains("Scene: Warehouse"));
    assert!(!ch1.contains("Chapter 2"));

    let err = export_view(
        &path,
        ExportView::Raw,
        &ExportOptions {
            chapter: Some(9),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("chapter 9 not found"));
}

#[test]
fn chapter_and_chunk_conflict() {
    let dir = tempdir().unwrap();
    let path = write_article(dir.path());
    let err = export_view(
        &path,
        ExportView::Raw,
        &ExportOptions {
            chunk_id: Some(1),
            chapter: Some(1),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("mutually exclusive"));
}

#[test]
fn html_coalesces_ordered_lists_and_renders_mathml() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("lists_math.tes");
    let mut s = TesWriterSession::create(&path, DocKind::Document);
    s.set_catalog(DocumentCatalog::new(
        "990e8400-e29b-41d4-a716-446655440099",
        "Lists and math",
        "2026-07-30T00:00:00Z",
        "2026-07-30T00:00:00Z",
        DocKind::Document,
    ))
    .unwrap();
    s.add_text_chunk(&TextHeader::heading(1), "Open questions")
        .unwrap();
    s.add_text_chunk(&TextHeader::list_item(ListKind::Ordered), "First question")
        .unwrap();
    s.add_text_chunk(&TextHeader::list_item(ListKind::Ordered), "Second question")
        .unwrap();
    s.add_text_chunk(&TextHeader::list_item(ListKind::Ordered), "Third question")
        .unwrap();
    s.add_text_chunk(&TextHeader::math(), r"\Delta = \frac{a}{b}")
        .unwrap();
    s.commit().unwrap();

    let html = export_view(&path, ExportView::Html, &ExportOptions::default()).unwrap();
    assert!(
        html.contains("<ol data-list-depth=\"1\">"),
        "expected one ordered list, got:\n{html}"
    );
    assert_eq!(
        html.matches("<ol data-list-depth=\"1\">").count(),
        1,
        "ordered items must share one <ol>, got:\n{html}"
    );
    assert!(html.contains("First question"));
    assert!(html.contains("Second question"));
    assert!(html.contains("Third question"));
    assert!(
        html.contains("<math") || html.contains("math-fallback"),
        "expected MathML or fallback, got:\n{html}"
    );
    assert!(
        !html.contains("<ol data-list-depth=\"1\"><li data-chunk-id=\"2\""),
        "must not wrap each item in its own ol"
    );
}

#[test]
fn html_renders_text_block_captions() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("captions.tes");
    let mut s = TesWriterSession::create(&path, DocKind::Document);
    s.set_catalog(DocumentCatalog::new(
        "a90e8400-e29b-41d4-a716-4466554400aa",
        "Captions",
        "2026-08-04T00:00:00Z",
        "2026-08-04T00:00:00Z",
        DocKind::Document,
    ))
    .unwrap();
    let mut code = TextHeader::code_block(Some("rust"));
    code.caption = Some("Snippet".into());
    s.add_text_chunk(&code, "fn main() {}").unwrap();
    let mut math = TextHeader::math();
    math.caption = Some("Identity".into());
    s.add_text_chunk(&math, "a = a").unwrap();
    s.commit().unwrap();

    let file = crate::catalog::TesFile::open(&path).unwrap();
    let html = export_file(&file, ExportView::Html, &ExportOptions::default()).unwrap();
    assert!(
        html.contains("<p class=\"tes-caption\">Snippet</p>"),
        "{html}"
    );
    assert!(
        html.contains("<p class=\"tes-caption\">Identity</p>"),
        "{html}"
    );
}

#[test]
fn annotate_ai_text() {
    let dir = tempdir().unwrap();
    let path = write_note(dir.path());
    let opts = ExportOptions {
        annotate: true,
        ..Default::default()
    };
    let out = export_view(&path, ExportView::AiText, &opts).unwrap();
    assert!(out.starts_with("<!-- chunk:1 -->\nHello from Tessera."));
}

#[test]
fn reusable_image_two_figures_and_exports() {
    use crate::catalog::{FigureRef, ImagePayload, ImagePlacement};
    use std::fs;

    let dir = tempdir().unwrap();
    let jpeg = fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/assets/images/square.jpg"),
    )
    .unwrap();
    let path = dir.path().join("figures.tes");
    let mut s = TesWriterSession::create(&path, DocKind::Document);
    s.set_catalog(DocumentCatalog::new(
        "770e8400-e29b-41d4-a716-446655440002",
        "Figures",
        "2026-07-25T00:00:00Z",
        "2026-07-25T00:00:00Z",
        DocKind::Document,
    ))
    .unwrap();
    s.add_text_chunk(&TextHeader::heading(1), "Gallery")
        .unwrap();
    let image_id = s
        .add_image_chunk(&ImagePayload {
            media_type: "image/jpeg".into(),
            width_px: 100,
            height_px: 100,
            data: jpeg,
        })
        .unwrap();
    s.add_figure(&FigureRef {
        image_chunk_id: image_id,
        alt_text: "Square crop".into(),
        title: None,
        caption: Some("First use".into()),
        placement: ImagePlacement::Flow,
    })
    .unwrap();
    s.add_figure(&FigureRef {
        image_chunk_id: image_id,
        alt_text: "Square crop again".into(),
        title: None,
        caption: Some("Second use, full width".into()),
        placement: ImagePlacement::FullWidth,
    })
    .unwrap();
    s.commit().unwrap();

    let file = crate::catalog::TesFile::open(&path).unwrap();
    assert_eq!(file.chunks().len(), 4); // heading + image + 2 figures
    let html = export_file(&file, ExportView::Html, &ExportOptions::default()).unwrap();
    assert!(html.contains("<figure data-chunk-id=\"3\""));
    assert!(html.contains("<figure data-chunk-id=\"4\""));
    assert!(html.contains("data-image-chunk=\"2\""));
    assert!(html.contains("data:image/jpeg;base64,"));
    assert_eq!(html.matches("data:image/jpeg;base64,").count(), 2);

    let md = export_file(&file, ExportView::Markdown, &ExportOptions::default()).unwrap();
    assert!(md.contains("![Square crop](media:2)"));
    assert!(md.contains("![Square crop again](media:2)"));

    let parts = export_ai_parts(&file, &ExportOptions::default()).unwrap();
    assert!(matches!(parts[0], AiPart::Text(_)));
    assert!(matches!(
        &parts[1],
        AiPart::Image {
            image_chunk_id: 2,
            alt_text,
            ..
        } if alt_text == "Square crop"
    ));
    assert!(matches!(
        &parts[2],
        AiPart::Image {
            image_chunk_id: 2,
            ..
        }
    ));
    // Same underlying bytes reused.
    let AiPart::Image { data: d1, .. } = &parts[1] else {
        panic!("expected image");
    };
    let AiPart::Image { data: d2, .. } = &parts[2] else {
        panic!("expected image");
    };
    assert_eq!(d1, d2);

    let report = crate::verify::verify_tes_file(&path, true).unwrap();
    assert!(report.ok, "{:?}", report.findings);
}

#[test]
fn research_cites_mirror_tlnk_and_export() {
    use crate::catalog::link::LinkKind;
    use crate::catalog::{CitePayload, TesFile};
    use crate::io::bib::{
        BibEntry, BibFormat, BibImportOptions, export_bibliography, import_bibliography,
    };

    let dir = tempdir().unwrap();
    let sample =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/assets/citations/sample.bib");
    let bib_tes = dir.path().join("from_bib.tes");
    import_bibliography(
        &sample,
        &bib_tes,
        BibFormat::Bibtex,
        &BibImportOptions::default(),
    )
    .unwrap();

    let target = "770e8400-e29b-41d4-a716-446655440099";
    let path = dir.path().join("paper.tes");
    let mut catalog = DocumentCatalog::new(
        "880e8400-e29b-41d4-a716-446655440088",
        "Cite specimen",
        "2026-07-25T00:00:00Z",
        "2026-07-25T00:00:00Z",
        DocKind::Research,
    );
    catalog.cite_style_id = Some("numeric".into());
    let mut session = TesWriterSession::create(&path, DocKind::Research);
    session.set_catalog(catalog).unwrap();
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Prior work established the baseline.",
        )
        .unwrap();
    session
        .add_cite_chunk(&CitePayload {
            quote: "Chunk-oriented containers help.".into(),
            target_doc_id: Some(target.into()),
            target_chunk_id: Some(1),
            target_byte_start: Some(0),
            target_byte_end: Some(12),
            label: Some("keller2020chunking".into()),
            page: Some(3),
            source: Some(BibEntry {
                cite_key: "keller2020chunking".into(),
                entry_type: "article".into(),
                author: Some("Keller, Ada and Hurowitz, Alex".into()),
                title: Some("Chunk-Oriented Document Containers for Local-First Notes".into()),
                journal: Some("Fixtures Review".into()),
                year: Some("2020".into()),
                ..BibEntry::default()
            }),
        })
        .unwrap();
    session.commit().unwrap();

    let file = TesFile::open(&path).unwrap();
    assert_eq!(file.links().len(), 1);
    assert_eq!(file.links()[0].link_kind, LinkKind::Citation);
    assert_eq!(file.links()[0].source_chunk_id, 2);

    let md = export_view(&path, ExportView::Markdown, &ExportOptions::default()).unwrap();
    assert!(md.contains("[@keller2020chunking]"));
    assert!(md.contains("## References"));

    let html = export_view(&path, ExportView::Html, &ExportOptions::default()).unwrap();
    assert!(html.contains("class=\"citation\""));
    assert!(html.contains("class=\"bibliography\""));
    assert!(html.contains("[1]"));

    let bibtex = export_bibliography(&path, BibFormat::Bibtex).unwrap();
    assert!(bibtex.contains("@article{keller2020chunking,"));

    let from_bib = TesFile::open(&bib_tes).unwrap();
    assert_eq!(
        from_bib
            .reading_order_chunks()
            .iter()
            .filter(|c| c.chunk_type == ChunkType::Cite)
            .count(),
        3
    );

    let report = crate::verify::verify_tes_file(&path, true).unwrap();
    assert!(report.ok, "{:?}", report.findings);
    assert!(
        !report.findings.iter().any(|f| f.check == "cite.mirror"),
        "{:?}",
        report.findings
    );
}

#[test]
fn deck_slides_export_html_regions() {
    use crate::catalog::SlidePayload;

    let dir = tempdir().unwrap();
    let path = dir.path().join("deck.tes");
    let mut s = TesWriterSession::create(&path, DocKind::Deck);
    s.set_catalog(DocumentCatalog::new(
        "880e8400-e29b-41d4-a716-446655440003",
        "Demo deck",
        "2026-07-28T00:00:00Z",
        "2026-07-28T00:00:00Z",
        DocKind::Deck,
    ))
    .unwrap();
    s.add_text_chunk(&TextHeader::heading(1), "Hello slides")
        .unwrap();
    s.add_text_chunk(&TextHeader::paragraph(), "Region body copy.")
        .unwrap();
    s.add_slide(&SlidePayload::title_body(1, 2)).unwrap();
    s.commit().unwrap();

    let report = crate::verify::verify_tes_file(&path, true).unwrap();
    assert!(report.ok, "{:?}", report.findings);

    let html = export_view(
        &path,
        ExportView::Html,
        &ExportOptions {
            standalone: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(html.contains("class=\"deck\""));
    assert!(html.contains("data-layout=\"title_body\""));
    assert!(html.contains("data-region=\"title\""));
    assert!(html.contains("Hello slides"));
    assert!(html.contains("Region body copy."));
    assert!(!html.contains("<article"));
}

#[test]
fn attachment_round_trip_verify_and_inert_export() {
    use crate::catalog::AttachmentPayload;
    use crate::edit::{EditWriteOptions, edit_read, edit_write};

    let dir = tempdir().unwrap();
    let path = dir.path().join("with_att.tes");
    let mut s = TesWriterSession::create(&path, DocKind::Note);
    s.set_catalog(DocumentCatalog::new(
        "990e8400-e29b-41d4-a716-446655440099",
        "Attachment specimen",
        "2026-07-28T00:00:00Z",
        "2026-07-28T00:00:00Z",
        DocKind::Note,
    ))
    .unwrap();
    s.add_text_chunk(&TextHeader::paragraph(), "See the PDF.")
        .unwrap();
    let att = AttachmentPayload::new(
        "application/pdf",
        "notes.pdf",
        b"%PDF-1.4 tessera-fixture".to_vec(),
        Some("Lab notes".into()),
    )
    .unwrap();
    let att_id = s.add_attachment_chunk(&att).unwrap();
    s.commit().unwrap();

    let report = crate::verify::verify_tes_file(&path, true).unwrap();
    assert!(report.ok, "{:?}", report.findings);

    let file = TesFile::open(&path).unwrap();
    let exported = export_attachment_bytes(&file, att_id).unwrap();
    assert_eq!(exported.data, b"%PDF-1.4 tessera-fixture");
    assert_eq!(exported.filename, "notes.pdf");

    let linear = export_view(&path, ExportView::Linear, &ExportOptions::default()).unwrap();
    assert!(linear.contains("[attachment filename=notes.pdf"));
    assert!(!linear.contains("%PDF"));

    let html = export_view(
        &path,
        ExportView::Html,
        &ExportOptions {
            attachment_url_prefix: Some("/attachment/".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(html.contains("tes-attachment"));
    assert!(html.contains(&format!("href=\"/attachment/{att_id}\"")));
    assert!(html.contains("download=\"notes.pdf\""));
    assert!(!html.contains("data:application/pdf"));

    let read = edit_read(&path).unwrap();
    assert!(read.tessprek.contains("\\attach{"));
    assert!(read.tessprek.contains("filename=\"notes.pdf\""));
    edit_write(
        &path,
        &read.tessprek,
        &EditWriteOptions::new(read.source_hash.clone(), false),
    )
    .unwrap();
    let report2 = crate::verify::verify_tes_file(&path, true).unwrap();
    assert!(report2.ok, "{:?}", report2.findings);
}
