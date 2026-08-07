use super::*;

fn attach_chunk_id(blocks: &[crate::edit::ContentBlock]) -> u64 {
    blocks
        .iter()
        .find_map(|b| match b {
            crate::edit::ContentBlock::Attachment { chunk_id, .. } => *chunk_id,
            _ => None,
        })
        .expect("attachment")
}

const ATTACH_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn attach_directive(chunk: u64) -> String {
    format!(
        "\\attach{{\n  chunk={chunk}\n  filename=\"notes.pdf\"\n  media_type=application/pdf\n  sha256={ATTACH_SHA}\n}}\n"
    )
}

#[test]
fn splits_free_markdown_and_assigns_sequential_ids() {
    let input = "# Title\n\n- one\n- two\n";
    let out = normalize_tessprek(input).unwrap();
    assert!(out.contains("\\tessera{"), "{out}");
    assert!(out.contains("format=tessprek"), "{out}");
    assert!(out.contains("version=2"), "{out}");
    assert!(out.contains("\\ids{1,2,3}"), "{out}");
    assert!(out.contains("# Title"), "{out}");
    assert!(out.contains("- one"), "{out}");
    assert!(out.contains("- two"), "{out}");
    // No brace directives needed for plain roles.
    assert!(!out.contains("\\text{"), "{out}");
}

#[test]
fn free_markdown_gets_ids_and_stays_markdown() {
    let input = "## Section\n\n1. first\n2. second\n";
    let out = normalize_tessprek(input).unwrap();
    assert!(out.contains("## Section"), "{out}");
    assert!(out.contains("1. first\n2. second"), "{out}");
    assert!(!out.contains("1. first\n\n2. second"), "{out}");
    assert!(out.contains("\\ids{1,2,3}"), "{out}");
}

#[test]
fn nested_list_depth() {
    let input = "- top\n  - nested\n";
    let out = normalize_tessprek(input).unwrap();
    assert!(out.contains("  - nested"), "{out}");
}

#[test]
fn preserves_source_hash_and_code_lang() {
    let input = "\\tessera{format=tessprek version=2 source-hash=abc123}\n\\ids{9}\n\n```rust\nfn x() {}\n```\n";
    let out = normalize_tessprek(input).unwrap();
    assert!(out.contains("source-hash=abc123"), "{out}");
    assert!(out.contains("```rust"), "{out}");
    assert!(out.contains("fn x() {}"), "{out}");
    assert!(out.contains("\\ids{9}"), "{out}");
}

#[test]
fn preserves_rich_tessera_doc_meta() {
    let input = "\
\\tessera{format=tessprek version=2 source-hash=abc doc_id=550e8400-e29b-41d4-a716-446655440099 doc_kind=note title=\"Text roles\" language=en cite_style_id=numeric}\n\
\\ids{1}\n\
\n\
Hi\n\
";
    let out = normalize_tessprek(input).unwrap();
    assert!(out.contains("source-hash=abc"), "{out}");
    assert!(
        out.contains("doc_id=550e8400-e29b-41d4-a716-446655440099"),
        "{out}"
    );
    assert!(out.contains("doc_kind=note"), "{out}");
    assert!(out.contains("title=\"Text roles\""), "{out}");
    assert!(out.contains("language=en"), "{out}");
    assert!(out.contains("cite_style_id=numeric"), "{out}");
    assert!(out.contains("\\ids{1}"), "{out}");
}

#[test]
fn idempotent_on_normalized() {
    let input = "\\tessera{format=tessprek version=2}\n\\ids{1,2}\n\n# Hello\n\n- item\n";
    let once = normalize_tessprek(input).unwrap();
    let twice = normalize_tessprek(&once).unwrap();
    assert_eq!(normalize_newlines(&once), normalize_newlines(&twice));
    assert!(!tessprek_needs_format(&once).unwrap());
}

#[test]
fn gfm_table_stays_table() {
    let input =
        "\\tessera{format=tessprek version=2}\n\\ids{1}\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n";
    let out = normalize_tessprek(input).unwrap();
    assert!(out.contains('|'), "{out}");
    assert!(out.contains("\\ids{1}"), "{out}");
}

