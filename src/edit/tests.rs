use std::path::{Path, PathBuf};

use super::*;
use crate::catalog::TesWriterSession;
use crate::catalog::chunk::TextHeader;
use crate::catalog::document::DocumentCatalog;
use crate::catalog::index::ChunkType;
use crate::catalog::media::{AttachmentPayload, FigureRef, ImagePayload};
use crate::catalog::{CitePayload, TesFile};
use crate::error::TesError;
use crate::layout::DocKind;
use crate::verify::verify_tes_file;
use tempfile::tempdir;

fn sample_note(dir: &Path) -> PathBuf {
    let path = dir.join("note.tes");
    let mut session = TesWriterSession::create(&path, DocKind::Note);
    session
        .set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440000",
            "Meeting notes",
            "2026-07-27T00:00:00Z",
            "2026-07-27T00:00:00Z",
            DocKind::Note,
        ))
        .unwrap();
    session
        .add_text_chunk(&TextHeader::heading(1), "Agenda")
        .unwrap();
    session
        .add_text_chunk(&TextHeader::paragraph(), "Ship Tessprek")
        .unwrap();
    session.commit().unwrap();
    path
}

fn apply_json(path: &Path, ops_json: &str, dry_run: bool) -> EditWriteReport {
    let read = edit_read(path).unwrap();
    apply_ops(
        path,
        &parse_ops_json(ops_json).unwrap(),
        &EditWriteOptions::new(read.source_hash, dry_run),
    )
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
fn assert_meta(
    aliases: &[String],
    slug: &Option<String>,
    category: &Option<String>,
    section: &Option<String>,
    want_aliases: &[&str],
    want_slug: Option<&str>,
    want_category: Option<&str>,
    want_section: Option<&str>,
) {
    assert_eq!(aliases, want_aliases);
    assert_eq!(slug.as_deref(), want_slug);
    assert_eq!(category.as_deref(), want_category);
    assert_eq!(section.as_deref(), want_section);
}

fn sample_cite_doc(dir: &Path, name: &str, title: &str, body: &str, cite: CitePayload) -> PathBuf {
    let path = dir.join(name);
    let mut session = TesWriterSession::create(&path, DocKind::Research);
    session
        .set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440000",
            title,
            "2026-08-04T00:00:00Z",
            "2026-08-04T00:00:00Z",
            DocKind::Research,
        ))
        .unwrap();
    session
        .add_text_chunk(&TextHeader::paragraph(), body)
        .unwrap();
    session.add_cite_chunk(&cite).unwrap();
    session.commit().unwrap();
    path
}

#[test]
fn edit_read_write_round_trip() {
    let dir = tempdir().unwrap();
    let path = sample_note(dir.path());
    let read = edit_read(&path).unwrap();
    assert!(read.tessprek.contains("Agenda"));
    assert_eq!(read.source_hash.len(), 64);

    let edited = read.tessprek.replace("Ship Tessprek", "Ship edit protocol");
    // Keep directives intact; only body changed.
    let report = edit_write(
        &path,
        &edited,
        &EditWriteOptions::new(read.source_hash.clone(), false),
    )
    .unwrap();
    assert!(report.replaced);
    let again = edit_read(&path).unwrap();
    assert!(again.tessprek.contains("Ship edit protocol"));
    assert_ne!(again.source_hash, read.source_hash);
}

