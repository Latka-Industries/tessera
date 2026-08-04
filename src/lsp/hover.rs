//! `textDocument/hover` over Tessprek markers and body lines (chunk id / role).

use std::collections::BTreeMap;
use std::fmt::Write;

use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range};

use crate::catalog::chunk::TextRole;
use crate::edit::ContentBlock;
use crate::edit::markers::parse_brace_command;
use crate::edit::tessprek::{decode_tessprek_with_spans, parse_attrs, take_leading_tessera_header};

use super::position::{nth_line, utf16_len};

/// Hover for a Tessprek marker or body line at `position`, if any.
pub(super) fn hover_at(text: &str, position: Position) -> Option<Hover> {
    let (line_idx, line) = nth_line(text, position.line)?;
    let line_usize = line_idx as usize;

    if let Some(hover) = tessera_header_hover(text, line_usize) {
        return Some(hover);
    }

    let trimmed = line.trim();
    let trim_start = line.find(trimmed).unwrap_or(0);

    if let Some((kind, attrs)) = parse_brace_command(trimmed, true) {
        if kind == "tessera" {
            // Leading header handled above; ignore stray single-line `\tessera`.
        } else {
            let marker_start = utf16_len(&line[..trim_start]);
            let marker_end = marker_start + utf16_len(trimmed);
            let map = parse_attrs(attrs, 1).unwrap_or_default();
            let markdown = match kind {
                "ids" => format_ids_hover(attrs),
                other => format_command_hover(other, &map),
            };
            return Some(markup_hover(
                markdown,
                Range {
                    start: Position {
                        line: line_idx,
                        character: marker_start,
                    },
                    end: Position {
                        line: line_idx,
                        character: marker_end,
                    },
                },
            ));
        }
    }

    body_hover(text, position.line)
}

fn tessera_header_hover(text: &str, line: usize) -> Option<Hover> {
    let lines: Vec<&str> = text.lines().collect();
    let (attrs, start, end) = take_leading_tessera_header(&lines).ok()?;
    if line < start || line >= end {
        return None;
    }
    let map = parse_attrs(&attrs, 1).unwrap_or_default();
    Some(markup_hover(
        format_header_hover(&map),
        Range {
            start: Position {
                line: start as u32,
                character: 0,
            },
            end: Position {
                line: end.saturating_sub(1).max(start) as u32,
                character: 0,
            },
        },
    ))
}

fn body_hover(text: &str, line: u32) -> Option<Hover> {
    let line = line as usize;
    let spanned = decode_tessprek_with_spans(text).ok()?;
    let (start, end, block) = spanned
        .into_iter()
        .find(|(s, e, _)| line >= *s && line < *e)?;
    let markdown = format_block_hover(&block);
    let end_line = end.saturating_sub(1).max(start) as u32;
    Some(markup_hover(
        markdown,
        Range {
            start: Position {
                line: start as u32,
                character: 0,
            },
            end: Position {
                line: end_line,
                character: 0,
            },
        },
    ))
}

fn markup_hover(value: String, range: Range) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: Some(range),
    }
}

fn format_block_hover(block: &ContentBlock) -> String {
    let id = block
        .chunk_id()
        .map_or_else(|| "?".into(), |id| id.to_string());
    match block {
        ContentBlock::Text { header, body, .. } => {
            let mut out = chunk_title(&id, header.role.as_str());
            if header.role == TextRole::Heading
                && let Some(level) = header.level
            {
                let _ = write!(out, " (h{level})");
            }
            if let Some(lang) = header.code_lang.as_deref().or(header.lang.as_deref()) {
                push_field(&mut out, "lang", lang);
            }
            if !header.classes.is_empty() {
                push_field(&mut out, "class", &header.classes.join(" "));
            }
            let preview = body.lines().next().unwrap_or("").trim();
            if !preview.is_empty() {
                let short = if preview.len() > 80 {
                    format!("{}…", &preview[..80])
                } else {
                    preview.to_owned()
                };
                let _ = write!(out, "\n\n_{short}_");
            }
            out
        }
        ContentBlock::Figure { figure, .. } => {
            let mut out = chunk_title(&id, "figure");
            push_field(&mut out, "image", &figure.image_chunk_id.to_string());
            push_field(&mut out, "placement", figure.placement.as_str());
            push_opt_field(&mut out, "caption", figure.caption.as_deref());
            out
        }
        ContentBlock::Cite { cite, .. } => {
            let mut out = chunk_title(&id, "cite");
            push_opt_field(&mut out, "label", cite.label.as_deref());
            push_opt_field(&mut out, "target_doc", cite.target_doc_id.as_deref());
            if let Some(chunk) = cite.target_chunk_id {
                push_field(&mut out, "target_chunk", &chunk.to_string());
            }
            if let Some(page) = cite.page {
                push_field(&mut out, "page", &page.to_string());
            }
            out
        }
        ContentBlock::Slide { slide, .. } => {
            let mut out = chunk_title(&id, "slide");
            push_field(&mut out, "layout", &slide.layout_id);
            out
        }
        ContentBlock::Attachment {
            filename,
            media_type,
            caption,
            ..
        } => {
            let mut out = chunk_title(&id, "attachment");
            push_field(&mut out, "filename", filename);
            push_field(&mut out, "media_type", media_type);
            push_opt_field(&mut out, "caption", caption.as_deref());
            out
        }
    }
}

