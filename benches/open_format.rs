//! Claim-backed microbenchmarks for open-format evidence (THI-185).
//!
//! Axes: partial chunk read, vault backlinks, Markdown import, raw export.
//! Run: `cargo bench -p tessera-doc --bench open_format`

use std::hint::black_box;
use std::path::PathBuf;

use criterion::{Criterion, criterion_group, criterion_main};
use tempfile::tempdir;
use tessera_doc::catalog::{
    DocumentCatalog, LinkEntry, LinkKind, TesFile, TesWriterSession, TextHeader,
};
use tessera_doc::io::export::{ExportOptions, ExportView, export_view};
use tessera_doc::io::import::{MarkdownImportOptions, import_markdown_v0};
use tessera_doc::layout::DocKind;
use tessera_doc::vault::Vault;
use uuid::Uuid;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

fn bench_partial_chunk_read(c: &mut Criterion) {
    let path = fixture("v0/note_one_chunk.tes");
    c.bench_function("mmap_open_and_decode_chunk_1", |b| {
        b.iter(|| {
            let file = TesFile::open(black_box(&path)).unwrap();
            let entry = file.chunk_by_id(1).unwrap();
            let bytes = file.decode_payload(entry).unwrap();
            black_box(bytes.len());
        });
    });
}

fn bench_export_raw(c: &mut Criterion) {
    let path = fixture("v0/note_one_chunk.tes");
    c.bench_function("export_raw_note_one_chunk", |b| {
        b.iter(|| {
            let out =
                export_view(black_box(&path), ExportView::Raw, &ExportOptions::default()).unwrap();
            black_box(out.len());
        });
    });
}

fn bench_import_markdown(c: &mut Criterion) {
    let md = fixture("assets/markdown/minimal.md");
    c.bench_function("import_markdown_minimal", |b| {
        b.iter(|| {
            let dir = tempdir().unwrap();
            let out = dir.path().join("out.tes");
            let report = import_markdown_v0(
                black_box(&md),
                &out,
                &MarkdownImportOptions {
                    doc_kind: DocKind::Document,
                    title: None,
                    doc_id: None,
                },
            )
            .unwrap();
            black_box(report.chunk_count);
        });
    });
}

fn bench_vault_backlinks(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    let note_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();

    let note_path = dir.path().join("note.tes");
    let mut note = TesWriterSession::create(&note_path, DocKind::Note);
    note.set_catalog(DocumentCatalog::new(
        note_id.to_string(),
        "Note",
        "2026-07-27T00:00:00Z",
        "2026-07-27T00:00:00Z",
        DocKind::Note,
    ))
    .unwrap();
    note.add_text_chunk(&TextHeader::paragraph(), "body")
        .unwrap();
    note.commit().unwrap();

    for i in 0..8u8 {
        let path = dir.path().join(format!("hub{i}.tes"));
        let hub_id = Uuid::from_bytes([
            0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80,
            0x90, i,
        ]);
        let mut hub = TesWriterSession::create(&path, DocKind::Hub);
        hub.set_catalog(DocumentCatalog::new(
            hub_id.to_string(),
            format!("Hub {i}"),
            "2026-07-27T00:00:00Z",
            "2026-07-27T00:00:00Z",
            DocKind::Hub,
        ))
        .unwrap();
        hub.add_text_chunk(&TextHeader::paragraph(), "link hub")
            .unwrap();
        hub.add_link(LinkEntry::new(1, 0, 4, note_id, 0, LinkKind::Wiki))
            .unwrap();
        hub.commit().unwrap();
    }

    c.bench_function("vault_backlinks_8_hubs", |b| {
        b.iter(|| {
            let vault = Vault::open(black_box(dir.path())).unwrap();
            let links = vault.backlinks(note_id);
            black_box(links.len());
        });
    });
}

criterion_group!(
    benches,
    bench_partial_chunk_read,
    bench_export_raw,
    bench_import_markdown,
    bench_vault_backlinks
);
criterion_main!(benches);
