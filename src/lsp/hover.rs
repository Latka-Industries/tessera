//! `textDocument/hover` over Tessprek header / chunk markers.

use std::fmt::Write;

use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range};

use crate::edit::markers::{CHUNK_PREFIX, COMMENT_SUFFIX, HEADER_PREFIX};

use super::position::{nth_line, utf16_len};

/// Hover for a Tessprek marker at `position`, if any.
pub(super) fn hover_at(text: &str, position: Position) -> Option<Hover> {
    let (line_idx, line) = nth_line(text, position.line)?;
    let trimmed = line.trim();
    let trim_start = line.find(trimmed).unwrap_or(0);

    let (kind, attrs) = if trimmed.starts_with(CHUNK_PREFIX) && trimmed.ends_with(COMMENT_SUFFIX) {
        let attrs = &trimmed[CHUNK_PREFIX.len()..trimmed.len() - COMMENT_SUFFIX.len()];
        ("chunk", attrs)
    } else if trimmed.starts_with(HEADER_PREFIX) && trimmed.ends_with(COMMENT_SUFFIX) {
        let attrs = &trimmed[HEADER_PREFIX.len()..trimmed.len() - COMMENT_SUFFIX.len()];
        ("header", attrs)
    } else {
        return None;
    };

    // Cursor must sit on the HTML comment (UTF-16 columns).
    let marker_start = utf16_len(&line[..trim_start]);
    let marker_end = marker_start + utf16_len(trimmed);
    if position.character < marker_start || position.character > marker_end {
        return None;
    }

    let map = parse_simple_attrs(attrs);
    let markdown = match kind {
        "header" => format_header_hover(&map),
        _ => format_chunk_hover(&map),
    };

    let range = Range {
        start: Position {
            line: line_idx,
            character: marker_start,
        },
        end: Position {
            line: line_idx,
            character: marker_end,
        },
    };

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: markdown,
        }),
        range: Some(range),
    })
}

fn format_chunk_hover(map: &[(String, String)]) -> String {
    let chunk = attr(map, "chunk").unwrap_or("?");
    let mut out = format!("**Tessprek chunk** `{chunk}`\n");
    if let Some(role) = attr(map, "role") {
        let _ = write!(out, "\n- **role:** `{role}`");
    }
    if let Some(ty) = attr(map, "type") {
        let _ = write!(out, "\n- **type:** `{ty}`");
    }
    for (k, v) in map {
        if k == "chunk" || k == "role" || k == "type" {
            continue;
        }
        let _ = write!(out, "\n- **{k}:** `{v}`");
    }
    if attr(map, "role").is_none() && attr(map, "type").is_none() {
        out.push_str("\n\n_(no role/type attrs)_");
    }
    out
}

fn format_header_hover(map: &[(String, String)]) -> String {
    let mut out = String::from("**Tessprek document header**\n");
    for (k, v) in map {
        let display = if k == "source-hash" && v.len() > 12 {
            format!("{}…", &v[..12])
        } else {
            v.clone()
        };
        let _ = write!(out, "\n- **{k}:** `{display}`");
    }
    out
}

fn attr<'a>(map: &'a [(String, String)], key: &str) -> Option<&'a str> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// Minimal `key=value` / `key="quoted"` tokenizer for hover display.
fn parse_simple_attrs(attrs: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = attrs.trim();
    while !rest.is_empty() {
        let Some(eq) = rest.find('=') else {
            break;
        };
        let key = rest[..eq].trim();
        rest = rest[eq + 1..].trim_start();
        if key.is_empty() {
            break;
        }
        let (value, next) = if let Some(quoted) = rest.strip_prefix('"') {
            let end = quoted.find('"').unwrap_or(quoted.len());
            let value = quoted[..end].to_owned();
            let next = quoted.get(end + 1..).unwrap_or("").trim_start();
            (value, next)
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let value = rest[..end].to_owned();
            let next = rest[end..].trim_start();
            (value, next)
        };
        out.push((key.to_owned(), value));
        rest = next;
    }
    out
}

/// Debug helper for unit tests.
#[cfg(test)]
fn hover_plain(hover: &Hover) -> String {
    use tower_lsp::lsp_types::{LanguageString, MarkedString};

    match &hover.contents {
        HoverContents::Scalar(MarkedString::String(s)) => s.clone(),
        HoverContents::Scalar(MarkedString::LanguageString(LanguageString { value, .. })) => {
            value.clone()
        }
        HoverContents::Markup(MarkupContent { value, .. }) => value.clone(),
        HoverContents::Array(items) => items
            .iter()
            .map(|m| match m {
                MarkedString::String(s) => s.as_str(),
                MarkedString::LanguageString(LanguageString { value, .. }) => value.as_str(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
<!-- tessera: format=tessprek version=1 source-hash=abc123def456 -->\n\
\n\
<!-- tes chunk=1 role=paragraph -->\n\
Hello\n\
\n\
<!-- tes chunk=2 type=figure image=3 placement=block caption=\"A cap\" -->\n\
";

    #[test]
    fn hover_chunk_role() {
        let h = hover_at(
            SAMPLE,
            Position {
                line: 2,
                character: 10,
            },
        )
        .expect("hover");
        let text = hover_plain(&h);
        assert!(text.contains("chunk"), "{text}");
        assert!(text.contains("`1`"), "{text}");
        assert!(text.contains("paragraph"), "{text}");
    }

    #[test]
    fn hover_header_source_hash() {
        let h = hover_at(
            SAMPLE,
            Position {
                line: 0,
                character: 5,
            },
        )
        .expect("hover");
        let text = hover_plain(&h);
        assert!(text.contains("header"), "{text}");
        assert!(text.contains("tessprek"), "{text}");
        assert!(
            text.contains("abc123def456")
                || text.contains("abc123def456…")
                || text.contains("abc123def456"),
            "{text}"
        );
    }

    #[test]
    fn hover_misses_body() {
        assert!(
            hover_at(
                SAMPLE,
                Position {
                    line: 3,
                    character: 0,
                },
            )
            .is_none()
        );
    }

    #[test]
    fn hover_figure_type() {
        let h = hover_at(
            SAMPLE,
            Position {
                line: 5,
                character: 8,
            },
        )
        .expect("hover");
        let text = hover_plain(&h);
        assert!(text.contains("figure"), "{text}");
        assert!(text.contains("`2`"), "{text}");
        assert!(text.contains("caption"), "{text}");
    }
}
