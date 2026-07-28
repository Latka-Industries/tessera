//! Regenerate layout-v1 / attachment must-reject conformance fixtures.
//!
//! These use [`TesWriterSession::add_payload_chunk`] so invalid payloads can be
//! sealed without writer-side validation. Structural rejects
//! (`bad_magic.tes`, …) stay in the Python snippet in
//! `fixtures/conformance/README.md`.
//!
//! ```bash
//! cargo run --example gen_conformance_rejects
//! ```

use std::fs;
use std::path::PathBuf;

use tessera_doc::catalog::index::{ChunkType, chunk_flags};
use tessera_doc::catalog::{
    DocumentCatalog, TEXT_HEADER_MAX_BYTES, TesWriterSession, encode_u32_prefixed,
};
use tessera_doc::layout::DocKind;

fn reject_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/conformance/reject")
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
}
