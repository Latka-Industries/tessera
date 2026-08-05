//! Tessprek text diff for edit-write reports.

use std::fmt::Write as _;

use super::markers;

pub(super) fn normalize_tessprek_for_diff(text: &str) -> String {
    text.lines()
        .map(|line| {
            if line.starts_with(markers::TESSERA_PREFIX) && line.contains("source-hash=") {
                // Ignore hash churn from re-encoding into a temp file.
                format!(
                    "{}format={} version={} source-hash=<hash>{}",
                    markers::TESSERA_PREFIX,
                    markers::FORMAT,
                    markers::VERSION,
                    markers::BRACE_SUFFIX,
                )
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn simple_diff(before: &str, after: &str) -> String {
    if before == after {
        return String::from("(no changes)\n");
    }
    let mut out = String::from("--- before\n+++ after\n");
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    let max = before_lines.len().max(after_lines.len());
    for i in 0..max {
        let a = before_lines.get(i).copied();
        let b = after_lines.get(i).copied();
        match (a, b) {
            (Some(a), Some(b)) if a == b => {}
            (Some(a), Some(b)) => {
                let _ = writeln!(out, "-{a}");
                let _ = writeln!(out, "+{b}");
            }
            (Some(a), None) => {
                let _ = writeln!(out, "-{a}");
            }
            (None, Some(b)) => {
                let _ = writeln!(out, "+{b}");
            }
            (None, None) => {}
        }
    }
    out
}
