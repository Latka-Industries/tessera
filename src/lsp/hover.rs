//! `textDocument/hover` over Tessprek `\tessera{}` / `\ids{}` / brace-command markers.

use std::collections::BTreeMap;
use std::fmt::Write;

use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range};

use crate::edit::markers::parse_brace_command;
use crate::edit::tessprek::parse_attrs;

use super::position::{nth_line, utf16_len};

/// Hover for a Tessprek marker at `position`, if any.
pub(super) fn hover_at(text: &str, position: Position) -> Option<Hover> {
    let (line_idx, line) = nth_line(text, position.line)?;
    let trimmed = line.trim();
    let trim_start = line.find(trimmed).unwrap_or(0);

    let (kind, attrs) = parse_brace_command(trimmed, true)?;

    // Whole marker line is hoverable (column check was easy to miss with
    // leading indent / curswant quirks in clients).
    let marker_start = utf16_len(&line[..trim_start]);
    let marker_end = marker_start + utf16_len(trimmed);

    let map = parse_attrs(attrs, 1).unwrap_or_default();
    let markdown = match kind {
        "tessera" => format_header_hover(&map),
        "ids" => format_ids_hover(attrs),
        other => format_command_hover(other, &map),
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

fn format_command_hover(kind: &str, map: &BTreeMap<String, String>) -> String {
    let mut out = format!("**Tessprek `\\{kind}{{}}`**\n");
    for (k, v) in map {
        let _ = write!(out, "\n- **{k}:** `{v}`");
    }
    if map.is_empty() {
        out.push_str("\n\n_(no attributes)_");
    }
    out
}

fn format_ids_hover(attrs: &str) -> String {
    format!("**Tessprek reading order** (`\\ids{{}}`)\n\n`{attrs}`")
}

fn format_header_hover(map: &BTreeMap<String, String>) -> String {
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
\\tessera{format=tessprek version=2 source-hash=abc123def456}\n\
\\ids{1,2}\n\
\n\
Hello\n\
\n\
\\figure{image=3 placement=block caption=\"A cap\"}\n\
![alt](media:chunk-3)\n\
";

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
        assert!(text.contains("abc123def456"), "{text}");
    }

    #[test]
    fn hover_ids_list() {
        let h = hover_at(
            SAMPLE,
            Position {
                line: 1,
                character: 3,
            },
        )
        .expect("hover");
        let text = hover_plain(&h);
        assert!(text.contains("reading order"), "{text}");
        assert!(text.contains("1,2"), "{text}");
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
                character: 3,
            },
        )
        .expect("hover");
        let text = hover_plain(&h);
        assert!(text.contains("figure"), "{text}");
        assert!(text.contains("caption"), "{text}");
    }
}
