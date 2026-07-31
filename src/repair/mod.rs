//! In-place / rewrite repairs for damaged `.tes` containers (`tes repair`).
//!
//! [`crate::verify`] stays read-only. Mutations happen only here. Never invent
//! semantic content — prefer clearing bad history flags or dropping out-of-bounds
//! chunks and rewriting a sealed body.
//!
//! ```text
//! tes repair doc.tes                 → plan (default)
//! tes repair doc.tes --apply-all --dry-run
//! tes repair doc.tes --apply footer_invalid
//! tes repair truncated.tes --apply drop_oob_chunks -o fixed.tes
//! ```

mod actions;
mod format;
mod plan;

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::Result;
use crate::verify::{TesVerifyReport, verify_tes_file};

pub use format::{format_plan_json, format_plan_text, format_repair_json, format_repair_text};
pub use plan::{RepairAction, RepairPlan, repair_plan_from_verify};

/// Options for [`repair_tes_file`].
#[derive(Debug, Clone, Default)]
pub struct RepairOptions {
    /// Report planned / would-apply actions without writing.
    pub dry_run: bool,
    /// Explicit codes to apply. Empty + `apply_all` false → plan only.
    pub apply: Vec<String>,
    /// Apply every repairable code from the verify plan.
    pub apply_all: bool,
    /// Write repaired bytes here instead of replacing `path` in place.
    pub output: Option<PathBuf>,
}

/// Outcome of one repair action.
#[derive(Debug, Clone, Serialize)]
pub struct RepairActionResult {
    /// Repair code (e.g. `footer_invalid`).
    pub code: String,
    /// Whether the file was mutated.
    pub applied: bool,
    /// Whether this was a dry-run.
    pub dry_run: bool,
    /// Human-readable result.
    pub message: String,
}

/// Full repair run result.
#[derive(Debug, Clone, Serialize)]
pub struct TesRepairReport {
    /// Input path.
    pub path: PathBuf,
    /// Output path written (`None` on plan-only / dry-run).
    pub output: Option<PathBuf>,
    /// Whether writes were suppressed.
    pub dry_run: bool,
    /// Plan-only (no `--apply` / `--apply-all`).
    pub plan_only: bool,
    /// Per-action results (empty when plan-only — see [`TesRepairReport::plan`]).
    pub actions: Vec<RepairActionResult>,
    /// Planned actions derived from verify.
    pub plan: RepairPlan,
    /// Re-verify `ok` after apply (`None` on dry-run / plan-only).
    pub verify_after_ok: Option<bool>,
    /// Verify findings that remain unrepairable.
    pub unrecoverable: Vec<String>,
}

/// Build a `tes repair …` command line for scripts (includes `--dry-run`).
#[must_use]
pub fn repair_command_line(path: &Path, code: &str, dry_run: bool) -> String {
    let p = path.display();
    if dry_run {
        format!("tes repair {p} --apply {code} --dry-run")
    } else {
        format!("tes repair {p} --apply {code}")
    }
}

/// Whether this repair code has an in-place / rewrite implementation.
#[must_use]
pub fn is_repairable_code(code: &str) -> bool {
    matches!(code, "footer_invalid" | "drop_oob_chunks")
}

/// Suggested CLI command for a verify check id, if repairable.
#[must_use]
pub fn repair_command_for_check(path: &Path, check: &str) -> Option<String> {
    let code = plan::code_for_check(check)?;
    Some(repair_command_line(path, code, true))
}

/// Plan repairs from a verify report (no mutation).
#[must_use]
pub fn repair_plan(path: &Path, verify: &TesVerifyReport) -> RepairPlan {
    repair_plan_from_verify(path, verify)
}

