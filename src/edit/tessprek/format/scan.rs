use std::collections::BTreeMap;

use crate::error::Result;

use super::super::markers::match_body_opener;
use super::super::{
    parse_attrs, scan_tessprek_preamble, skip_blank_lines, take_brace_command, take_row_panes,
    trim_block_body,
};

#[derive(Debug)]
pub(super) enum Segment {
    /// Free Markdown run, optionally preceded by `\block{…}` (attrs applied to
    /// the first resulting block). Lines are 0-based half-open `[start, end)`;
    /// Markdown body begins at `body_start` (after a multiline `\block{…}`).
    Markdown {
        start: usize,
        body_start: usize,
        end: usize,
        preserve: Option<BTreeMap<String, String>>,
    },
    /// A brace-command directive (`figure` / `cite` / `slide` / `attachment`).
    Directive {
        start: usize,
        end: usize,
        kind: String,
        map: BTreeMap<String, String>,
        body: String,
    },
    /// Tessprek `\row{pane}{pane}…` (2+ consecutive content braces).
    Row {
        start: usize,
        end: usize,
        panes: Vec<String>,
    },
}

pub(super) fn scan_segments(lines: &[&str]) -> Result<Vec<Segment>> {
    let mut segments = Vec::new();
    let mut i = scan_tessprek_preamble(lines, 0).body_start;

    while i < lines.len() {
        let start = i;
        let line_no = i + 1;
        let trimmed = lines[i].trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        if trimmed.starts_with("\\row{") || trimmed == "\\row" {
            let (panes, cmd_end) = take_row_panes(lines, i)?;
            segments.push(Segment::Row {
                start,
                end: cmd_end,
                panes,
            });
            i = cmd_end;
            continue;
        }

        // Bare `\toc` (no braces) — attrs-only TOC with defaults.
        if trimmed == "\\toc" {
            segments.push(Segment::Directive {
                start,
                end: i + 1,
                kind: "toc".into(),
                map: BTreeMap::new(),
                body: String::new(),
            });
            i += 1;
            continue;
        }

        // Bare `\columns` / `\endcolumns` (THI-391).
        if trimmed == "\\columns" {
            segments.push(Segment::Directive {
                start,
                end: i + 1,
                kind: "columns".into(),
                map: BTreeMap::new(),
                body: String::new(),
            });
            i += 1;
            continue;
        }
        if trimmed == "\\endcolumns" {
            segments.push(Segment::Directive {
                start,
                end: i + 1,
                kind: "endcolumns".into(),
                map: BTreeMap::new(),
                body: String::new(),
            });
            i += 1;
            continue;
        }

        if let Some((kind, prefix)) = match_body_opener(trimmed) {
            let (attrs, cmd_end) = take_brace_command(lines, i, prefix, kind)?;
            i = cmd_end;
            // `\layout{…}` carries op lines, not flat key=value attrs.
            if kind == "layout" {
                segments.push(Segment::Directive {
                    start,
                    end: i,
                    kind: kind.to_owned(),
                    map: BTreeMap::new(),
                    body: attrs,
                });
                continue;
            }
            let map = parse_attrs(&attrs, line_no)?;
            match kind {
                "block" => {
                    let body_start = i;
                    i = next_boundary(lines, i);
                    segments.push(Segment::Markdown {
                        start,
                        body_start,
                        end: i,
                        preserve: Some(map),
                    });
                }
                "slide" | "attachment" | "cite" | "quote" | "ref" | "toc" | "columns" => {
                    segments.push(Segment::Directive {
                        start,
                        end: i,
                        kind: kind.to_owned(),
                        map,
                        body: String::new(),
                    });
                }
                "figure" => {
                    // Prefer attrs-only (`alt=`). Optional legacy Markdown body:
                    // `![alt](media:N)` on the following lines.
                    let mut body = String::new();
                    let j = skip_blank_lines(lines, i);
                    if lines
                        .get(j)
                        .is_some_and(|l| l.trim().starts_with("![") && l.contains("](media:"))
                    {
                        i = next_boundary(lines, j);
                        body = trim_block_body(&lines[j..i]);
                    }
                    segments.push(Segment::Directive {
                        start,
                        end: i,
                        kind: kind.to_owned(),
                        map,
                        body,
                    });
                }
                _ => {
                    let body_start = i;
                    i = next_boundary(lines, i);
                    let body = trim_block_body(&lines[body_start..i]);
                    segments.push(Segment::Directive {
                        start,
                        end: i,
                        kind: kind.to_owned(),
                        map,
                        body,
                    });
                }
            }
        } else {
            i = next_boundary(lines, i);
            // Skip all-blank runs (shouldn't happen after trim gate above).
            if lines[start..i].iter().any(|l| !l.trim().is_empty()) {
                segments.push(Segment::Markdown {
                    start,
                    body_start: start,
                    end: i,
                    preserve: None,
                });
            }
        }
    }

    Ok(segments)
}

fn next_boundary(lines: &[&str], mut i: usize) -> usize {
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if match_body_opener(trimmed).is_some()
            || trimmed.starts_with("\\row{")
            || trimmed == "\\row"
            || trimmed == "\\toc"
            || trimmed == "\\columns"
            || trimmed == "\\endcolumns"
        {
            break;
        }
        i += 1;
    }
    i
}
