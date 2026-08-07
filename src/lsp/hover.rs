//! `textDocument/hover` over Tessprek markers and body lines (chunk id / role).

use std::collections::BTreeMap;
use std::fmt::Write;

use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range};

use crate::catalog::chunk::TextRole;
use crate::edit::ContentBlock;
use crate::edit::markers::{
    BODY_COMMANDS, HEADER_COMMANDS, command_attr_keys, parse_brace_command,
};
use crate::edit::tessprek::{
    parse_attrs, parse_media_header, take_brace_command, take_leading_tessera_header,
};

use super::position::{nth_line, utf16_len};

/// Hover for a Tessprek marker or body line at `position`, if any.
pub(super) fn hover_at(text: &str, position: Position) -> Option<Hover> {
    let (line_idx, line) = nth_line(text, position.line)?;
    let line_usize = line_idx as usize;

    if let Some(hover) = tessera_header_hover(text, line_usize) {
        return Some(hover);
    }

    if let Some(hover) = brace_block_hover(text, line_usize) {
        return Some(hover);
    }

    let trimmed = line.trim();
    let trim_start = line.find(trimmed).unwrap_or(0);

    // Closed single-line `\ids{…}` / body cmds when not caught above.
    if let Some((kind, attrs)) = parse_brace_command(trimmed, true) {
        if kind == "tessera" {
            // Leading header handled above; ignore stray single-line `\tessera`.
        } else {
            let marker_start = utf16_len(&line[..trim_start]);
            let marker_end = marker_start + utf16_len(trimmed);
            let markdown = format_kind_hover(kind, attrs);
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

/// Hover for multiline `\media{…}`, `\figure{…}`, `\text{…}`, etc. when the
/// cursor is on any line of the brace block (not only the opener).
fn brace_block_hover(text: &str, line: usize) -> Option<Hover> {
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        let hit = HEADER_COMMANDS
            .iter()
            .chain(BODY_COMMANDS.iter())
            .find(|&&(prefix, kind)| kind != "tessera" && trimmed.starts_with(prefix));
        let Some(&(prefix, kind)) = hit else {
            i += 1;
            continue;
        };
        let Ok((inner, end)) = take_brace_command(&lines, i, prefix, kind) else {
            i += 1;
            continue;
        };
        // Only treat as a block hover when the command spans multiple lines
        // (single-line closed forms keep the tighter range from parse_brace_command).
        if end > i + 1 && line >= i && line < end {
            return Some(markup_hover(
                format_kind_hover(kind, &inner),
                Range {
                    start: Position {
                        line: i as u32,
                        character: 0,
                    },
                    end: Position {
                        line: end.saturating_sub(1).max(i) as u32,
                        character: 0,
                    },
                },
            ));
        }
        i = end.max(i + 1);
    }
    None
}

fn body_hover(text: &str, line: u32) -> Option<Hover> {
    let line = line as usize;
    let spanned = decode_tessprek_with_spans_safe(text)?;
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

fn decode_tessprek_with_spans_safe(text: &str) -> Option<Vec<(usize, usize, ContentBlock)>> {
    crate::edit::tessprek::decode_tessprek_with_spans(text).ok()
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

fn format_kind_hover(kind: &str, attrs: &str) -> String {
    match kind {
        "ids" => format_ids_hover(attrs),
        "media" => format_media_hover(attrs),
        other => {
            let map = parse_attrs(attrs, 1).unwrap_or_default();
            format_command_hover(other, &map)
        }
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
            push_opt_field(&mut out, "title", header.title.as_deref());
            push_opt_field(&mut out, "caption", header.caption.as_deref());
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
            push_field(
                &mut out,
                "media",
                &format!("media:{}", figure.image_chunk_id),
            );
            push_field(&mut out, "placement", figure.placement.as_str());
            push_opt_field(&mut out, "title", figure.title.as_deref());
            push_opt_field(&mut out, "caption", figure.caption.as_deref());
            push_field(&mut out, "alt", &figure.alt_text);
            out
        }
        ContentBlock::Cite { cite, .. } => {
            let kind = crate::io::cite::classify_cite(cite);
            let title = match kind {
                crate::io::cite::CiteTessprekKind::Biblio => "cite",
                crate::io::cite::CiteTessprekKind::Quote => "quote",
                crate::io::cite::CiteTessprekKind::Ref => "ref",
            };
            let mut out = chunk_title(&id, title);
            push_opt_field(&mut out, "label", cite.label.as_deref());
            push_opt_field(&mut out, "target_doc", cite.target_doc_id.as_deref());
            push_opt_num(&mut out, "target_chunk", cite.target_chunk_id);
            push_opt_num(&mut out, "target_byte_start", cite.target_byte_start);
            push_opt_num(&mut out, "target_byte_end", cite.target_byte_end);
            push_opt_num(&mut out, "page", cite.page);
            if matches!(kind, crate::io::cite::CiteTessprekKind::Quote) && !cite.quote.is_empty() {
                let preview: String = cite.quote.chars().take(80).collect();
                push_opt_field(&mut out, "quote", Some(preview.as_str()));
            }
            out
        }
        ContentBlock::Slide { slide, .. } => {
            let mut out = chunk_title(&id, "slide");
            push_field(&mut out, "layout", &slide.layout_id);
            out
        }
        ContentBlock::Layout { layout, .. } => {
            let mut out = chunk_title(&id, "layout");
            push_field(&mut out, "ops", &layout.ops.len().to_string());
            out
        }
        ContentBlock::Attachment {
            filename,
            media_type,
            caption,
            sha256,
            ..
        } => {
            let mut out = chunk_title(&id, "attachment");
            push_field(&mut out, "filename", filename);
            push_field(&mut out, "media_type", media_type);
            push_field(&mut out, "sha256", sha256);
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

fn push_opt_num(out: &mut String, key: &str, value: Option<impl ToString>) {
    if let Some(v) = value {
        push_field(out, key, &v.to_string());
    }
}

fn format_command_hover(kind: &str, map: &BTreeMap<String, String>) -> String {
    let mut out = format!("**Tessprek `\\{kind}{{}}`**\n");
    // Prefer a stable key order for known commands.
    let preferred = command_attr_keys(kind).unwrap_or(&[]);
    let mut seen = std::collections::BTreeSet::new();
    for key in preferred {
        if let Some(v) = map.get(*key) {
            seen.insert(*key);
            push_field(&mut out, key, v);
        }
    }
    for (k, v) in map {
        if seen.contains(k.as_str()) {
            continue;
        }
        push_field(&mut out, k, v);
    }
    if map.is_empty() {
        out.push_str("\n\n_(no attributes)_");
    }
    if kind == "figure"
        && let Some(image) = map.get("image")
    {
        let _ = write!(
            out,
            "\n\nPoints at image payload `media:{image}` (see `\\media{{}}`)."
        );
    }
    if kind == "text" {
        out.push_str(
            "\n\nOptional label for the following Markdown block (`title` above, `caption` below on table/math/code).",
        );
    }
    out
}

fn format_ids_hover(attrs: &str) -> String {
    format!(
        "**Tessprek reading order** (`\\ids{{}}`)\n\n\
         Chunk ids for body blocks (text / figure / cite / slide / attachment). \
         Image payloads are listed separately in `\\media{{}}`.\n\n`{attrs}`"
    )
}

fn format_media_hover(attrs: &str) -> String {
    let entries = parse_media_header(attrs);
    let mut out = String::from(
        "**Media payloads** (`\\media{}`)\n\n\
         Image chunk metadata — targets of `media:N` / `\\figure{image=N}`. \
         Not reading-order blocks; bytes stay in the `.tes`.",
    );
    if entries.is_empty() {
        let trimmed = attrs.trim();
        if trimmed.is_empty() {
            out.push_str("\n\n_(no payloads)_");
        } else {
            let _ = write!(out, "\n\n`{trimmed}`");
        }
        return out;
    }
    for entry in entries {
        let _ = write!(out, "\n\n### `media:{}`", entry.chunk_id);
        if let Some(mime) = entry.media_type.as_deref() {
            push_field(&mut out, "media_type", mime);
        }
        if let Some(hash) = entry.sha256.as_deref() {
            let display = if hash.len() > 16 {
                format!("{}…", &hash[..16])
            } else {
                hash.to_owned()
            };
            push_field(&mut out, "sha256", &display);
        }
        if let Some(w) = entry.width_px {
            push_field(&mut out, "width", &w.to_string());
        }
        if let Some(h) = entry.height_px {
            push_field(&mut out, "height", &h.to_string());
        }
    }
    out
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
\\media{\n\
  id=3\n\
  media_type=image/png\n\
  sha256=7576115942178cbe3494ca7f82aba02d97d6f4467894d4d1314c1b2346155854\n\
  width=1\n\
  height=1\n\
}\n\
\n\
Hello\n\
\n\
\\figure{\n\
  image=3\n\
  placement=flow\n\
  alt=\"alt\"\n\
  title=\"Hero\"\n\
  caption=\"A cap\"\n\
}\n\
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
        assert!(text.contains("\\media"), "{text}");
    }

    #[test]
    fn hover_media_multiline_attr_line() {
        // Cursor on `media_type=image/png` inside `\media{…}`.
        let h = hover_at(
            SAMPLE,
            Position {
                line: 4,
                character: 4,
            },
        )
        .expect("media hover");
        let text = hover_plain(&h);
        assert!(text.contains("Media payloads"), "{text}");
        assert!(text.contains("media:3"), "{text}");
        assert!(text.contains("image/png"), "{text}");
        assert!(text.contains("width"), "{text}");
        assert!(text.contains("7576115942178cbe"), "{text}");
    }

    #[test]
    fn hover_body_chunk_id() {
        let h = hover_at(
            SAMPLE,
            Position {
                line: 10, // Hello
                character: 0,
            },
        )
        .expect("body hover");
        let text = hover_plain(&h);
        assert!(text.contains("chunk `1`"), "{text}");
        assert!(text.contains("paragraph"), "{text}");
    }

    #[test]
    fn hover_figure_multiline_attrs() {
        let h = hover_at(
            SAMPLE,
            Position {
                line: 16, // title="Hero"
                character: 4,
            },
        )
        .expect("figure hover");
        let text = hover_plain(&h);
        assert!(text.contains("figure"), "{text}");
        assert!(text.contains("image"), "{text}");
        assert!(text.contains("media:3"), "{text}");
        assert!(text.contains("Hero"), "{text}");
        assert!(text.contains("A cap"), "{text}");
    }

    #[test]
    fn hover_figure_chunk_on_directive() {
        // Body hover on `\figure{` opener (attrs-only; no Markdown image line).
        let h = hover_at(
            SAMPLE,
            Position {
                line: 12,
                character: 0,
            },
        )
        .expect("figure directive");
        let text = hover_plain(&h);
        // Multiline brace hover wins over body span on opener lines.
        assert!(text.contains("figure"), "{text}");
        assert!(text.contains("media:3") || text.contains("image"), "{text}");
        assert!(text.contains("Hero"), "{text}");
    }

    #[test]
    fn hover_text_title_caption_directive() {
        let text = "\
\\tessera{format=tessprek version=2}\n\
\\ids{1}\n\
\n\
\\text{\n\
  title=\"Listing 1\"\n\
  caption=\"Hello\"\n\
}\n\
```rust\n\
fn main() {}\n\
```\n\
";
        let h = hover_at(
            text,
            Position {
                line: 4,
                character: 4,
            },
        )
        .expect("text directive hover");
        let plain = hover_plain(&h);
        assert!(plain.contains("\\text"), "{plain}");
        assert!(plain.contains("Listing 1"), "{plain}");
        assert!(plain.contains("Hello"), "{plain}");

        let body = hover_at(
            text,
            Position {
                line: 8,
                character: 0,
            },
        )
        .expect("code body hover");
        let plain = hover_plain(&body);
        assert!(plain.contains("code"), "{plain}");
        assert!(plain.contains("Listing 1"), "{plain}");
        assert!(plain.contains("Hello"), "{plain}");
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
