//! `textDocument/completion` for Tessprek brace commands and attribute keys.
//!
//! Pack-aware `\font{id}` / `\phrase{key}` / alias completions (THI-369).

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit, Documentation,
    InsertTextFormat, MarkupContent, MarkupKind, Position, Range, TextEdit,
};

use crate::edit::markers::{BODY_COMMANDS, HEADER_COMMANDS, command_attr_keys, surface_name};

use super::pack_completions::{PackCompletionCatalog, catalog_for_tessprek};
use super::position::{nth_line, utf16_len, utf16_prefix};

/// Completions at `position` in a Tessprek buffer.
pub(super) fn completions_at(text: &str, position: Position) -> Option<CompletionResponse> {
    let catalog = catalog_for_tessprek(text);
    completions_at_with_catalog(text, position, &catalog)
}

pub(super) fn completions_at_with_catalog(
    text: &str,
    position: Position,
    catalog: &PackCompletionCatalog,
) -> Option<CompletionResponse> {
    let (line_idx, line) = nth_line(text, position.line)?;
    let prefix = utf16_prefix(line, position.character as usize);

    if let Some(items) = pack_value_completions(prefix, line_idx, position.character, catalog) {
        return Some(CompletionResponse::Array(items));
    }
    if let Some(items) = attr_key_completions(prefix, line_idx, position.character) {
        return Some(CompletionResponse::Array(items));
    }
    if let Some(items) = command_completions(prefix, line_idx, position.character, catalog) {
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

/// Completions inside `\font{…}` / `\phrase{…}` first-brace slots.
fn pack_value_completions(
    prefix: &str,
    line: u32,
    character: u32,
    catalog: &PackCompletionCatalog,
) -> Option<Vec<CompletionItem>> {
    let (cmd, inside) = open_brace_context(prefix)?;
    let typed = inside
        .rsplit(['{', ' ', '\t', '='])
        .next()
        .unwrap_or(inside);
    if typed.contains('=') {
        return None;
    }
    let ids: &[String] = match cmd {
        "font" => &catalog.font_ids,
        "phrase" => &catalog.phrase_keys,
        _ => return None,
    };
    if ids.is_empty() {
        return None;
    }
    let replace_start = character.saturating_sub(utf16_len(typed));
    let mut items = Vec::new();
    for id in ids {
        if !id.starts_with(typed) {
            continue;
        }
        items.push(CompletionItem {
            label: id.clone(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some(format!("pack {cmd} id")),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("From document template pack (`\\{cmd}{{{id}}}`)."),
            })),
            text_edit: Some(snippet_edit(line, replace_start, character, id.clone())),
            ..Default::default()
        });
    }
    if items.is_empty() { None } else { Some(items) }
}

fn command_completions(
    prefix: &str,
    line: u32,
    character: u32,
    catalog: &PackCompletionCatalog,
) -> Option<Vec<CompletionItem>> {
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
        items.push(snippet_command_item(
            &format!("\\{surface}"),
            detail,
            format!("Tessprek `\\{surface}{{…}}`"),
            line,
            replace_start,
            character,
            insert,
        ));
    }
    push_inline_macro_snippets(&mut items, typed, line, replace_start, character);
    push_pack_alias_items(&mut items, typed, catalog, line, replace_start, character);
    if items.is_empty() { None } else { Some(items) }
}

fn snippet_command_item(
    label: &str,
    detail: &str,
    docs: String,
    line: u32,
    replace_start: u32,
    character: u32,
    insert: String,
) -> CompletionItem {
    CompletionItem {
        label: label.into(),
        kind: Some(CompletionItemKind::SNIPPET),
        detail: Some(detail.into()),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: docs,
        })),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        text_edit: Some(snippet_edit(line, replace_start, character, insert)),
        filter_text: Some(label.into()),
        ..Default::default()
    }
}