#[test]
fn edit_write_applies_biblio_source_attrs_not_stale_chunk() {
    use crate::io::bib::BibEntry;

    let dir = tempdir().unwrap();
    let path = dir.path().join("refs.tes");
    let mut session = TesWriterSession::create(&path, DocKind::Research);
    session
        .set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440099",
            "Bibliography",
            "2026-08-05T00:00:00Z",
            "2026-08-05T00:00:00Z",
            DocKind::Research,
        ))
        .unwrap();
    session
        .add_cite_chunk(&CitePayload {
            quote: String::new(),
            target_doc_id: None,
            target_chunk_id: None,
            target_byte_start: None,
            target_byte_end: None,
            label: Some("stale".into()),
            page: None,
            source: Some(BibEntry {
                cite_key: "stale".into(),
                entry_type: "article".into(),
                author: Some("Old, Author".into()),
                title: Some("Stale title".into()),
                year: Some("1999".into()),
                ..BibEntry::default()
            }),
        })
        .unwrap();
    session.commit().unwrap();

    let read = edit_read(&path).unwrap();
    let tessprek = "\
\\tessera{format=tessprek version=2 cite_style_id=numeric}\n\
\\ids{1}\n\
\n\
\\cite{\n\
  label=Duque-Quintero2022\n\
  entry_type=article\n\
  author=\"Duque-Quintero, Mariana\"\n\
  title=\"ELA meta-analysis\"\n\
  year=2022\n\
}\n\
";
    edit_write(
        &path,
        tessprek,
        &EditWriteOptions::new(read.source_hash, false),
    )
    .unwrap();
    let again = edit_read(&path).unwrap();
    assert!(
        again.tessprek.contains("Duque-Quintero2022"),
        "{}",
        again.tessprek
    );
    assert!(
        again.tessprek.contains("ELA meta-analysis"),
        "{}",
        again.tessprek
    );
    assert!(
        !again.tessprek.contains("Stale title"),
        "must not keep on-disk BibEntry when Tessprek supplies source attrs: {}",
        again.tessprek
    );
}

#[test]
fn edit_read_write_preserves_cite_byte_ranges() {
    let dir = tempdir().unwrap();
    let path = sample_cite_doc(
        dir.path(),
        "cite.tes",
        "Cite ranges",
        "ABCDEFGHIJ",
        CitePayload {
            quote: "ABCD".into(),
            target_doc_id: None,
            target_chunk_id: Some(1),
            target_byte_start: Some(0),
            target_byte_end: Some(4),
            label: Some("local".into()),
            page: None,
            source: None,
        },
    );

    let read = edit_read(&path).unwrap();
    assert!(
        read.tessprek.contains("target_byte_start=0"),
        "{}",
        read.tessprek
    );
    assert!(
        read.tessprek.contains("target_byte_end=4"),
        "{}",
        read.tessprek
    );

    let report = edit_write(
        &path,
        &read.tessprek,
        &EditWriteOptions::new(read.source_hash.clone(), false),
    )
    .unwrap();
    assert!(report.replaced);

    let file = TesFile::open(&path).unwrap();
    let cite_entry = file
        .reading_order_chunks()
        .into_iter()
        .find(|c| c.chunk_type == ChunkType::Cite)
        .expect("cite chunk");
    let raw = file.decode_payload(cite_entry).unwrap();
    let cite = CitePayload::from_bytes(raw.as_ref()).unwrap();
    assert_eq!(cite.target_byte_start, Some(0));
    assert_eq!(cite.target_byte_end, Some(4));
    assert_eq!(cite.target_chunk_id, Some(1));

    let report = verify_tes_file(&path, true).unwrap();
    assert!(report.ok, "{:?}", report.findings);
    assert!(
        !report.findings.iter().any(|f| f.check == "cite.range"),
        "{:?}",
        report.findings
    );
}

#[test]
fn verify_warns_on_cite_byte_range_oob() {
    let dir = tempdir().unwrap();
    let path = sample_cite_doc(
        dir.path(),
        "cite_oob.tes",
        "Cite OOB",
        "short",
        CitePayload {
            quote: "too long".into(),
            target_doc_id: None,
            target_chunk_id: Some(1),
            target_byte_start: Some(0),
            target_byte_end: Some(99),
            label: Some("oob".into()),
            page: None,
            source: None,
        },
    );

    let report = verify_tes_file(&path, true).unwrap();
    assert!(
        report.findings.iter().any(|f| f.check == "cite.range"),
        "{:?}",
        report.findings
    );
}

#[test]
fn source_hash_mismatch_rejects() {
    let dir = tempdir().unwrap();
    let path = sample_note(dir.path());
    let read = edit_read(&path).unwrap();
    let err = edit_write(
        &path,
        &read.tessprek,
        &EditWriteOptions::new("deadbeef", false),
    )
    .unwrap_err();
    assert!(matches!(err, TesError::SourceHashMismatch { .. }));
}

#[test]
fn apply_ops_set_text_dry_run() {
    let dir = tempdir().unwrap();
    let path = sample_note(dir.path());
    let report = apply_json(
        &path,
        r#"[{"op":"set_text","chunk_id":2,"body":"Updated body"},{"op":"set_title","title":"Renamed"}]"#,
        true,
    );
    assert!(!report.replaced);
    assert!(report.diff.contains("Updated body") || report.diff.contains('+'));
    // Original unchanged.
    let file = TesFile::open(&path).unwrap();
    assert_eq!(file.catalog().unwrap().title, "Meeting notes");
}

