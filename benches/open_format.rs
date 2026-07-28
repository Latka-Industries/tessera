//! Claim-backed microbenchmarks (THI-185 / THI-200).
//!
//! Axes: mmap partial chunk read, vault backlinks, Markdown import, and
//! linear/HTML/(optional) PDF export — including long-fixture and Markdown
//! vault comparison cases.
//!
//! ```bash
//! cargo bench -p tessera-doc --bench open_format
//! # or: mise run bench
//! ```
//!
//! Criterion writes HTML reports under `target/criterion/`. Do not paste
//! unverified numbers into the README; link this harness instead.

use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use criterion::{Criterion, criterion_group, criterion_main};
use tempfile::{TempDir, tempdir};
use tessera_doc::catalog::{
    DocumentCatalog, LinkEntry, LinkKind, TesFile, TesWriterSession, TextHeader,
};
use tessera_doc::io::export::{ExportOptions, ExportView, export_view};
use tessera_doc::io::import::{MarkdownImportOptions, import_markdown_v0};
use tessera_doc::layout::DocKind;
use tessera_doc::render::pdf::{PdfExportOptions, export_pdf, find_chrome};
use tessera_doc::vault::Vault;
use uuid::Uuid;

const VAULT_N: u8 = 8;
const STAMP: &str = "2026-07-27T00:00:00Z";
const IMPORT_OPTS: MarkdownImportOptions = MarkdownImportOptions {
    doc_kind: DocKind::Document,
    title: None,
    doc_id: None,
};

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture(name: &str) -> PathBuf {
    crate_root().join("fixtures").join(name)
}

fn catalog(doc_id: impl Into<String>, title: impl Into<String>, kind: DocKind) -> DocumentCatalog {
    DocumentCatalog::new(doc_id, title, STAMP, STAMP, kind)
}

/// One-shot import of `lorem_long.md` (~900 KiB) into a process-lifetime temp `.tes`.
fn long_tes_path() -> &'static Path {
    static LONG: OnceLock<(TempDir, PathBuf)> = OnceLock::new();
    &LONG
        .get_or_init(|| {
            let dir = tempfile::Builder::new()
                .prefix("tessera-bench-long-")
                .tempdir()
                .expect("tempdir");
            let path = dir.path().join("lorem_long.tes");
            import_markdown_v0(
                &fixture("assets/markdown/lorem_long.md"),
                &path,
                &IMPORT_OPTS,
            )
            .expect("import lorem_long");
            (dir, path)
        })
        .1
}

fn decode_chunk_1(path: &Path) -> usize {
    let file = TesFile::open(path).unwrap();
    let entry = file.chunk_by_id(1).unwrap();
    file.decode_payload(entry).unwrap().len()
}

fn import_md_to_temp(md: &Path) -> usize {
    let dir = tempdir().unwrap();
    let out = dir.path().join("out.tes");
    import_markdown_v0(md, &out, &IMPORT_OPTS)
        .unwrap()
        .chunk_count
}

fn export_len(path: &Path, view: ExportView) -> usize {
    export_view(path, view, &ExportOptions::default())
        .unwrap()
        .len()
}

fn write_text_doc(path: &Path, doc_id: Uuid, title: &str, kind: DocKind, body: &str) {
    let mut session = TesWriterSession::create(path, kind);
    session
        .set_catalog(catalog(doc_id.to_string(), title, kind))
        .unwrap();
    session
        .add_text_chunk(&TextHeader::paragraph(), body)
        .unwrap();
    session.commit().unwrap();
}

fn write_hub_vault(dir: &Path, hub_count: u8) -> Uuid {
    let note_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
    write_text_doc(
        &dir.join("note.tes"),
        note_id,
        "Note",
        DocKind::Note,
        "body",
    );

    for i in 0..hub_count {
        let path = dir.join(format!("hub{i}.tes"));
        let hub_id = Uuid::from_bytes([
            0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80,
            0x90, i,
        ]);
        let mut hub = TesWriterSession::create(&path, DocKind::Hub);
        hub.set_catalog(catalog(
            hub_id.to_string(),
            format!("Hub {i}"),
            DocKind::Hub,
        ))
        .unwrap();
        hub.add_text_chunk(&TextHeader::paragraph(), "link hub")
            .unwrap();
        hub.add_link(LinkEntry::new(1, 0, 4, note_id, 0, LinkKind::Wiki))
            .unwrap();
        hub.commit().unwrap();
    }
    note_id
}