fn push_inline_macro_snippets(
    items: &mut Vec<CompletionItem>,
    typed: &str,
    line: u32,
    replace_start: u32,
    character: u32,
) {
    // Pack-expanded inline (not a sealed ContentBlock); snippet-only in v1.
    if "phrase".starts_with(typed) || typed.is_empty() {
        items.push(snippet_command_item(
            "\\phrase",
            "pack phrase (expand on format/seal)",
            "Expand pack `phrases.toml` / `tessera.toml` `[phrases]`: `\\phrase{key}` / `\\phrase{key}{arg…}` (D23). Slots `{arg}`/`$1` and `{argN}`/`$N`. Lossy seal.".into(),
            line,
            replace_start,
            character,
            "\\phrase{${1:key}}{${2:arg}}$0".into(),
        ));
    }
    // Sealed pack-pinned font (D23 / THI-356).
    if "font".starts_with(typed) || typed.is_empty() {
        items.push(snippet_command_item(
            "\\font",
            "pack-pinned font (seal to InlineKind::Font)",
            "Seal `\\font{font_id}{text}` → inline Font span; ids come from pack `fonts.toml` / `tessera.toml` `[fonts]` (THI-369).".into(),
            line,
            replace_start,
            character,
            "\\font{${1:font_id}}{${2:text}}$0".into(),
        ));
    }
    // Named Font Awesome icons (seal as Font span; encode prefers `\icon{name}`).
    if "icon".starts_with(typed) || typed.is_empty() {
        items.push(snippet_command_item(
            "\\icon",
            "named FA icon (→ Font span)",
            format!(
                "Seal `\\icon{{name}}` → pack face + glyph. Names: {}.",
                crate::catalog::icon_names().join(", ")
            ),
            line,
            replace_start,
            character,
            "\\icon{${1:github}}$0".into(),
        ));
    }
    if "footnote".starts_with(typed) || typed.is_empty() {
        items.push(snippet_command_item(
            "\\footnote",
            "footnote (seal to InlineKind::Note)",
            "Seal `\\footnote{body}` → inline note; native print paints a page-bottom band (THI-396).".into(),
            line,
            replace_start,
            character,
            "\\footnote{${1:note}}$0".into(),
        ));
    }
    if "endnote".starts_with(typed) || typed.is_empty() {
        items.push(snippet_command_item(
            "\\endnote",
            "endnote (seal to InlineKind::Note)",
            "Seal `\\endnote{body}` → inline note dumped after the last body block (THI-396)."
                .into(),
            line,
            replace_start,
            character,
            "\\endnote{${1:note}}$0".into(),
        ));
    }
}

fn push_pack_alias_items(
    items: &mut Vec<CompletionItem>,
    typed: &str,
    catalog: &PackCompletionCatalog,
    line: u32,
    replace_start: u32,
    character: u32,
) {
    // Pack aliases: `\name` → fixed string at format (not sealed core commands).
    // Only after the user typed a prefix (avoid dumping every alias on bare `\`).
    if typed.is_empty() {
        return;
    }
    for name in &catalog.alias_names {
        if !name.starts_with(typed) {
            continue;
        }
        items.push(CompletionItem {
            label: format!("\\{name}"),
            kind: Some(CompletionItemKind::TEXT),
            detail: Some("pack alias".into()),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!(
                    "Pack alias `\\{name}` (expands at format from aliases.toml / tessera.toml)."
                ),
            })),
            text_edit: Some(snippet_edit(
                line,
                replace_start,
                character,
                format!("\\{name}"),
            )),
            filter_text: Some(format!("\\{name}")),
            ..Default::default()
        });
    }
}