#[test]
fn apply_ops_catalog_aliases_slug_category_round_trip() {
    let dir = tempdir().unwrap();
    let vault = dir.path();
    let path = sample_note(vault);
    let report = apply_json(
        &path,
        r#"[
          {"op":"set_aliases","aliases":["American Fiction","Percival"]},
          {"op":"set_slug","slug":"Erasure"},
          {"op":"set_category","category":"Literature"},
          {"op":"set_section","section":"Books/Authors"}
        ]"#,
        false,
    );
    assert!(report.replaced);

    let file = TesFile::open(&path).unwrap();
    let cat = file.catalog().unwrap();
    assert_meta(
        &cat.aliases,
        &cat.slug,
        &cat.category,
        &cat.section,
        &["American Fiction", "Percival"],
        Some("Erasure"),
        Some("Literature"),
        Some("Books/Authors"),
    );

    crate::vault::rebuild_vault_index(vault).unwrap();
    let index = crate::vault::load_vault_index(vault).unwrap().unwrap();
    let entry = index
        .entries
        .iter()
        .find(|e| e.doc_id == cat.doc_id)
        .expect("note in vault.tes");
    assert_meta(
        &entry.aliases,
        &entry.slug,
        &entry.category,
        &entry.section,
        &["American Fiction", "Percival"],
        Some("Erasure"),
        Some("Literature"),
        Some("Books/Authors"),
    );
}

