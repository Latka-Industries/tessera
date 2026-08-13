//! Regenerate layout-v1 / attachment / feature-flag must-reject (and feature
//! must-accept) conformance fixtures.
//!
//! These use [`TesWriterSession::add_payload_chunk`] so invalid payloads can be
//! sealed without writer-side validation. Structural rejects
//! (`bad_magic.tes`, …) are regenerated with
//! `uv run scripts/gen_structural_rejects.py`.
//!
//! ```bash
//! cargo run --example gen_conformance_rejects
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use tessera_doc::catalog::index::{ChunkType, chunk_flags};
use tessera_doc::catalog::{
    DocumentCatalog, FeatureSet, TEXT_HEADER_MAX_BYTES, TesWriterSession, TextHeader,
    encode_u32_prefixed,
};
use tessera_doc::layout::DocKind;

fn reject_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/conformance/reject")
}

fn accept_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/conformance/accept")
}

fn write_reject(
    name: &str,
    title: &str,
    chunk_type: ChunkType,
    chunk_flags: u32,
    payload: Vec<u8>,
) {
    let path = reject_dir().join(name);
    let _ = fs::remove_file(&path);
    let mut session = TesWriterSession::create(&path, DocKind::Note);
    session
        .set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440080",
            title,
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Note,
        ))
        .expect("catalog");
    session
        .add_payload_chunk(chunk_type, chunk_flags, payload)
        .expect("payload");
    session.commit().expect("commit");
    println!("wrote {}", path.display());
}

fn oversized_table_header_json() -> String {
    let pad = "x".repeat(TEXT_HEADER_MAX_BYTES);
    let header = format!(
        "{{\"role\":\"table\",\"table\":{{\"rows\":[{{\"cells\":[{{\"text\":\"{pad}\",\"is_header\":true}}]}}]}}}}"
    );
    assert!(
        header.len() > TEXT_HEADER_MAX_BYTES,
        "fixture header must exceed TEXT_HEADER_MAX_BYTES"
    );
    header
}

fn main() {
    fs::create_dir_all(reject_dir()).expect("reject dir");
    fs::create_dir_all(accept_dir()).expect("accept dir");

    // Body is 5 bytes; end=99 is out of bounds.
    write_reject(
        "span_oob.tes",
        "Reject: span out of bounds",
        ChunkType::Text,
        chunk_flags::READING_ORDER,
        encode_u32_prefixed(
            br#"{"role":"paragraph","spans":[{"start":0,"end":99,"kind":"strong"}]}"#,
            b"hello",
        ),
    );

    // Nested span must be fully inside outer; 2..8 escapes 0..6.
    write_reject(
        "span_partial_overlap.tes",
        "Reject: partial span overlap",
        ChunkType::Text,
        chunk_flags::READING_ORDER,
        encode_u32_prefixed(
            br#"{"role":"paragraph","spans":[{"start":0,"end":6,"kind":"strong"},{"start":2,"end":8,"kind":"emphasis"}]}"#,
            b"abcdefghij",
        ),
    );

    write_reject(
        "table_rowspan_zero.tes",
        "Reject: table rowspan 0",
        ChunkType::Text,
        chunk_flags::READING_ORDER,
        encode_u32_prefixed(
            br#"{"role":"table","table":{"rows":[{"cells":[{"text":"A","is_header":true,"rowspan":0}]}]}}"#,
            b"",
        ),
    );

    // Captions are only valid on table / math / code_block.
    write_reject(
        "caption_on_paragraph.tes",
        "Reject: caption on paragraph",
        ChunkType::Text,
        chunk_flags::READING_ORDER,
        encode_u32_prefixed(br#"{"role":"paragraph","caption":"nope"}"#, b"hello"),
    );

    let oversized = oversized_table_header_json();
    write_reject(
        "oversized_text_header.tes",
        "Reject: oversized text header / table",
        ChunkType::Text,
        chunk_flags::READING_ORDER,
        encode_u32_prefixed(oversized.as_bytes(), b""),
    );

    // Path traversal basename — deep verify must fail on normalize.
    write_reject(
        "unsafe_attachment_filename.tes",
        "Reject: unsafe attachment filename",
        ChunkType::Attachment,
        0,
        encode_u32_prefixed(
            br#"{"media_type":"application/pdf","filename":"../evil.pdf","sha256":"0000000000000000000000000000000000000000000000000000000000000000"}"#,
            b"%PDF-1.4 bad name",
        ),
    );

    // Unknown must-understand feature → verify fail (layout_version stays 0).
    {
        let mut features = FeatureSet::default();
        features.declare_required("encrypted_payload");
        write_feature_note(
            &reject_dir(),
            "unknown_required_feature.tes",
            "550e8400-e29b-41d4-a716-446655440081",
            "Reject: unknown required feature",
            features,
        );
    }

    // Unknown optional feature → warn but accept.
    {
        let mut features = FeatureSet::default();
        features.declare_optional("future_widget");
        write_feature_note(
            &accept_dir(),
            "unknown_optional_feature.tes",
            "550e8400-e29b-41d4-a716-446655440082",
            "Accept: unknown optional feature",
            features,
        );
    }
}

fn write_feature_note(dir: &Path, name: &str, doc_id: &str, title: &str, features: FeatureSet) {
    let path = dir.join(name);
    let _ = fs::remove_file(&path);
    let mut session = TesWriterSession::create(&path, DocKind::Note);
    let mut cat = DocumentCatalog::new(
        doc_id,
        title,
        "2026-07-28T00:00:00Z",
        "2026-07-28T00:00:00Z",
        DocKind::Note,
    );
    cat.features = features;
    session.set_catalog(cat).expect("catalog");
    session
        .add_text_chunk(&TextHeader::paragraph(), "body")
        .expect("text");
    session.commit().expect("commit");
    println!("wrote {}", path.display());
}
