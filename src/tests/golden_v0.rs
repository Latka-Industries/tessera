//! Byte-exact golden fixtures for layout v0.

use std::fs;
use std::path::PathBuf;

use crate::catalog::{DocumentCatalog, LinkEntry, LinkKind, TesWriterSession, TextHeader};
use crate::layout::{DocKind, SUPERBLOCK_LEN, SuperblockV0};
use uuid::Uuid;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/v0")
}

fn expected_empty() -> Vec<u8> {
    TesWriterSession::create("empty.tes", DocKind::Note)
        .encode_file()
        .unwrap()
}

fn expected_note_one_chunk() -> Vec<u8> {
    let mut session = TesWriterSession::create("note_one_chunk.tes", DocKind::Note);
    session
        .set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440000",
            "Meeting notes",
            "2026-06-05T12:00:00Z",
            "2026-06-05T12:30:00Z",
            DocKind::Note,
        ))
        .unwrap();
    session
        .add_text_chunk(&TextHeader::paragraph(), "Hello from Tessera.")
        .unwrap();
    session.encode_file().unwrap()
}

fn expected_hub_links() -> Vec<u8> {
    let mut session = TesWriterSession::create("hub_links.tes", DocKind::Hub);
    session
        .set_catalog(DocumentCatalog::new(
            "770e8400-e29b-41d4-a716-446655440002",
            "Fixture hub",
            "2026-06-05T12:00:00Z",
            "2026-06-05T12:30:00Z",
            DocKind::Hub,
        ))
        .unwrap();
    session
        .add_text_chunk(&TextHeader::paragraph(), "Meeting notes")
        .unwrap();
    session
        .add_link(LinkEntry::new(
            1,
            0,
            13,
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            1,
            LinkKind::Wiki,
        ))
        .unwrap();
    session.encode_file().unwrap()
}

#[test]
fn empty_tes_matches_encoder() {
    let path = fixtures_dir().join("empty.tes");
    let on_disk = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let expected = expected_empty();
    assert_eq!(on_disk.len(), SUPERBLOCK_LEN);
    assert_eq!(on_disk, expected);
    let sb = SuperblockV0::from_bytes(&on_disk).unwrap();
    assert_eq!(sb.doc_kind, DocKind::Note);
    assert!(!sb.catalog.is_present());
    assert!(!sb.chunk_index.is_present());
}

#[test]
fn note_one_chunk_tes_matches_encoder() {
    let path = fixtures_dir().join("note_one_chunk.tes");
    let on_disk = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let expected = expected_note_one_chunk();
    assert_eq!(
        on_disk, expected,
        "fixture drifted — run: cargo run --example gen_v0_fixtures"
    );
    let sb = SuperblockV0::from_bytes(&on_disk).unwrap();
    assert!(sb.catalog.is_present());
    assert!(sb.chunk_index.is_present());
}

#[test]
fn hub_links_tes_matches_encoder() {
    let path = fixtures_dir().join("hub_links.tes");
    let on_disk = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert_eq!(on_disk, expected_hub_links());
    let sb = SuperblockV0::from_bytes(&on_disk).unwrap();
    assert_eq!(sb.doc_kind, DocKind::Hub);
    assert!(sb.link_table.is_present());
}
