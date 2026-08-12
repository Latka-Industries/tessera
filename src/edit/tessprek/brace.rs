use crate::error::Result;

use super::markers::{BRACE_SUFFIX, IDS_PREFIX, MEDIA_PREFIX, TESSERA_PREFIX};
use super::util::parse_err;

/// Result of scanning optional `\tessera{}`, `\ids{}`, and `\media{}` preamble.
pub(crate) struct TessprekPreamble {
    /// First body line index after preamble (skips trailing blanks).
    pub body_start: usize,
    /// Inner attrs of `\media{…}` when present and parseable.
    pub media_inner: Option<String>,
}

/// Skip optional Tessprek header, ids line, and media block after `start`.
pub(crate) fn scan_tessprek_preamble(lines: &[&str], start: usize) -> TessprekPreamble {
    let mut i = skip_blank_lines(lines, start);
    if let Ok((_, end)) = take_tessera_header(lines, i) {
        i = end;
    }
    i = skip_blank_lines(lines, i);
    if lines
        .get(i)
        .is_some_and(|l| l.trim().starts_with(IDS_PREFIX))
    {
        i += 1;
    }
    i = skip_blank_lines(lines, i);
    let media_inner = if lines
        .get(i)
        .is_some_and(|l| l.trim().starts_with(MEDIA_PREFIX))
        && let Ok((inner, end)) = take_brace_command(lines, i, MEDIA_PREFIX, "media header")
    {
        i = end;
        Some(inner)
    } else {
        None
    };
    TessprekPreamble {
        body_start: skip_blank_lines(lines, i),
        media_inner,
    }
}

/// Parse a brace command starting at `lines[start]` (0-based).
///
/// Accepts a single-line `\cmd{…}` or a multiline form:
///
/// ```text
/// \block{
///   title="…"
///   caption="…"
/// }
/// ```
///
/// Returns `(inner attrs, end_line_exclusive)`.
///
/// # Errors
///
/// Returns [`TesError::EditParse`] when the opener is missing or `}` is never
/// closed (respecting quoted attribute values).
pub(crate) fn take_brace_command(
    lines: &[&str],
    start: usize,
    prefix: &str,
    label: &str,
) -> Result<(String, usize)> {
    let header_line_no = start.saturating_add(1);
    let first = lines.get(start).map_or("", |l| l.trim());
    if !first.starts_with(prefix) {
        return Err(parse_err(
            header_line_no,
            1,
            format!("expected `{prefix}...{BRACE_SUFFIX}` {label}, found: {first}"),
        ));
    }

    let mut buf = String::new();
    let mut end = start;
    while end < lines.len() {
        let piece = lines[end].trim();
        if end > start {
            buf.push(' ');
        }
        buf.push_str(piece);
        let after = buf
            .strip_prefix(prefix)
            .expect("prefix checked on first line");
        if let Some(close) = find_unquoted_close_brace(after) {
            let trailing = after[close + 1..].trim();
            if !trailing.is_empty() {
                return Err(parse_err(
                    end + 1,
                    1,
                    format!("trailing junk after `{BRACE_SUFFIX}` in \\{label}: {trailing}"),
                ));
            }
            return Ok((after[..close].to_owned(), end + 1));
        }
        end += 1;
    }
    Err(parse_err(
        header_line_no,
        1,
        format!("unterminated `{prefix}` {label} (missing `{BRACE_SUFFIX}`)"),
    ))
}

/// Parse `\row{pane0}{pane1}…` starting at `lines[start]` (single logical line).
///
/// Requires at least two consecutive content braces. Nested `{…}` inside a pane
/// (e.g. `\font{id}{text}`) is respected via [`find_unquoted_close_brace`].
///
/// # Errors
///
/// Returns [`TesError::EditParse`] when the opener is missing, a brace is
/// unclosed, fewer than two panes are present, or trailing junk remains.
pub(crate) fn take_row_panes(lines: &[&str], start: usize) -> Result<(Vec<String>, usize)> {
    let line_no = start.saturating_add(1);
    let first = lines.get(start).map_or("", |l| l.trim());
    let Some(mut rest) = first.strip_prefix("\\row") else {
        return Err(parse_err(
            line_no,
            1,
            format!("expected `\\row{{…}}{{…}}`, found: {first}"),
        ));
    };
    let mut panes = Vec::new();
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let Some(inner) = rest.strip_prefix('{') else {
            return Err(parse_err(
                line_no,
                1,
                format!("expected `{{` for next \\row pane, found: {rest}"),
            ));
        };
        let Some(close) = find_unquoted_close_brace(inner) else {
            return Err(parse_err(line_no, 1, "unclosed `{…}` in \\row pane"));
        };
        panes.push(inner[..close].to_owned());
        rest = &inner[close + 1..];
    }
    if panes.len() < 2 {
        return Err(parse_err(
            line_no,
            1,
            format!("\\row requires at least 2 panes, found {}", panes.len()),
        ));
    }
    Ok((panes, start + 1))
}

/// Parse a `\tessera{…}` header starting at `lines[start]` (0-based).
///
/// # Errors
///
/// Same as [`take_brace_command`].
pub(crate) fn take_tessera_header(lines: &[&str], start: usize) -> Result<(String, usize)> {
    take_brace_command(lines, start, TESSERA_PREFIX, "tessera header")
}

/// Skip leading blank lines; return the first non-blank index (or `lines.len()`).
#[must_use]
pub(crate) fn skip_blank_lines(lines: &[&str], mut i: usize) -> usize {
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }
    i
}

/// Parse the leading `\tessera{…}` header after optional blanks.
///
/// Returns `(attrs, start_line, end_line_exclusive)` where `start_line` is the
/// first header line (0-based).
///
/// # Errors
///
/// Same as [`take_tessera_header`].
pub(crate) fn take_leading_tessera_header(lines: &[&str]) -> Result<(String, usize, usize)> {
    let start = skip_blank_lines(lines, 0);
    let (attrs, end) = take_tessera_header(lines, start)?;
    Ok((attrs, start, end))
}

/// Byte offset of the matching `}` for an already-opened brace group.
///
/// Tracks nested `{…}` depth and ignores braces inside double-quoted attribute
/// values (with `\"` escapes). Callers pass the slice **after** the opening `{`.
#[must_use]
pub(crate) fn find_unquoted_close_brace(s: &str) -> Option<usize> {
    let mut depth = 1i32;
    let mut in_quote = false;
    let mut escape = false;
    for (i, ch) in s.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_quote => escape = true,
            '"' => in_quote = !in_quote,
            '{' if !in_quote => depth += 1,
            '}' if !in_quote => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}
