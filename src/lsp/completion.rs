//! `textDocument/completion` for Tessprek brace commands and attribute keys.

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit, Documentation,
    InsertTextFormat, MarkupContent, MarkupKind, Position, Range, TextEdit,
};

use crate::edit::markers::{BODY_COMMANDS, HEADER_COMMANDS, surface_name};

use super::position::{nth_line, utf16_len, utf16_prefix};

/// Completions at `position` in a Tessprek buffer.
pub(super) fn completions_at(text: &str, position: Position) -> Option<CompletionResponse> {
    let (line_idx, line) = nth_line(text, position.line)?;
    let prefix = utf16_prefix(line, position.character as usize);

    if let Some(items) = attr_key_completions(prefix, line_idx, position.character) {
        return Some(CompletionResponse::Array(items));
    }
    if let Some(items) = command_completions(prefix, line_idx, position.character) {
        return Some(CompletionResponse::Array(items));
    }
    None
}

fn snippet_edit(line: u32, start: u32, end: u32, new_text: String) -> CompletionTextEdit {
    CompletionTextEdit::Edit(TextEdit {
        range: Range {
            start: Position {
                line,
                character: start,
            },
            end: Position {
                line,
                character: end,
            },
        },
        new_text,
    })
}

fn command_completions(prefix: &str, line: u32, character: u32) -> Option<Vec<CompletionItem>> {
    let bs = prefix.rfind('\\')?;
    let rest = &prefix[bs + 1..];
    if rest.contains('{') || rest.contains(' ') || rest.contains('\t') {
        return None;
    }
    let typed = rest;
    let replace_start = utf16_len(&prefix[..bs]);
    let mut items = Vec::new();
    for &(_prefix, kind) in HEADER_COMMANDS.iter().chain(BODY_COMMANDS.iter()) {
        let surface = surface_name(kind);
        if !surface.starts_with(typed) && !kind.starts_with(typed) {
            continue;
        }
        let (insert, detail) = command_snippet(surface);
        items.push(CompletionItem {
            label: format!("\\{surface}"),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some(detail.into()),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("Tessprek `\\{surface}{{…}}`"),
            })),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            text_edit: Some(snippet_edit(line, replace_start, character, insert)),
            filter_text: Some(format!("\\{surface}")),
            ..Default::default()
        });
    }
    if items.is_empty() { None } else { Some(items) }
}

fn command_snippet(surface: &str) -> (String, &'static str) {
    match surface {
        "tessera" => (
            "\\tessera{format=tessprek version=2$0}".into(),
            "document header",
        ),
        "ids" => ("\\ids{${1:1}}$0".into(), "reading-order chunk ids"),
        "text" => (
            "\\text{${1:class=\"\"}}$0".into(),
            "preserve class/lang/align",
        ),
        "figure" => (
            "\\figure{image=${1:1} placement=${2:flow} caption=\"${3:}\"}$0".into(),
            "figure directive",
        ),
        "cite" => ("\\cite{label=${1:Key}}$0".into(), "cite / quote block"),
        "slide" => (
            "\\slide{layout=${1:title_body} regions=\"${2:title:1,body:2}\"}$0".into(),
            "slide layout",
        ),
        "attach" => (
            "\\attach{filename=\"${1:file.pdf}\" media_type=${2:application/pdf} sha256=${3:}}$0"
                .into(),
            "attachment",
        ),
        other => (format!("\\{other}{{$0}}"), "brace command"),
    }
}

fn attr_key_completions(prefix: &str, line: u32, character: u32) -> Option<Vec<CompletionItem>> {
    let (cmd, inside) = open_brace_context(prefix)?;
    let after = inside.rsplit(['{', ' ', '\t']).next().unwrap_or(inside);
    if after.contains('=') {
        return None;
    }
    let typed = after;
    let keys = attr_keys_for(cmd)?;
    let replace_start = character.saturating_sub(utf16_len(typed));
    let mut items = Vec::new();
    for key in keys {
        if !key.starts_with(typed) {
            continue;
        }
        items.push(CompletionItem {
            label: (*key).into(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some(format!("\\{cmd} attribute")),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            text_edit: Some(snippet_edit(
                line,
                replace_start,
                character,
                format!("{key}=$0"),
            )),
            ..Default::default()
        });
    }
    if items.is_empty() { None } else { Some(items) }
}

fn open_brace_context(prefix: &str) -> Option<(&str, &str)> {
    let bs = prefix.rfind('\\')?;
    let rest = &prefix[bs + 1..];
    let brace = rest.find('{')?;
    if rest[brace + 1..].contains('}') {
        return None;
    }
    let cmd = &rest[..brace];
    if cmd.is_empty() || cmd.chars().any(|c| !c.is_ascii_alphabetic()) {
        return None;
    }
    Some((cmd, &rest[brace + 1..]))
}

fn attr_keys_for(cmd: &str) -> Option<&'static [&'static str]> {
    Some(match cmd {
        "tessera" => &["format", "version", "source-hash"],
        "text" => &["class", "lang", "align", "code_lang"],
        "figure" => &["image", "placement", "region", "caption"],
        "cite" => &["label", "key", "target_doc", "target_chunk", "page"],
        "slide" => &["layout", "regions"],
        "attach" => &["filename", "media_type", "sha256", "caption"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completes_figure_stem() {
        let text = "\\fig";
        let items = completions_at(
            text,
            Position {
                line: 0,
                character: 4,
            },
        )
        .expect("completions");
        let CompletionResponse::Array(items) = items else {
            panic!("expected array");
        };
        assert!(items.iter().any(|i| i.label == "\\figure"), "{items:?}");
    }

    #[test]
    fn completes_figure_attrs() {
        let text = "\\figure{im";
        let items = completions_at(
            text,
            Position {
                line: 0,
                character: 10,
            },
        )
        .expect("attr completions");
        let CompletionResponse::Array(items) = items else {
            panic!("expected array");
        };
        assert!(items.iter().any(|i| i.label == "image"), "{items:?}");
    }

    #[test]
    fn no_completion_in_prose() {
        assert!(
            completions_at(
                "Hello world",
                Position {
                    line: 0,
                    character: 5,
                },
            )
            .is_none()
        );
    }
}
