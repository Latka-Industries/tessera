//! Verify + source-hash → LSP diagnostics (file-level for v1).

use std::path::Path;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

use crate::edit::file_source_hash;
use crate::verify::{Finding, Severity, verify_tes_file};

/// File-level range — v1 does not map every verify finding onto Tessprek spans.
fn file_level_range() -> Range {
    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line: 0,
            character: 1,
        },
    }
}

pub(super) fn file_diagnostic(
    severity: DiagnosticSeverity,
    code: &str,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic {
        range: file_level_range(),
        severity: Some(severity),
        code: Some(NumberOrString::String(code.to_owned())),
        source: Some("tes-lsp".into()),
        message: message.into(),
        ..Default::default()
    }
}

fn finding_to_diagnostic(finding: &Finding) -> Diagnostic {
    let severity = match finding.severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Info => DiagnosticSeverity::INFORMATION,
    };
    file_diagnostic(severity, &finding.check, finding.message.clone())
}

/// Build diagnostics from `verify_*` plus an optional source-hash expectation.
pub(super) fn collect_diagnostics(path: &Path, expected_hash: Option<&str>) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    if let Some(expected) = expected_hash {
        match file_source_hash(path) {
            Ok(found) if found != expected => {
                out.push(file_diagnostic(
                    DiagnosticSeverity::ERROR,
                    "source-hash",
                    format!(
                        "source-hash mismatch: expected {}, found {} (refusing silent overwrite)",
                        &expected[..expected.len().min(12)],
                        &found[..found.len().min(12)]
                    ),
                ));
            }
            Ok(_) => {}
            Err(err) => {
                out.push(file_diagnostic(
                    DiagnosticSeverity::ERROR,
                    "source-hash",
                    format!("source-hash check failed: {err}"),
                ));
            }
        }
    }

    match verify_tes_file(path, true) {
        Ok(report) => {
            out.extend(report.findings.iter().map(finding_to_diagnostic));
        }
        Err(err) => {
            out.push(file_diagnostic(
                DiagnosticSeverity::ERROR,
                "verify",
                format!("verify failed: {err}"),
            ));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_tes() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/v0/note_one_chunk.tes")
    }

    fn reject_bad_magic() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/conformance/reject/bad_magic.tes")
    }

    #[test]
    fn collect_diagnostics_clean_fixture_empty_or_ok() {
        let path = fixture_tes();
        let hash = file_source_hash(&path).unwrap();
        let diags = collect_diagnostics(&path, Some(&hash));
        assert!(
            diags
                .iter()
                .all(|d| d.severity != Some(DiagnosticSeverity::ERROR)),
            "unexpected errors: {diags:?}"
        );
    }

    #[test]
    fn collect_diagnostics_bad_magic_has_error() {
        let path = reject_bad_magic();
        let diags = collect_diagnostics(&path, None);
        assert!(
            diags.iter().any(|d| {
                d.severity == Some(DiagnosticSeverity::ERROR)
                    && matches!(&d.code, Some(NumberOrString::String(c)) if c.contains("magic") || c == "verify")
            }),
            "expected magic/verify error, got {diags:?}"
        );
    }

    #[test]
    fn collect_diagnostics_hash_mismatch() {
        let path = fixture_tes();
        let diags = collect_diagnostics(&path, Some("deadbeef"));
        assert!(
            diags.iter().any(|d| {
                d.severity == Some(DiagnosticSeverity::ERROR)
                    && d.code == Some(NumberOrString::String("source-hash".into()))
            }),
            "expected source-hash error, got {diags:?}"
        );
    }
}