#[test]
fn apply_ops_set_tags_round_trip_and_clear() {
    let dir = tempdir().unwrap();
    let vault = dir.path();
    let path = sample_note(vault);

    let set = apply_json(
        &path,
        r#"[{"op":"set_tags","tags":["pilot","fiction"]},{"op":"set_title","title":"Tagged note"}]"#,
        false,
    );
    assert!(set.replaced);
    let cat = TesFile::open(&path).unwrap().catalog().unwrap().clone();
    assert_eq!(cat.title, "Tagged note");
    assert_eq!(cat.tags, vec!["pilot", "fiction"]);

    // Other catalog ops must not wipe tags.
    apply_json(
        &path,
        r#"[{"op":"set_aliases","aliases":["Alt"]},{"op":"set_slug","slug":"tagged-note"}]"#,
        false,
    );
    let cat = TesFile::open(&path).unwrap().catalog().unwrap().clone();
    assert_eq!(cat.tags, vec!["pilot", "fiction"]);
    assert_eq!(cat.aliases, vec!["Alt"]);
    assert_eq!(cat.slug.as_deref(), Some("tagged-note"));

    crate::vault::rebuild_vault_index(vault).unwrap();
    let index = crate::vault::load_vault_index(vault).unwrap().unwrap();
    let entry = index
        .entries
        .iter()
        .find(|e| e.doc_id == cat.doc_id)
        .expect("note in vault.tes");
    assert_eq!(entry.tags, vec!["pilot", "fiction"]);

    let clear = apply_json(&path, r#"[{"op":"set_tags","tags":[]}]"#, false);
    assert!(clear.replaced);
    let cat = TesFile::open(&path).unwrap().catalog().unwrap().clone();
    assert!(cat.tags.is_empty());
    assert_eq!(cat.aliases, vec!["Alt"]);
    assert_eq!(cat.slug.as_deref(), Some("tagged-note"));
    assert_eq!(cat.title, "Tagged note");
}

/// Append `extra` chunk ids to the `\ids{…}` header line (v2 has no
/// per-block ids; new appended blocks must be declared there).
fn with_extra_ids(tessprek: &str, extra: &[u64]) -> String {
    let idx = tessprek.find("\\ids{").expect("ids line");
    let start = idx + "\\ids{".len();
    let end = start + tessprek[start..].find('}').expect("ids close");
    let mut ids: Vec<String> = tessprek[start..end]
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    ids.extend(extra.iter().map(u64::to_string));
    format!(
        "{}{}{}",
        &tessprek[..start],
        ids.join(","),
        &tessprek[end..]
    )
}

#[test]
fn edit_write_with_media_injects_image_and_attachment() {
    let dir = tempdir().unwrap();
    let path = sample_note(dir.path());
    let read = edit_read(&path).unwrap();

    let image = ImagePayload {
        media_type: "image/png".into(),
        width_px: 2,
        height_px: 2,
        data: vec![0x89, 0x50, 0x4e, 0x47],
    };
    let att = AttachmentPayload::new(
        "application/pdf",
        "notes.pdf",
        b"%PDF-1.4 inject-test".to_vec(),
        Some("Handout".into()),
    )
    .unwrap();

    let base = with_extra_ids(read.tessprek.trim_end(), &[10, 11]);
    let tessprek = format!(
        "{base}\n\\figure{{image=900001 placement=flow alt=\"Injected\" caption=\"Shot\"}}\n\n\\attach{{filename=\"notes.pdf\" media_type=application/pdf sha256={} caption=\"Handout\"}}\n",
        att.sha256,
    );
    let media = EditMediaBag {
        images: vec![(900_001, image.clone())],
        attachments: vec![(11, att.clone())],
    };
    let report = edit_write_with_media(
        &path,
        &tessprek,
        &EditWriteOptions::new(read.source_hash, false),
        media,
    )
    .unwrap();
    assert!(report.replaced);

    let file = TesFile::open(&path).unwrap();
    let mut saw_image = false;
    let mut saw_figure = false;
    let mut saw_attachment = false;
    for entry in file.chunks() {
        match entry.chunk_type {
            ChunkType::Image => {
                let raw = file.decode_payload(entry).unwrap();
                let payload = ImagePayload::from_bytes(raw.as_ref()).unwrap();
                assert_eq!(payload.data, image.data);
                saw_image = true;
            }
            ChunkType::Figure => {
                let raw = file.decode_payload(entry).unwrap();
                let figure = FigureRef::from_bytes(raw.as_ref()).unwrap();
                assert_eq!(figure.alt_text, "Injected");
                assert_eq!(figure.caption.as_deref(), Some("Shot"));
                saw_figure = true;
            }
            ChunkType::Attachment => {
                let raw = file.decode_payload(entry).unwrap();
                let payload = AttachmentPayload::from_bytes(raw.as_ref()).unwrap();
                assert_eq!(payload.filename, "notes.pdf");
                assert_eq!(payload.data, att.data);
                assert_eq!(payload.sha256, att.sha256);
                saw_attachment = true;
            }
            _ => {}
        }
    }
    assert!(saw_image && saw_figure && saw_attachment);
}

#[test]
fn edit_write_with_media_missing_bag_image_errors() {
    let dir = tempdir().unwrap();
    let path = sample_note(dir.path());
    let read = edit_read(&path).unwrap();
    let base = with_extra_ids(read.tessprek.trim_end(), &[10]);
    let tessprek = format!("{base}\n\\figure{{image=900001 placement=flow alt=\"Missing\"}}\n");
    let err = edit_write_with_media(
        &path,
        &tessprek,
        &EditWriteOptions::new(read.source_hash, false),
        EditMediaBag::default(),
    )
    .unwrap_err();
    assert!(matches!(err, TesError::EditOp { .. }));
}

#[test]
fn apply_ops_catalog_clear_fields_dry_run_unchanged() {
    let dir = tempdir().unwrap();
    let path = sample_note(dir.path());
    apply_json(
        &path,
        r#"[{"op":"set_aliases","aliases":["Keep"]},{"op":"set_slug","slug":"keep-slug"},{"op":"set_category","category":"KeepCat"},{"op":"set_section","section":"Keep/Sec"},{"op":"set_tags","tags":["KeepTag"]}]"#,
        false,
    );
    let report = apply_json(
        &path,
        r#"[{"op":"set_aliases","aliases":[]},{"op":"set_slug","slug":null},{"op":"set_category","category":null},{"op":"set_section","section":null},{"op":"set_tags","tags":[]}]"#,
        true,
    );
    assert!(!report.replaced);
    let cat = TesFile::open(&path).unwrap().catalog().unwrap().clone();
    assert_meta(
        &cat.aliases,
        &cat.slug,
        &cat.category,
        &cat.section,
        &["Keep"],
        Some("keep-slug"),
        Some("KeepCat"),
        Some("Keep/Sec"),
    );
    assert_eq!(cat.tags, vec!["KeepTag"]);
}
