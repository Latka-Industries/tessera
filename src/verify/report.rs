//! Verification findings, report model, and CLI formatters.

use std::fmt::Write as _;

use serde::Serialize;

use crate::error::Result;

/// Severity of a single verification finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// File violates the v0 spec; `tes verify` exits 1.
    Error,
    /// Suspicious but readable.
    Warning,
    /// Informational note.
    Info,
}

/// One check outcome.
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    /// Machine-readable check id (e.g. `superblock.magic`).
    pub check: String,
    /// Severity of the outcome.
    pub severity: Severity,
    /// Human-readable explanation.
    pub message: String,
}

impl Finding {
    pub(crate) fn error(check: &str, message: impl Into<String>) -> Self {
        Self {
            check: check.to_owned(),
            severity: Severity::Error,
            message: message.into(),
        }
    }

    pub(crate) fn warning(check: &str, message: impl Into<String>) -> Self {
        Self {
            check: check.to_owned(),
            severity: Severity::Warning,
            message: message.into(),
        }
    }
}

/// Full verification report for one `.tes` file.
#[derive(Debug, Clone, Serialize)]
pub struct TesVerifyReport {
    /// Path that was checked.
    pub path: String,
    /// File length in bytes.
    pub file_len: u64,
    /// Whether every check passed (no [`Severity::Error`] findings).
    pub ok: bool,
    /// Number of chunk index rows parsed (best effort).
    pub chunk_count: u64,
    /// Whether payload bytes were decoded (`--deep`).
    pub deep: bool,
    /// All findings, in check order.
    pub findings: Vec<Finding>,
}

impl TesVerifyReport {
    /// Findings with [`Severity::Error`].
    #[must_use]
    pub fn errors(&self) -> Vec<&Finding> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .collect()
    }
}

/// Human-readable checklist for `tes verify`.
#[must_use]
pub fn format_verify_human(report: &TesVerifyReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "path:    {}", report.path);
    let _ = writeln!(out, "bytes:   {}", report.file_len);
    let _ = writeln!(out, "chunks:  {}", report.chunk_count);
    let _ = writeln!(
        out,
        "mode:    {}",
        if report.deep { "deep" } else { "basic" }
    );
    if report.findings.is_empty() {
        let _ = writeln!(out, "findings: none");
    } else {
        let _ = writeln!(out, "findings:");
        for f in &report.findings {
            let tag = match f.severity {
                Severity::Error => "ERROR",
                Severity::Warning => "WARN ",
                Severity::Info => "INFO ",
            };
            let _ = writeln!(out, "  [{tag}] {}: {}", f.check, f.message);
        }
    }
    let _ = writeln!(out, "status:  {}", if report.ok { "ok" } else { "failed" });
    out
}

/// One-line quiet form: `status=ok` or `status=failed`.
#[must_use]
pub fn format_verify_quiet(report: &TesVerifyReport) -> String {
    format!("status={}", if report.ok { "ok" } else { "failed" })
}

/// JSON report for `tes verify --json`.
///
/// # Errors
///
/// Returns [`crate::error::TesError::Json`] if serialization fails.
pub fn format_verify_json(report: &TesVerifyReport) -> Result<String> {
    Ok(serde_json::to_string_pretty(report)?)
}