fn command_snippet(surface: &str) -> (String, &'static str) {
    match surface {
        "tessera" => (
            "\\tessera{format=tessprek version=2$0}".into(),
            "document header",
        ),
        "ids" => ("\\ids{${1:1}}$0".into(), "reading-order chunk ids"),
        "media" => (
            "\\media{\n  id=${1:1}\n  media_type=${2:image/png}\n  sha256=${3:}\n  width=${4:0}\n  height=${5:0}\n}$0"
                .into(),
            "media payload header (media:N targets)",
        ),
        "block" => (
            "\\block{${1:title=\"\" caption=\"\"}}$0".into(),
            "title/caption/class/lang/align",
        ),
        "figure" => (
            "\\figure{\n  image=${1:1}\n  placement=${2:flow}\n  alt=\"${3:}\"\n  title=\"${4:}\"\n  caption=\"${5:}\"\n}$0"
                .into(),
            "figure directive",
        ),
        "cite" => (
            "\\cite{label=${1:Key} author=\"${2:}\" title=\"${3:}\" year=${4:}}$0".into(),
            "bibliography stub",
        ),
        "quote" => (
            "\\quote{target_chunk=${1:1} quote=\"${2:}\"}$0".into(),
            "quoted passage",
        ),
        "ref" => (
            "\\ref{target_chunk=${1:1}}$0".into(),
            "cross-doc / chunk pointer",
        ),
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
    if matches!(cmd, "font" | "phrase") {
        // Value slot handled by pack_value_completions.
        return None;
    }
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
    command_attr_keys(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::pack_completions::PackCompletionCatalog;

    fn catalog_demo() -> PackCompletionCatalog {
        PackCompletionCatalog {
            font_ids: vec!["armenian".into(), "greek".into(), "cyrillic".into()],
            phrase_keys: vec!["yegourdoon".into()],
            alias_names: vec!["shortname".into()],
        }
    }

    #[test]
    fn completes_figure_stem() {
        let text = "\\fig";
        let items = completions_at_with_catalog(
            text,
            Position {
                line: 0,
                character: 4,
            },
            &PackCompletionCatalog::default(),
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
        let items = completions_at_with_catalog(
            text,
            Position {
                line: 0,
                character: 10,
            },
            &PackCompletionCatalog::default(),
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
            completions_at_with_catalog(
                "Hello world",
                Position {
                    line: 0,
                    character: 5,
                },
                &PackCompletionCatalog::default(),
            )
            .is_none()
        );
    }

    #[test]
    fn completes_phrase_snippet() {
        let text = "\\phr";
        let items = completions_at_with_catalog(
            text,
            Position {
                line: 0,
                character: 4,
            },
            &catalog_demo(),
        )
        .expect("completions");
        let CompletionResponse::Array(items) = items else {
            panic!("expected array");
        };
        assert!(items.iter().any(|i| i.label == "\\phrase"), "{items:?}");
    }

    #[test]
    fn completes_font_snippet() {
        let font = completions_at_with_catalog(
            "\\fon",
            Position {
                line: 0,
                character: 4,
            },
            &catalog_demo(),
        )
        .expect("font");
        let CompletionResponse::Array(font) = font else {
            panic!("expected array");
        };
        assert!(font.iter().any(|i| i.label == "\\font"), "{font:?}");
        assert!(
            !font.iter().any(|i| i.label == "\\arm"),
            "language-specific font aliases are not LSP snippets: {font:?}"
        );
    }

    #[test]
    fn completes_font_ids_from_pack() {
        let items = completions_at_with_catalog(
            "\\font{ar",
            Position {
                line: 0,
                character: 8,
            },
            &catalog_demo(),
        )
        .expect("font ids");
        let CompletionResponse::Array(items) = items else {
            panic!("expected array");
        };
        assert!(items.iter().any(|i| i.label == "armenian"), "{items:?}");
        assert!(!items.iter().any(|i| i.label == "greek"), "{items:?}");
    }

    #[test]
    fn completes_phrase_keys_from_pack() {
        let items = completions_at_with_catalog(
            "\\phrase{ye",
            Position {
                line: 0,
                character: 10,
            },
            &catalog_demo(),
        )
        .expect("phrase keys");
        let CompletionResponse::Array(items) = items else {
            panic!("expected array");
        };
        assert!(items.iter().any(|i| i.label == "yegourdoon"), "{items:?}");
    }

    #[test]
    fn completes_pack_alias() {
        let items = completions_at_with_catalog(
            "\\sho",
            Position {
                line: 0,
                character: 4,
            },
            &catalog_demo(),
        )
        .expect("alias");
        let CompletionResponse::Array(items) = items else {
            panic!("expected array");
        };
        assert!(items.iter().any(|i| i.label == "\\shortname"), "{items:?}");
    }
}
