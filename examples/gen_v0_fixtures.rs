//! Regenerate golden `.tes` fixtures under `fixtures/v0/`.
//!
//! ```bash
//! cargo run --example gen_v0_fixtures
//! ```
//!
//! Values are fixed so byte-exact CI tests stay stable.

use std::fs;
use std::path::PathBuf;

use tessera_doc::catalog::{DocumentCatalog, LinkEntry, LinkKind, TesWriterSession, TextHeader};
use tessera_doc::layout::DocKind;
use uuid::Uuid;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/v0")
}

fn write_empty(dir: &std::path::Path) {
    let path = dir.join("empty.tes");
    let _ = fs::remove_file(&path);
    TesWriterSession::create(&path, DocKind::Note)
        .commit()
        .expect("write empty.tes");
    println!("wrote {}", path.display());
}

fn write_note_one_chunk(dir: &std::path::Path) {
    let path = dir.join("note_one_chunk.tes");
    let _ = fs::remove_file(&path);
    let mut session = TesWriterSession::create(&path, DocKind::Note);
    session
        .set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440000",
            "Meeting notes",
            "2026-06-05T12:00:00Z",
            "2026-06-05T12:30:00Z",
            DocKind::Note,
        ))
        .expect("catalog");
    session
        .add_text_chunk(&TextHeader::paragraph(), "Hello from Tessera.")
        .expect("chunk");
    session.commit().expect("write note_one_chunk.tes");
    println!("wrote {}", path.display());
}

fn write_hub_links(dir: &std::path::Path) {
    let path = dir.join("hub_links.tes");
    let _ = fs::remove_file(&path);
    let mut session = TesWriterSession::create(&path, DocKind::Hub);
    session
        .set_catalog(DocumentCatalog::new(
            "770e8400-e29b-41d4-a716-446655440002",
            "Fixture hub",
            "2026-06-05T12:00:00Z",
            "2026-06-05T12:30:00Z",
            DocKind::Hub,
        ))
        .expect("catalog");
    session
        .add_text_chunk(&TextHeader::paragraph(), "Meeting notes")
        .expect("chunk");
    session
        .add_link(LinkEntry::new(
            1,
            0,
            13,
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").expect("uuid"),
            1,
            LinkKind::Wiki,
        ))
        .expect("link");
    session.commit().expect("write hub_links.tes");
    println!("wrote {}", path.display());
}

fn main() {
    let dir = fixtures_dir();
    fs::create_dir_all(&dir).expect("fixtures/v0");
    write_empty(&dir);
    write_note_one_chunk(&dir);
    write_hub_links(&dir);
}
