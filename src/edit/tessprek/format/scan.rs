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

/// Bare empty-body markers accepted without braces (`\toc`, `\lof`, …).
const BARE_MARKERS: &[(&str, &str)] = &[
    ("\\toc", "toc"),
    ("\\lof", "lof"),
    ("\\lot", "lot"),
    ("\\columns", "columns"),
    ("\\endcolumns", "endcolumns"),
];

/// Bare titled-band openers whose following paragraph is the body (THI-414 / 412).
const BARE_BODY_MARKERS: &[(&str, &str)] = &[("\\proof", "proof"), ("\\abstract", "abstract")];

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

        if let Some(kind) = bare_marker_kind(trimmed) {
            segments.push(empty_directive(start, i + 1, kind));
            i += 1;
            continue;
        }

        if let Some(kind) = bare_body_marker_kind(trimmed) {
            i += 1;
            let body_start = skip_blank_lines(lines, i);
            i = next_para_end(lines, body_start);
            segments.push(Segment::Directive {
                start,
                end: i,
                kind: kind.to_owned(),
                map: BTreeMap::new(),
                body: trim_block_body(&lines[body_start..i]),
            });
            continue;
        }

        if let Some((kind, prefix)) = match_body_opener(trimmed) {
            i = push_brace_directive(&mut segments, lines, start, i, line_no, kind, prefix)?;
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

fn bare_marker_kind(trimmed: &str) -> Option<&'static str> {
    BARE_MARKERS
        .iter()
        .find_map(|&(token, kind)| (trimmed == token).then_some(kind))
}

fn bare_body_marker_kind(trimmed: &str) -> Option<&'static str> {
    BARE_BODY_MARKERS
        .iter()
        .find_map(|&(token, kind)| (trimmed == token).then_some(kind))
}

fn empty_directive(start: usize, end: usize, kind: &str) -> Segment {
    Segment::Directive {
        start,
        end,
        kind: kind.to_owned(),
        map: BTreeMap::new(),
        body: String::new(),
    }
}

fn push_brace_directive(
    segments: &mut Vec<Segment>,
    lines: &[&str],
    start: usize,
    i: usize,
    line_no: usize,
    kind: &str,
    prefix: &str,
) -> Result<usize> {
    let (attrs, mut i) = take_brace_command(lines, i, prefix, kind)?;
    // `\layout{…}` carries op lines, not flat key=value attrs.
    if kind == "layout" {
        segments.push(Segment::Directive {
            start,
            end: i,
            kind: kind.to_owned(),
            map: BTreeMap::new(),
            body: attrs,
        });
        return Ok(i);
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
        "slide" | "attachment" | "cite" | "quote" | "ref" | "toc" | "lof" | "lot" | "columns" => {
            segments.push(Segment::Directive {
                start,
                end: i,
                kind: kind.to_owned(),
                map,
                body: String::new(),
            });
        }
        "theorem" | "callout" | "proof" | "abstract" => {
            let body_start = skip_blank_lines(lines, i);
            i = next_para_end(lines, body_start);
            let body = trim_block_body(&lines[body_start..i]);
            segments.push(Segment::Directive {
                start,
                end: i,
                kind: kind.to_owned(),
                map,
                body,
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
    Ok(i)
}

fn next_para_end(lines: &[&str], mut i: usize) -> usize {
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.is_empty() || is_segment_boundary(trimmed) {
            break;
        }
        i += 1;
    }
    i
}

fn is_segment_boundary(trimmed: &str) -> bool {
    match_body_opener(trimmed).is_some()
        || trimmed.starts_with("\\row{")
        || trimmed == "\\row"
        || bare_marker_kind(trimmed).is_some()
        || bare_body_marker_kind(trimmed).is_some()
}

fn next_boundary(lines: &[&str], mut i: usize) -> usize {
    while i < lines.len() {
        if is_segment_boundary(lines[i].trim()) {
            break;
        }
        i += 1;
    }
    i
}