fn fill_copies(src: &Path, dest_dir: &Path, n: u8, name: &str, ext: &str) {
    for i in 0..n {
        fs::copy(src, dest_dir.join(format!("{name}{i}.{ext}"))).unwrap();
    }
}

fn import_copies(src_md: &Path, dest_dir: &Path, n: u8) {
    for i in 0..n {
        let out = dest_dir.join(format!("note{i}.tes"));
        import_markdown_v0(src_md, &out, &IMPORT_OPTS).unwrap();
    }
}

fn sum_ext_bytes(dir: &Path, ext: &str) -> usize {
    fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some(ext))
        .map(|p| fs::read_to_string(p).unwrap().len())
        .sum()
}

fn bench_mmap_partial(c: &mut Criterion) {
    let mut group = c.benchmark_group("mmap_partial_chunk");
    let small = fixture("v0/note_one_chunk.tes");
    let long = long_tes_path();

    group.bench_function("note_one_chunk_decode_chunk_1", |b| {
        b.iter(|| black_box(decode_chunk_1(black_box(&small))));
    });
    group.bench_function("lorem_long_decode_chunk_1", |b| {
        b.iter(|| black_box(decode_chunk_1(black_box(long))));
    });
    group.finish();
}

fn bench_import(c: &mut Criterion) {
    let mut group = c.benchmark_group("import_markdown");
    let minimal = fixture("assets/markdown/minimal.md");
    let long_md = fixture("assets/markdown/lorem_long.md");

    group.bench_function("minimal", |b| {
        b.iter(|| black_box(import_md_to_temp(black_box(&minimal))));
    });
    group.sample_size(20);
    group.bench_function("lorem_long", |b| {
        b.iter(|| black_box(import_md_to_temp(black_box(&long_md))));
    });
    group.finish();
}

fn bench_export(c: &mut Criterion) {
    let mut group = c.benchmark_group("export");
    let small = fixture("v0/note_one_chunk.tes");
    let long = long_tes_path();

    group.bench_function("raw_note_one_chunk", |b| {
        b.iter(|| black_box(export_len(black_box(&small), ExportView::Raw)));
    });
    group.bench_function("linear_lorem_long", |b| {
        b.iter(|| black_box(export_len(black_box(long), ExportView::Linear)));
    });
    group.bench_function("html_lorem_long", |b| {
        b.iter(|| black_box(export_len(black_box(long), ExportView::Html)));
    });

    if find_chrome().is_ok() {
        group.sample_size(10);
        let pdf_opts = PdfExportOptions {
            template_root: crate_root().join("templates"),
            ..PdfExportOptions::default()
        };
        group.bench_function("pdf_note_one_chunk", |b| {
            b.iter(|| {
                let dir = tempdir().unwrap();
                let out = dir.path().join("out.pdf");
                export_pdf(black_box(&small), &out, &pdf_opts).unwrap();
                black_box(fs::metadata(&out).unwrap().len());
            });
        });
    }
    group.finish();
}

fn bench_vault(c: &mut Criterion) {
    let mut group = c.benchmark_group("vault");

    let tes_dir = tempdir().unwrap();
    let note_id = write_hub_vault(tes_dir.path(), VAULT_N);
    group.bench_function("backlinks_8_hubs", |b| {
        b.iter(|| {
            let vault = Vault::open(black_box(tes_dir.path())).unwrap();
            black_box(vault.backlinks(note_id).len());
        });
    });

    let md_src = fixture("assets/markdown/rich_document.md");
    let md_dir = tempdir().unwrap();
    fill_copies(&md_src, md_dir.path(), VAULT_N, "note", "md");
    group.bench_function("markdown_vault_read_8_files", |b| {
        b.iter(|| black_box(sum_ext_bytes(black_box(md_dir.path()), "md")));
    });

    let imported = tempdir().unwrap();
    import_copies(&md_src, imported.path(), VAULT_N);
    group.bench_function("tes_vault_open_list_8_docs", |b| {
        b.iter(|| {
            let vault = Vault::open(black_box(imported.path())).unwrap();
            black_box(vault.documents().count());
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_mmap_partial,
    bench_import,
    bench_export,
    bench_vault
);
criterion_main!(benches);
