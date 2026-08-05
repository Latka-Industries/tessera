//! Foreign-format importers under [`crate::io`].
//!
//! v0 implements the `CommonMark` subset from `docs/decisions.md`: ATX headings,
//! paragraphs, lists, fenced code, and blockquotes, plus GFM pipe tables
//! (`pulldown_cmark::Options::ENABLE_TABLES` into [`crate::catalog::TextRole::Table`]
//! with [`crate::catalog::TableData`]). Inline presentation is parsed once and
//! flattened into clean canonical text.
//!
//! Obsidian front matter (`id` / tags / aliases), deterministic `doc_id` seeds,
//! and `[[wikilink]]` rewrite helpers support vault batch import.
//!
//! HTML import lives in [`html`].

mod front_matter;
mod markdown;
mod parser;
mod types;
mod wikilinks;

pub mod html;

pub use front_matter::parse_front_matter;
pub use html::{HtmlImportOptions, HtmlImportReport, import_html_v0, parse_html_blocks};
pub use markdown::{import_markdown_v0, resolve_import_doc_id, seal_text_blocks};
pub use parser::parse_markdown_blocks;
pub use types::{
    MarkdownBlock, MarkdownFrontMatter, MarkdownImportOptions, MarkdownImportReport,
    WikilinkResolver,
};
pub use wikilinks::{
    WikilinkSpan, collect_unresolved_wikilinks, rewrite_wikilinks, visit_wikilinks,
};

#[cfg(test)]
mod tests;
