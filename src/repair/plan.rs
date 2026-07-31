//! Repair plans derived from [`TesVerifyReport`] findings.

use std::path::Path;

use serde::Serialize;

use crate::verify::TesVerifyReport;

use super::{is_repairable_code, repair_command_line};

/// One planned repair action.
#[derive(Debug, Clone, Serialize)]
pub struct RepairAction {
    /// Stable repair code (or verify check id when unrepairable).
    pub code: String,
    /// Whether an automatic repair exists.
    pub repairable: bool,
    /// Human summary (includes suggested CLI when repairable).
    pub summary: String,
}

/// Planned repairs for a file.
#[derive(Debug, Clone, Serialize)]
pub struct RepairPlan {
    /// Path that was verified.
    pub path: String,
    /// Actions in stable order (footer before drop).
    pub actions: Vec<RepairAction>,
}

/// Map a verify `check` id to a repair code, if any.
#[must_use]
pub fn code_for_check(check: &str) -> Option<&'static str> {
    match check {
        "history.footer" | "history.decode" => Some("footer_invalid"),
        "chunk.payload_bounds" => Some("drop_oob_chunks"),
        _ => None,
    }
}

/// Build a repair plan from a verify report.
#[must_use]
pub fn repair_plan_from_verify(path: &Path, verify: &TesVerifyReport) -> RepairPlan {
    let path_s = path.display().to_string();
    let mut seen = std::collections::BTreeSet::new();
    let mut actions = Vec::new();

    for finding in &verify.findings {
        if finding.severity != crate::verify::Severity::Error {
            continue;
        }
        if let Some(code) = code_for_check(&finding.check) {
            if !seen.insert(code.to_owned()) {
                continue;
            }
            let summary = format!(
                "{} — try: {}",
                finding.message,
                repair_command_line(path, code, true)
            );
            actions.push(RepairAction {
                code: code.to_owned(),
                repairable: is_repairable_code(code),
                summary,
            });
        } else if seen.insert(finding.check.clone()) {
            actions.push(RepairAction {
                code: finding.check.clone(),
                repairable: false,
                summary: finding.message.clone(),
            });
        }
    }

    // Stable preference: footer before drop.
    actions.sort_by_key(|a| match a.code.as_str() {
        "footer_invalid" => 0,
        "drop_oob_chunks" => 1,
        _ => 2,
    });

    RepairPlan {
        path: path_s,
        actions,
    }
}
