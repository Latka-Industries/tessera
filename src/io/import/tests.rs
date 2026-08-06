use std::path::PathBuf;

use super::*;
use crate::catalog::{InlineKind, ListKind, TesFile, TextAlign, TextHeader, TextRole};
use crate::error::TesError;
use crate::io::export::{ExportOptions, ExportView, export_view};
use tempfile::tempdir;

#[test]
fn parses_nested_list_depth_from_markdown() {
    let md = concat!(
        "- top\n",
        "  - nested\n",
        "    - deeper\n",
        "1. ordered top\n",
        "   1. ordered nested\n",
    );
    let blocks = parse_markdown_blocks(md);
    let items: Vec<_> = blocks
        .iter()
        .filter(|b| b.header.role == TextRole::ListItem)
        .collect();
    assert!(items.len() >= 4, "got {} items: {items:?}", items.len());
    assert_eq!(items[0].header.list_depth_or_default(), 1);
    assert_eq!(items[1].header.list_depth_or_default(), 2);
    assert_eq!(items[2].header.list_depth_or_default(), 3);
}

#[test]
fn parses_underline_html_into_inline_span() {
    let blocks = parse_markdown_blocks("Note the <u>underlined</u> word.\n");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].body, "Note the underlined word.");
    assert_eq!(blocks[0].header.spans.len(), 1);
    assert_eq!(blocks[0].header.spans[0].kind, InlineKind::Underline);
    assert_eq!(
        &blocks[0].body
            [blocks[0].header.spans[0].start as usize..blocks[0].header.spans[0].end as usize],
        "underlined"
    );
}

#[test]
fn parses_emphasis_and_strong_into_inline_spans() {
    let blocks = parse_markdown_blocks("Say *I am Yes* and **bold**.\n");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].body, "Say I am Yes and bold.");
    assert!(
        blocks[0]
            .header
            .spans
            .iter()
            .any(|s| s.kind == InlineKind::Emphasis
                && &blocks[0].body[s.start as usize..s.end as usize] == "I am Yes"),
        "{:?}",
        blocks[0].header.spans
    );
    assert!(
        blocks[0]
            .header
            .spans
            .iter()
            .any(|s| s.kind == InlineKind::Strong
                && &blocks[0].body[s.start as usize..s.end as usize] == "bold"),
        "{:?}",
        blocks[0].header.spans
    );
}

#[test]
fn parses_commonmark_subset_into_semantic_blocks() {
    let md = concat!(
        "# Methods\n\n",
        "A **bold** paragraph with [a link](https://example.com).\n\n",
        "> Quoted *text*.\n\n",
        "1. First\n",
        "2. Second\n\n",
        "```rust\nlet x = 1;\n```\n",
    );
    let blocks = parse_markdown_blocks(md);
    assert_eq!(blocks.len(), 6);
    assert_eq!(blocks[0].header, TextHeader::heading(1));
    assert_eq!(blocks[0].body, "Methods");
    assert_eq!(blocks[1].body, "A bold paragraph with a link.");
    assert_eq!(blocks[1].pending_links.len(), 1);
    assert_eq!(blocks[1].pending_links[0].dest, "https://example.com");
    assert_eq!(
        &blocks[1].body
            [blocks[1].pending_links[0].start as usize..blocks[1].pending_links[0].end as usize],
        "a link"
    );
    assert_eq!(blocks[2].header.role, TextRole::Blockquote);
    assert_eq!(blocks[3].header.list_kind, Some(ListKind::Ordered));
    assert_eq!(blocks[5].header.role, TextRole::CodeBlock);
    assert_eq!(blocks[5].body, "let x = 1;");
}

#[test]
fn parses_gfm_pipe_table_into_table_data() {
    let md = concat!(
        "| Name | Score |\n",
        "| :--- | ----: |\n",
        "| Ada | 10 |\n",
        "| Bob | 7 |\n",
    );
    let blocks = parse_markdown_blocks(md);
    assert_eq!(blocks.len(), 1, "{blocks:?}");
    assert_eq!(blocks[0].header.role, TextRole::Table);
    assert!(blocks[0].body.is_empty());
    let table = blocks[0].header.table.as_ref().expect("TableData");
    assert_eq!(table.rows.len(), 3);
    assert!(table.rows[0].cells[0].is_header);
    assert_eq!(table.rows[0].cells[0].text, "Name");
    assert_eq!(table.rows[0].cells[0].align, Some(TextAlign::Start));
    assert_eq!(table.rows[0].cells[1].text, "Score");
    assert_eq!(table.rows[0].cells[1].align, Some(TextAlign::End));
    assert!(!table.rows[1].cells[0].is_header);
    assert_eq!(table.rows[1].cells[0].text, "Ada");
    assert_eq!(table.rows[2].cells[1].text, "7");
}