#[test]
fn trailing_prose_after_table_becomes_paragraph() {
    let input = "\\tessera{format=tessprek version=2}\n\\ids{1}\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\nI am testing\n";
    let out = normalize_tessprek(input).unwrap();
    assert!(out.contains("I am testing"), "{out}");
    assert!(out.contains("\\ids{1,2}"), "{out}");
}

#[test]
fn second_table_after_blank_line() {
    let input = "\\tessera{format=tessprek version=2}\n\\ids{1}\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n| C | D |\n| --- | --- |\n| 3 | 4 |\n";
    let out = normalize_tessprek(input).unwrap();
    assert!(out.matches('|').count() > 6, "{out}");
    assert!(out.contains("\\ids{1,2}"), "{out}");
    assert!(out.contains('C'), "{out}");
}

#[test]
fn second_table_via_copied_divider_no_blank() {
    let input = "\\tessera{format=tessprek version=2}\n\\ids{1}\n\n| Role | Markdown cue |\n| --- | --- |\n| heading | # Title |\n| list_item | - / 1. |\n| Role | Markdown cue |\n| --- | --- |\n| heading | # Title |\n| list_item | - / 1. |\n";
    let out = normalize_tessprek(input).unwrap();
    assert!(out.contains("\\ids{1,2}"), "{out}");
}

#[test]
fn text_directive_preserves_class_and_align() {
    let input = "\\text{class=\"lead\" align=center}\n# Hello\n";
    let out = normalize_tessprek(input).unwrap();
    assert!(out.contains("class=\"lead\""), "{out}");
    assert!(out.contains("align=center"), "{out}");
    assert!(out.contains("# Hello"), "{out}");
}

#[test]
fn text_directive_preserves_caption_on_table() {
    let input = "\\text{caption=\"Results\"}\n| A | B |\n| --- | --- |\n| 1 | 2 |\n";
    let out = normalize_tessprek(input).unwrap();
    assert!(out.contains("caption=\"Results\""), "{out}");
    assert!(out.contains("| A | B |"), "{out}");
}

#[test]
fn text_directive_preserves_caption_on_code() {
    let input = "\\text{caption=\"Snippet\"}\n```rust\nfn main() {}\n```\n";
    let out = normalize_tessprek(input).unwrap();
    assert!(out.contains("caption=\"Snippet\""), "{out}");
    assert!(out.contains("```rust"), "{out}");
}

#[test]
fn text_directive_rejects_caption_on_heading() {
    let input = "\\text{caption=\"Nope\"}\n# Hello\n";
    assert!(normalize_tessprek(input).is_err());
}

#[test]
fn multiline_text_title_and_caption() {
    let input =
        "\\text{\n  title=\"Listing\"\n  caption=\"Says hi\"\n}\n```rust\nfn main() {}\n```\n";
    let out = normalize_tessprek(input).unwrap();
    assert!(out.contains("title=\"Listing\""), "{out}");
    assert!(out.contains("caption=\"Says hi\""), "{out}");
    assert!(out.contains("\\text{\n"), "{out}");
    assert!(out.contains("```rust"), "{out}");
}

#[test]
fn multiline_figure_round_trip() {
    let input = "\\figure{\n  image=3\n  placement=flow\n  alt=\"alt\"\n  title=\"Hero\"\n  caption=\"A still\"\n}\n";
    let out = normalize_tessprek(input).unwrap();
    assert!(out.contains("title=\"Hero\""), "{out}");
    assert!(out.contains("caption=\"A still\""), "{out}");
    assert!(out.contains("alt=\"alt\""), "{out}");
    assert!(out.contains("\\figure{\n"), "{out}");
    assert!(!out.contains("![alt](media:"), "{out}");
    assert!(out.contains("id=3"), "{out}");
    assert!(out.contains("\\media{\n"), "{out}");
}

#[test]
fn legacy_figure_markdown_body_still_decodes() {
    let input = "\\figure{\n  image=3\n  placement=flow\n}\n![legacy alt](media:3)\n";
    let out = normalize_tessprek(input).unwrap();
    assert!(out.contains("alt=\"legacy alt\""), "{out}");
    assert!(!out.contains("![legacy alt](media:"), "{out}");
}

