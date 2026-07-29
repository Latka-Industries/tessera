//! Byte-exact golden fixtures for layout v0 / additive layout-v1 text.
//!
//! Expected bytes come from [`crate::fixtures::v0`] (same builders as
//! `examples/gen_v0_fixtures`).

use std::fs;
use std::path::PathBuf;

use crate::fixtures::v0;
use crate::layout::{DocKind, SUPERBLOCK_LEN, SuperblockV0};
use crate::verify::verify_tes_file;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/v0")
}

fn assert_matches_encoder(name: &str, expected: Vec<u8>) {
    let path = fixtures_dir().join(name);
    let on_disk = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert_eq!(
        on_disk, expected,
        "{name} drifted — run: cargo run --example gen_v0_fixtures"
    );
    let report = verify_tes_file(&path, true).unwrap();
    assert!(report.ok, "{name} deep verify failed: {report:?}");
}

#[test]
fn empty_tes_matches_encoder() {
    let path = fixtures_dir().join("empty.tes");
    let on_disk = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let expected = v0::encode_empty();
    assert_eq!(on_disk.len(), SUPERBLOCK_LEN);
    assert_eq!(on_disk, expected);
    let sb = SuperblockV0::from_bytes(&on_disk).unwrap();
    assert_eq!(sb.doc_kind, DocKind::Note);
    assert!(!sb.catalog.is_present());
    assert!(!sb.chunk_index.is_present());
}

#[test]
fn note_one_chunk_tes_matches_encoder() {
    assert_matches_encoder("note_one_chunk.tes", v0::encode_note_one_chunk());
}

#[test]
fn note_three_chunks_tes_matches_encoder() {
    assert_matches_encoder("note_three_chunks.tes", v0::encode_note_three_chunks());
}

#[test]
fn hub_links_tes_matches_encoder() {
    assert_matches_encoder("hub_links.tes", v0::encode_hub_links());
    let on_disk = fs::read(fixtures_dir().join("hub_links.tes")).unwrap();
    let sb = SuperblockV0::from_bytes(&on_disk).unwrap();
    assert_eq!(sb.doc_kind, DocKind::Hub);
    assert!(sb.link_table.is_present());
}

#[test]
fn layout_v1_text_tes_matches_encoder() {
    assert_matches_encoder("layout_v1_text.tes", v0::encode_layout_v1_text());
}

#[test]
fn slide_deck_tes_matches_encoder() {
    assert_matches_encoder("slide_deck.tes", v0::encode_slide_deck());
}

#[test]
fn research_cite_tes_matches_encoder() {
    assert_matches_encoder("research_cite.tes", v0::encode_research_cite());
}

#[test]
fn figure_sample_tes_matches_encoder() {
    assert_matches_encoder("figure_sample.tes", v0::encode_figure_sample());
}

#[test]
fn attachment_sample_tes_matches_encoder() {
    assert_matches_encoder("attachment_sample.tes", v0::encode_attachment_sample());
}

#[test]
fn external_links_tes_matches_encoder() {
    assert_matches_encoder("external_links.tes", v0::encode_external_links());
    let on_disk = fs::read(fixtures_dir().join("external_links.tes")).unwrap();
    let sb = SuperblockV0::from_bytes(&on_disk).unwrap();
    assert!(sb.link_table.is_present());
    // TLNK v1: version byte after magic.
    let region = sb.link_table.slice(&on_disk, "link_table").unwrap();
    assert_eq!(&region[..4], b"TLNK");
    assert_eq!(region[4], 1);
}