#[test]
fn imports_obsidian_pipe_table_round_trips_as_table_role() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("table.md");
    let output = dir.path().join("table.tes");
    std::fs::write(&input, "# Notes\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n").unwrap();
    let report = import_markdown_v0(&input, &output, &MarkdownImportOptions::default()).unwrap();
    assert_eq!(report.chunk_count, 2);

    let file = TesFile::open(&output).unwrap();
    let mut saw_table = false;
    for entry in file.chunks() {
        if entry.chunk_type != crate::catalog::ChunkType::Text {
            continue;
        }
        let raw = file.decode_payload(entry).unwrap();
        let (header, body) = crate::catalog::decode_text_payload(&raw).unwrap();
        if header.role == TextRole::Table {
            saw_table = true;
            assert!(body.is_empty());
            let table = header.table.as_ref().expect("TableData on header");
            assert_eq!(table.rows.len(), 2);
            assert_eq!(table.rows[0].cells[0].text, "A");
            assert_eq!(table.rows[1].cells[1].text, "2");
        }
    }
    assert!(saw_table, "expected a table text chunk");
}

#[test]
fn imports_minimal_fixture_and_round_trips_views() {
    let dir = tempdir().unwrap();
    let input =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/assets/markdown/minimal.md");
    let output = dir.path().join("minimal.tes");
    let report = import_markdown_v0(&input, &output, &MarkdownImportOptions::default()).unwrap();
    assert_eq!(report.title, "Minimal note");
    assert_eq!(report.chunk_count, 2);

    let linear = export_view(&output, ExportView::Linear, &ExportOptions::default()).unwrap();
    assert_eq!(
        linear,
        "# Minimal note\n\nLorem ipsum dolor sit amet, consectetur adipiscing elit.\n"
    );
    let ai = export_view(&output, ExportView::AiText, &ExportOptions::default()).unwrap();
    assert_eq!(
        ai,
        "Minimal note\n\nLorem ipsum dolor sit amet, consectetur adipiscing elit.\n"
    );
}

#[test]
fn external_https_link_round_trips_through_tes() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("linked.md");
    let output = dir.path().join("linked.tes");
    std::fs::write(
        &input,
        "# Linked\n\nSee [the docs](https://example.com/path) for more.\n",
    )
    .unwrap();
    import_markdown_v0(&input, &output, &MarkdownImportOptions::default()).unwrap();

    let file = crate::catalog::TesFile::open(&output).unwrap();
    assert_eq!(file.links().len(), 1);
    assert_eq!(
        file.links()[0].external_uri(),
        Some("https://example.com/path")
    );

    let md = export_view(&output, ExportView::Markdown, &ExportOptions::default()).unwrap();
    assert!(md.contains("[the docs](https://example.com/path)"));

    let html = export_view(&output, ExportView::Html, &ExportOptions::default()).unwrap();
    assert!(html.contains("href=\"https://example.com/path\""));

    let tessprek = crate::edit::encode_tessprek(&file, "deadbeef").unwrap();
    assert!(tessprek.contains("[the docs](https://example.com/path)"));
}

#[test]
fn front_matter_title_wins_over_heading() {
    let (front, body) = parse_front_matter("---\ntitle: \"Front\"\n---\n# Heading\n");
    assert_eq!(front.title.as_deref(), Some("Front"));
    assert_eq!(body, "# Heading\n");
}

#[test]
fn parses_obsidian_front_matter_lists() {
    let (front, _) = parse_front_matter(
        "---\nid: Erasure\ntags:\n  - Books\n  - Fiction\naliases:\n  - American Fiction\n---\n# Erasure\n",
    );
    assert_eq!(front.id.as_deref(), Some("Erasure"));
    assert_eq!(front.tags, vec!["Books", "Fiction"]);
    assert_eq!(front.aliases, vec!["American Fiction"]);
}

#[test]
fn rewrite_wikilinks_resolves_known_targets() {
    let out = rewrite_wikilinks("See [[Erasure|the novel]] and [[Missing]].", &|name| {
        if name == "Erasure" {
            Some("550e8400-e29b-41d4-a716-446655440000".into())
        } else {
            None
        }
    });
    assert!(out.contains("[the novel](550e8400-e29b-41d4-a716-446655440000)"));
    assert!(out.contains("[[Missing]]"));
}

#[test]
fn import_keeps_existing_doc_id_on_reimport() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("note.md");
    let output = dir.path().join("note.tes");
    std::fs::write(&input, "---\nid: Stable\n---\n# Hello\n\nBody.\n").unwrap();
    let first = import_markdown_v0(
        &input,
        &output,
        &MarkdownImportOptions {
            doc_id_seed: Some("Stable".into()),
            ..MarkdownImportOptions::default()
        },
    )
    .unwrap();
    std::fs::write(&input, "---\nid: Stable\n---\n# Hello\n\nChanged.\n").unwrap();
    let second = import_markdown_v0(
        &input,
        &output,
        &MarkdownImportOptions {
            doc_id_seed: Some("other-seed".into()),
            ..MarkdownImportOptions::default()
        },
    )
    .unwrap();
    assert_eq!(first.doc_id, second.doc_id);
    assert_eq!(first.slug.as_deref(), Some("Stable"));
}

#[test]
fn rejects_invalid_explicit_doc_id() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("note.md");
    let output = dir.path().join("note.tes");
    std::fs::write(&input, "# Note\n").unwrap();
    let options = MarkdownImportOptions {
        doc_id: Some("not-a-uuid".to_owned()),
        ..Default::default()
    };
    let err = import_markdown_v0(input, output, &options).unwrap_err();
    assert!(matches!(err, TesError::InvalidDocId { .. }));
}
