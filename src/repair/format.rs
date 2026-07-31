//! CLI / JSON formatters for repair plans and reports.

use std::fmt::Write as _;

use crate::error::Result;

use super::TesRepairReport;
use super::plan::RepairPlan;

/// Human-readable plan listing.
#[must_use]
pub fn format_plan_text(plan: &RepairPlan) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "path: {}", plan.path);
    if plan.actions.is_empty() {
        let _ = writeln!(out, "actions: none (verify clean or no mapped repairs)");
    } else {
        let _ = writeln!(out, "actions:");
        for a in &plan.actions {
            let tag = if a.repairable { "repair" } else { "manual" };
            let _ = writeln!(out, "  [{tag}] {}: {}", a.code, a.summary);
        }
    }
    out
}

/// JSON plan.
///
/// # Errors
///
/// Returns [`crate::error::TesError::Json`] if serialization fails.
pub fn format_plan_json(plan: &RepairPlan) -> Result<String> {
    Ok(serde_json::to_string_pretty(plan)?)
}

/// Human-readable repair run report.
#[must_use]
pub fn format_repair_text(report: &TesRepairReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "path: {}", report.path.display());
    if report.plan_only {
        // `format_plan_text` already includes path.
        let plan_body = format_plan_text(&report.plan);
        // Avoid double path line when we already printed path above.
        let rest = plan_body
            .strip_prefix(&format!("path: {}\n", report.path.display()))
            .unwrap_or(plan_body.as_str());
        out.push_str(rest);
        if !report.unrecoverable.is_empty() {
            let _ = writeln!(out, "unrecoverable:");
            for u in &report.unrecoverable {
                let _ = writeln!(out, "  - {u}");
            }
        }
        let _ = writeln!(
            out,
            "hint: re-run with --apply <code> or --apply-all (add --dry-run first)"
        );
        return out;
    }
    let _ = writeln!(out, "dry_run: {}", report.dry_run);
    if let Some(ref output) = report.output {
        let _ = writeln!(out, "output: {}", output.display());
    }
    let _ = writeln!(out, "actions:");
    for a in &report.actions {
        let tag = if a.applied {
            "applied"
        } else if a.dry_run {
            "would"
        } else {
            "skip"
        };
        let _ = writeln!(out, "  [{tag}] {}: {}", a.code, a.message);
    }
    if !report.unrecoverable.is_empty() {
        let _ = writeln!(out, "unrecoverable:");
        for u in &report.unrecoverable {
            let _ = writeln!(out, "  - {u}");
        }
    }
    if let Some(ok) = report.verify_after_ok {
        let _ = writeln!(out, "verify_after: {}", if ok { "ok" } else { "failed" });
    }
    out
}

/// JSON repair report.
///
/// # Errors
///
/// Returns [`crate::error::TesError::Json`] if serialization fails.
pub fn format_repair_json(report: &TesRepairReport) -> Result<String> {
    Ok(serde_json::to_string_pretty(report)?)
}