fn chunk_title(id: &str, kind: &str) -> String {
    format!("**Tessprek chunk `{id}`** — `{kind}`")
}

fn push_field(out: &mut String, key: &str, value: &str) {
    let _ = write!(out, "\n\n- **{key}:** `{value}`");
}

fn push_opt_field(out: &mut String, key: &str, value: Option<&str>) {
    if let Some(v) = value {
        push_field(out, key, v);
    }
}

fn format_command_hover(kind: &str, map: &BTreeMap<String, String>) -> String {
    let mut out = format!("**Tessprek `\\{kind}{{}}`**\n");
    for (k, v) in map {
        push_field(&mut out, k, v);
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
    use crate::edit::markers::TESSERA_HEADER_KEYS;

    let mut out = String::from("**Tessprek document header**\n");
    let mut seen = std::collections::BTreeSet::new();
    for key in TESSERA_HEADER_KEYS {
        if let Some(v) = map.get(*key) {
            seen.insert(*key);
            let display = if *key == "source-hash" && v.len() > 12 {
                format!("{}…", &v[..12])
            } else {
                v.clone()
            };
            push_field(&mut out, key, &display);
        }
    }
    for (k, v) in map {
        if seen.contains(k.as_str()) {
            continue;
        }
        push_field(&mut out, k, v);
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
\\tessera{format=tessprek version=2 source-hash=abc123def456 doc_id=550e8400-e29b-41d4-a716-446655440000 doc_kind=note title=\"Demo note\" language=en}\n\
\\ids{1,2}\n\
\n\
Hello\n\
\n\
\\figure{image=3 placement=flow caption=\"A cap\"}\n\
![alt](media:chunk-3)\n\
";

    #[test]
    fn hover_multiline_header_attr_line() {
        let text = "\
\\tessera{\n\
  format=tessprek\n\
  version=2\n\
  title=\"Demo note\"\n\
}\n\
\\ids{1}\n\
\n\
Hello\n\
";
        let h = hover_at(
            text,
            Position {
                line: 3,
                character: 4,
            },
        )
        .expect("hover");
        let plain = hover_plain(&h);
        assert!(plain.contains("header"), "{plain}");
        assert!(plain.contains("Demo note"), "{plain}");
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
        assert!(text.contains("abc123def456"), "{text}");
        assert!(text.contains("Demo note"), "{text}");
        assert!(text.contains("doc_id"), "{text}");
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
    fn hover_body_chunk_id() {
        let h = hover_at(
            SAMPLE,
            Position {
                line: 3,
                character: 0,
            },
        )
        .expect("body hover");
        let text = hover_plain(&h);
        assert!(text.contains("chunk `1`"), "{text}");
        assert!(text.contains("paragraph"), "{text}");
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

    #[test]
    fn hover_figure_body_line() {
        let h = hover_at(
            SAMPLE,
            Position {
                line: 6,
                character: 0,
            },
        )
        .expect("figure body");
        let text = hover_plain(&h);
        assert!(text.contains("chunk `2`"), "{text}");
        assert!(text.contains("figure"), "{text}");
    }

    #[test]
    fn math_lines_hover_as_math_not_table() {
        let text = "\
\\tessera{format=tessprek version=2}\n\
\\ids{1,2}\n\
\n\
$$\n\
x = 1\n\
$$\n\
\n\
| A | B |\n\
| --- | --- |\n\
| 1 | 2 |\n\
";
        let h = hover_at(
            text,
            Position {
                line: 4, // `x = 1`
                character: 0,
            },
        )
        .expect("hover on math body");
        let plain = hover_plain(&h);
        assert!(plain.contains("math"), "{plain}");
        assert!(!plain.contains("`table`"), "{plain}");
    }

    #[test]
    fn table_line_hover_as_table() {
        let text = "\
\\tessera{format=tessprek version=2}\n\
\\ids{1,2}\n\
\n\
$$\n\
x = 1\n\
$$\n\
\n\
| A | B |\n\
| --- | --- |\n\
| 1 | 2 |\n\
";
        let h = hover_at(
            text,
            Position {
                line: 7, // `| A | B |`
                character: 0,
            },
        )
        .expect("hover on table");
        let plain = hover_plain(&h);
        assert!(plain.contains("table"), "{plain}");
    }
}