/// Run repairs on a `.tes` file.
///
/// Default (no `--apply` / `--apply-all`): return a plan without writing.
///
/// # Errors
///
/// I/O or structural errors from repair actions / verify open.
pub fn repair_tes_file(path: &Path, options: &RepairOptions) -> Result<TesRepairReport> {
    let verify = verify_tes_file(path, true)?;
    let plan = repair_plan_from_verify(path, &verify);
    let unrecoverable: Vec<String> = plan
        .actions
        .iter()
        .filter(|a| !a.repairable)
        .map(|a| format!("{}: {}", a.code, a.summary))
        .collect();

    let plan_only = options.apply.is_empty() && !options.apply_all;
    if plan_only {
        return Ok(TesRepairReport {
            path: path.to_path_buf(),
            output: None,
            dry_run: true,
            plan_only: true,
            actions: Vec::new(),
            plan,
            verify_after_ok: None,
            unrecoverable,
        });
    }

    let codes: Vec<String> = if options.apply_all {
        plan.actions
            .iter()
            .filter(|a| a.repairable)
            .map(|a| a.code.clone())
            .collect()
    } else {
        options.apply.clone()
    };

    // Prefer a single rewrite when drop_oob is requested; footer clear can run
    // inside the rewrite path. Apply footer-only when that is the sole code.
    let mut actions = Vec::new();
    let target = options.output.as_deref().unwrap_or(path);
    let mut working = std::fs::read(path)?;

    let want_drop = codes.iter().any(|c| c == "drop_oob_chunks");
    let want_footer = codes.iter().any(|c| c == "footer_invalid");

    if want_footer && !want_drop {
        actions.push(actions::apply_footer_invalid(
            &mut working,
            target,
            options.dry_run,
        )?);
    }

    if want_drop {
        actions.push(actions::apply_drop_oob_chunks(
            &mut working,
            target,
            options.dry_run,
            want_footer,
        )?);
    }

    for code in &codes {
        if code != "footer_invalid" && code != "drop_oob_chunks" {
            actions.push(RepairActionResult {
                code: code.clone(),
                applied: false,
                dry_run: options.dry_run,
                message: if is_repairable_code(code) {
                    "skipped".to_owned()
                } else {
                    "no in-place repair for this code; rewrite or re-import required".to_owned()
                },
            });
        }
    }

    let verify_after_ok = if options.dry_run {
        None
    } else {
        let check_path = if options.output.is_some() {
            target
        } else {
            path
        };
        Some(verify_tes_file(check_path, true)?.ok)
    };

    Ok(TesRepairReport {
        path: path.to_path_buf(),
        output: if options.dry_run {
            None
        } else {
            Some(target.to_path_buf())
        },
        dry_run: options.dry_run,
        plan_only: false,
        actions,
        plan,
        verify_after_ok,
        unrecoverable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn reject(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/conformance/reject")
            .join(name)
    }

    #[test]
    fn plan_history_flag_maps_footer_invalid() {
        let path = reject("history_flag_no_thst.tes");
        let report = repair_tes_file(&path, &RepairOptions::default()).unwrap();
        assert!(report.plan_only);
        assert!(
            report
                .plan
                .actions
                .iter()
                .any(|a| a.code == "footer_invalid" && a.repairable)
        );
    }

    #[test]
    fn apply_footer_invalid_makes_verify_ok() {
        let src = reject("history_flag_no_thst.tes");
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("fixed.tes");
        std::fs::copy(&src, &dest).unwrap();

        let report = repair_tes_file(
            &dest,
            &RepairOptions {
                apply: vec!["footer_invalid".into()],
                ..RepairOptions::default()
            },
        )
        .unwrap();
        assert_eq!(report.verify_after_ok, Some(true));
        assert!(report.actions.iter().any(|a| a.applied));
    }

    #[test]
    fn apply_drop_oob_on_truncated_makes_verify_ok() {
        let src = reject("truncated.tes");
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("fixed.tes");
        std::fs::copy(&src, &dest).unwrap();

        let report = repair_tes_file(
            &dest,
            &RepairOptions {
                apply: vec!["drop_oob_chunks".into()],
                ..RepairOptions::default()
            },
        )
        .unwrap();
        assert_eq!(report.verify_after_ok, Some(true), "{report:?}");
        assert!(
            report
                .actions
                .iter()
                .any(|a| a.code == "drop_oob_chunks" && a.applied)
        );
    }

    #[test]
    fn bad_magic_is_unrecoverable() {
        let path = reject("bad_magic.tes");
        let report = repair_tes_file(&path, &RepairOptions::default()).unwrap();
        assert!(
            report
                .plan
                .actions
                .iter()
                .any(|a| a.code.starts_with("superblock") && !a.repairable)
        );
    }
}
