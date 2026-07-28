//! Layout health checks (`docs/layout_v0.md` — *File health*, `docs/cli.md` — `tes verify`).
//!
//! Unlike [`crate::catalog::file::TesFile`], which fails on the first structural
//! error, verification collects **findings** so a corrupt file yields a full
//! report. `tes verify` exits `1` when any finding is an error.
//!
//! - [`verify_tes_file`] / [`verify_bytes`] — run checks.
//! - [`TesVerifyReport`], [`Finding`], [`Severity`] — report model.
//! - [`format_verify_human`] / [`format_verify_quiet`] / [`format_verify_json`] — CLI output.

mod checks;
mod report;

pub use checks::{verify_bytes, verify_tes_file};
pub use report::{
    Finding, Severity, TesVerifyReport, format_verify_human, format_verify_json,
    format_verify_quiet,
};

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::catalog::{DocumentCatalog, TesWriterSession, TextHeader};
    use crate::layout::DocKind;

    fn note_bytes() -> Vec<u8> {
        let mut s = TesWriterSession::create("note.tes", DocKind::Note);
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
        s.encode_file().unwrap()
    }

    fn note_with_features(title: &str, features: crate::catalog::FeatureSet) -> Vec<u8> {
        let mut session = TesWriterSession::create("feat.tes", DocKind::Note);
        let mut cat = DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440000",
            title,
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Note,
        );
        cat.features = features;
        session.set_catalog(cat).unwrap();
        session
            .add_text_chunk(&TextHeader::paragraph(), "hi")
            .unwrap();
        session.encode_file().unwrap()
    }

    fn p() -> &'static Path {
        Path::new("mem.tes")
    }

    #[test]
    fn valid_note_passes_basic_and_deep() {
        let bytes = note_bytes();
        let basic = verify_bytes(p(), &bytes, false);
        assert!(basic.ok, "{:?}", basic.findings);
        assert_eq!(basic.chunk_count, 1);

        let deep = verify_bytes(p(), &bytes, true);
        assert!(deep.ok, "{:?}", deep.findings);
        assert!(deep.deep);
    }

    #[test]
    fn empty_skeleton_passes() {
        let bytes = TesWriterSession::create("empty.tes", DocKind::Note)
            .encode_file()
            .unwrap();
        let report = verify_bytes(p(), &bytes, true);
        assert!(report.ok, "{:?}", report.findings);
        assert_eq!(report.chunk_count, 0);
    }

    #[test]
    fn truncated_file_fails() {
        let bytes = note_bytes();
        let report = verify_bytes(p(), &bytes[..bytes.len() - 10], false);
        assert!(!report.ok);
        assert!(report.errors().iter().any(|f| f.check.starts_with("chunk")));
    }

    #[test]
    fn too_short_for_superblock_fails() {
        let report = verify_bytes(p(), &[0u8; 10], false);
        assert!(!report.ok);
        assert_eq!(report.errors()[0].check, "superblock.size");
    }

    #[test]
    fn bad_magic_fails() {
        let mut bytes = note_bytes();
        bytes[0] = b'X';
        let report = verify_bytes(p(), &bytes, false);
        assert!(!report.ok);
        assert_eq!(report.errors()[0].check, "superblock.magic");
    }

    #[test]
    fn corrupt_index_magic_fails() {
        let mut bytes = note_bytes();
        let sb = crate::layout::SuperblockV0::from_bytes(&bytes).unwrap();
        let off = sb.chunk_index.offset as usize;
        bytes[off] = b'Z';
        let report = verify_bytes(p(), &bytes, false);
        assert!(!report.ok);
        assert_eq!(report.errors()[0].check, "chunk_index.magic");
    }

    #[test]
    fn corrupt_catalog_json_fails() {
        let mut bytes = note_bytes();
        let sb = crate::layout::SuperblockV0::from_bytes(&bytes).unwrap();
        bytes[sb.catalog.offset as usize] = b'?';
        let report = verify_bytes(p(), &bytes, false);
        assert!(!report.ok);
        assert_eq!(report.errors()[0].check, "catalog.json");
    }

    #[test]
    fn history_flag_without_footer_fails() {
        let mut bytes = note_bytes();
        // Set flags bit 1 (HISTORY_FOOTER) at offset 8.
        bytes[8..12].copy_from_slice(&1u32.to_le_bytes());
        let report = verify_bytes(p(), &bytes, false);
        assert!(!report.ok);
        assert!(report.errors().iter().any(|f| f.check == "history.footer"));
    }

    #[test]
    fn unknown_optional_feature_warns_but_passes() {
        let mut features = crate::catalog::FeatureSet::default();
        features.declare_optional("future_widget");
        let report = verify_bytes(p(), &note_with_features("Optional unknown", features), true);
        assert!(report.ok, "{:?}", report.findings);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.check == "features.optional")
        );
    }

    #[test]
    fn unknown_required_feature_fails() {
        let mut features = crate::catalog::FeatureSet::default();
        features.declare_required("encrypted_payload");
        let report = verify_bytes(p(), &note_with_features("Required unknown", features), true);
        assert!(!report.ok);
        assert!(
            report
                .errors()
                .iter()
                .any(|f| f.check == "features.required")
        );
    }

    #[test]
    fn formatters_render() {
        let bytes = note_bytes();
        let report = verify_bytes(p(), &bytes, false);
        assert!(format_verify_human(&report).contains("status:  ok"));
        assert_eq!(format_verify_quiet(&report), "status=ok");
        assert!(
            format_verify_json(&report)
                .unwrap()
                .contains("\"ok\": true")
        );
    }
}