#[test]
fn normalize_preserves_media_metadata() {
    let input = "\
\\tessera{format=tessprek version=2}\n\
\\ids{1}\n\
\\media{\n\
  id=3\n\
  media_type=image/png\n\
  sha256=abc123\n\
  width=2\n\
  height=2\n\
}\n\
\n\
\\figure{image=3 placement=flow alt=\"alt\"}\n\
";
    let out = normalize_tessprek(input).unwrap();
    assert!(out.contains("  media_type=image/png\n"), "{out}");
    assert!(out.contains("  sha256=abc123\n"), "{out}");
    assert!(out.contains("  width=2\n"), "{out}");
    assert!(out.contains("  height=2\n"), "{out}");
}

#[test]
fn normalize_preserves_two_media_entries() {
    let input = "\
\\tessera{format=tessprek version=2}\n\
\\ids{1,2}\n\
\\media{\n\
  id=3\n\
  media_type=image/png\n\
  sha256=aaa\n\
  width=1\n\
  height=1\n\
\n\
  id=4\n\
  media_type=image/jpeg\n\
  sha256=bbb\n\
  width=2\n\
  height=2\n\
}\n\
\n\
\\figure{image=3 placement=flow alt=\"a\"}\n\
\n\
\\figure{image=4 placement=flow alt=\"b\"}\n\
";
    let out = normalize_tessprek(input).unwrap();
    assert!(out.contains("id=3"), "{out}");
    assert!(out.contains("id=4"), "{out}");
    assert!(out.contains("sha256=aaa"), "{out}");
    assert!(out.contains("sha256=bbb"), "{out}");
    assert!(out.contains("image/jpeg"), "{out}");
}

#[test]
fn parks_biblio_cites_after_prose_and_quotes() {
    let input = "\
\\tessera{format=tessprek version=2}\n\
\\ids{1,2,3}\n\
\n\
\\cite{label=mid author=\"Ada\" title=\"Paper\" year=2020}\n\
\n\
Hello world.\n\
\n\
\\quote{target_chunk=9 quote=\"excerpt\"}\n\
";
    let out = normalize_tessprek(input).unwrap();
    let hello = out
        .find("Hello world.")
        .unwrap_or_else(|| panic!("missing Hello world: {out}"));
    let quote = out
        .find("\\quote{")
        .unwrap_or_else(|| panic!("missing quote: {out}"));
    let cite = out
        .find("\\cite{")
        .unwrap_or_else(|| panic!("missing cite: {out}"));
    assert!(hello < quote, "{out}");
    assert!(quote < cite, "{out}");
    assert!(out.contains("label=mid"), "{out}");
    // Ids follow blocks after park, but keep pre-park identities (cite stays 1).
    assert!(out.contains("\\ids{2,3,1}"), "{out}");
}

#[test]
fn format_keeps_attachment_id_when_parking_biblio() {
    let input = format!(
        "\
\\tessera{{format=tessprek version=2}}\n\
\\ids{{10,20,30}}\n\
\n\
\\cite{{label=book author=\"Ada\" title=\"Paper\" year=2020}}\n\
\n\
Hello.\n\
\n\
{}",
        attach_directive(30)
    );
    let out = normalize_tessprek(&input).unwrap();
    assert_eq!(
        attach_chunk_id(&crate::edit::decode_tessprek(&out).unwrap()),
        30
    );
    assert!(out.contains("chunk=30"), "{out}");
}

#[test]
fn format_keeps_attach_chunk_when_inserting_before() {
    // Stale `\\ids` length after inserting a paragraph before attach.
    let edited = format!(
        "\
\\tessera{{format=tessprek version=2}}\n\
\\ids{{1,2}}\n\
\n\
Hello.\n\
\n\
Inserted.\n\
\n\
{}",
        attach_directive(2)
    );
    let out = normalize_tessprek(&edited).unwrap();
    let blocks = crate::edit::decode_tessprek(&out).unwrap();
    assert_eq!(blocks.len(), 3, "{out}");
    assert_eq!(attach_chunk_id(&blocks), 2);
    assert!(
        out.contains("\\ids{1,3,2}") || out.contains("\\ids{1,4,2}"),
        "{out}"
    );
}
