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
fn must_reject_deep_verify() {
    let reject = conformance_root().join("reject");
    let paths = list_tes(&reject);
    assert!(
        !paths.is_empty(),
        "expected reject fixtures under {}",
        reject.display()
    );
    for path in paths {
        if let Ok(report) = verify_tes_file(&path, true) {
            assert!(
                !report.ok,
                "{} should reject under deep verify, but verify reported ok",
                path.display()
            );
        }
    }
}

#[test]
fn note_one_chunk_raw_export_stable() {
    let path = conformance_root().join("accept/note_one_chunk.tes");
    let out = export_view(&path, ExportView::Raw, &ExportOptions::default()).unwrap();
    assert_eq!(
        out.trim_end(),
        "Hello from Tessera — use tes textconv for readable diffs."
    );
}

#[test]
fn layout_v1_text_markdown_export_snapshot() {
    let path = conformance_root().join("accept/layout_v1_text.tes");
    let out = export_view(&path, ExportView::Markdown, &ExportOptions::default()).unwrap();
    assert!(
        out.contains("**Strong**") || out.contains("*emphasis*") || out.contains("Strong"),
        "expected spanned prose, got:\n{out}"
    );
    assert!(
        out.contains("E = mc^2") || out.contains("$$"),
        "expected math projection, got:\n{out}"
    );
    assert!(
        out.contains("rust") || out.contains("fn main"),
        "expected code block, got:\n{out}"
    );
    assert!(
        out.contains("mermaid") || out.contains("flowchart"),
        "expected mermaid code block, got:\n{out}"
    );
    assert!(
        out.contains('|') || out.contains("A"),
        "expected table projection, got:\n{out}"
    );
    assert!(
        out.contains("**Relativity**")
            && out.contains("**Listing 1**")
            && out.contains("**Pipeline**")
            && out.contains("**Features**"),
        "expected bold title lines above blocks, got:\n{out}"
    );
    assert!(
        out.contains("*Mass–energy equivalence*")
            && out.contains("*Hello Tessera*")
            && out.contains("*Authoring flow*")
            && out.contains("*Layout feature ids*"),
        "expected italic caption lines under blocks, got:\n{out}"
    );
}

#[test]
fn layout_v1_text_html_export_captions() {
    let path = conformance_root().join("accept/layout_v1_text.tes");
    let out = export_view(&path, ExportView::Html, &ExportOptions::default()).unwrap();
    assert!(
        out.contains("<p class=\"tes-title\">Features</p>")
            && out.contains("<p class=\"tes-caption\">Layout feature ids</p>"),
        "expected table title above + caption below, got:\n{out}"
    );
    assert!(
        out.contains("<p class=\"tes-title\">Relativity</p>")
            && out.contains("<p class=\"tes-caption\">Mass–energy equivalence</p>")
            && out.contains("<p class=\"tes-title\">Listing 1</p>")
            && out.contains("<p class=\"tes-caption\">Hello Tessera</p>"),
        "expected math/code titles above + captions below, got:\n{out}"
    );
}
