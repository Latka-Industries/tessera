//! Open-format conformance: must-accept / must-reject fixtures.

use std::path::{Path, PathBuf};

use crate::io::export::{ExportOptions, ExportView, export_view};
use crate::verify::verify_tes_file;

fn conformance_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/conformance")
}

fn list_tes(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("read {}: {err}", dir.display()))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("tes"))
        .collect();
    paths.sort();
    paths
}

#[test]
fn must_accept_deep_verify() {
    let accept = conformance_root().join("accept");
    let paths = list_tes(&accept);
    assert!(
        !paths.is_empty(),
        "expected accept fixtures under {}",
        accept.display()
    );
    for path in paths {
        let report = verify_tes_file(&path, true).unwrap_or_else(|err| {
            panic!("{}: open/verify error: {err}", path.display());
        });
        assert!(
            report.ok,
            "{} should accept, findings={:?}",
            path.display(),
            report.findings
        );
    }
}

#[test]
fn must_reject_verify() {
    let reject = conformance_root().join("reject");
    let paths = list_tes(&reject);
    assert!(
        !paths.is_empty(),
        "expected reject fixtures under {}",
        reject.display()
    );
    for path in paths {
        if let Ok(report) = verify_tes_file(&path, false) {
            assert!(
                !report.ok,
                "{} should reject, but verify reported ok",
                path.display()
            );
        }
    }
}

#[test]
fn note_one_chunk_raw_export_stable() {
    let path = conformance_root().join("accept/note_one_chunk.tes");
    let out = export_view(&path, ExportView::Raw, &ExportOptions::default()).unwrap();
    assert_eq!(out.trim_end(), "Hello from Tessera.");
}
