//! Verify + source-hash + Tessprek parse → LSP diagnostics.

use std::path::Path;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

use crate::edit::tessprek::TessprekDocMeta;
use crate::edit::{decode_tessprek, file_source_hash};
use crate::error::TesError;
use crate::verify::{Finding, Severity, verify_tes_file};

use super::position::line_column_range;

/// File-level range — used when a finding has no Tessprek span (verify / hash).
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

/// Ranged diagnostic for a Tessprek [`TesError::EditParse`].
pub(super) fn parse_diagnostic(
    text: &str,
    line: usize,
    column: usize,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic {
        range: line_column_range(text, line, column),
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(NumberOrString::String("edit-parse".into())),
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

/// Parse the in-memory Tessprek buffer; emit a ranged `edit-parse` on failure.
/// Unknown `\tessera{…}` keys become warnings (ignored on decode).
pub(super) fn collect_buffer_diagnostics(tessprek: &str) -> Vec<Diagnostic> {
    let mut out = match decode_tessprek(tessprek) {
        Ok(_) => Vec::new(),
        Err(TesError::EditParse {
            line,
            column,
            message,
        }) => {
            vec![parse_diagnostic(tessprek, line, column, message)]
        }
        Err(err) => {
            vec![file_diagnostic(
                DiagnosticSeverity::ERROR,
                "edit-parse",
                format!("Tessprek parse failed: {err}"),
            )]
        }
    };
    out.extend(unknown_header_key_warnings(tessprek));
    out
}

fn unknown_header_key_warnings(tessprek: &str) -> Vec<Diagnostic> {
    let Some((line, keys)) = TessprekDocMeta::unknown_keys_in_buffer(tessprek) else {
        return Vec::new();
    };
    keys.into_iter()
        .map(|key| Diagnostic {
            range: line_column_range(tessprek, line, 1),
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String("tessera-unknown-key".into())),
            source: Some("tes-lsp".into()),
            message: format!(
                "unknown `\\tessera{{}}` key `{key}` — remove it before write (does not update catalog)"
            ),
            ..Default::default()
        })
        .collect()
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

/// Buffer parse diagnostics plus on-disk verify / source-hash.
pub(super) fn collect_open_diagnostics(
    path: &Path,
    expected_hash: Option<&str>,
    tessprek: Option<&str>,
) -> Vec<Diagnostic> {
    let mut out = match tessprek {
        Some(text) => collect_buffer_diagnostics(text),
        None => Vec::new(),
    };
    out.extend(collect_diagnostics(path, expected_hash));
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

    #[test]
    fn buffer_parse_error_is_ranged_on_offending_line() {
        let text = "\
\\tessera{format=tessprek version=2}\n\
\\ids{1}\n\
\n\
\\figure{placement=flow alt=\"x\"}\n\
";
        let diags = collect_buffer_diagnostics(text);
        assert_eq!(diags.len(), 1, "{diags:?}");
        let d = &diags[0];
        assert_eq!(d.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(d.code, Some(NumberOrString::String("edit-parse".into())));
        assert!(
            d.message.contains("missing required attribute"),
            "{}",
            d.message
        );
        // Directive is on 1-based line 4 → LSP line 3; whole-line highlight.
        assert_eq!(d.range.start.line, 3);
        assert_eq!(d.range.start.character, 0);
        assert!(d.range.end.character > 1, "{:?}", d.range);
    }

    #[test]
    fn buffer_parse_clean_empty() {
        let text = "\
\\tessera{format=tessprek version=2}\n\
\\ids{1}\n\
\n\
Hello\n\
";
        assert!(collect_buffer_diagnostics(text).is_empty());
    }

    #[test]
    fn buffer_warns_on_unknown_tessera_key() {
        let text = "\
\\tessera{format=tessprek version=2 tags=nope}\n\
\\ids{1}\n\
\n\
Hello\n\
";
        let diags = collect_buffer_diagnostics(text);
        assert!(
            diags.iter().any(|d| {
                d.severity == Some(DiagnosticSeverity::ERROR)
                    && d.code == Some(NumberOrString::String("tessera-unknown-key".into()))
                    && d.message.contains("tags")
            }),
            "{diags:?}"
        );
    }
}
