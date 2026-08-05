//! Markdown import options, reports, and intermediate blocks.

use std::path::PathBuf;

use crate::catalog::{OutboundLink, TextHeader};
use crate::layout::DocKind;

/// Shared wikilink name → catalog `doc_id` resolver (vault batch import).
pub type WikilinkResolver = std::sync::Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Options for Markdown → `.tes` import.
#[derive(Clone)]
pub struct MarkdownImportOptions {
    /// Kind stored in the superblock and catalog.
    pub doc_kind: DocKind,
    /// Catalog title. When absent, front matter, first heading, or filename wins.
    pub title: Option<String>,
    /// Stable document UUID string. When absent: keep existing output catalog
    /// id (D2), else `UUIDv5` from [`Self::doc_id_seed`], else random.
    pub doc_id: Option<String>,
    /// Seed for deterministic [`crate::catalog::doc_id_from_seed`] when `doc_id` is absent and
    /// the output file does not already exist.
    pub doc_id_seed: Option<String>,
    /// Catalog tags (merged with front matter tags; options win on duplicates
    /// by extending after front matter).
    pub tags: Vec<String>,
    /// Catalog category override (e.g. vault top-level folder).
    pub category: Option<String>,
    /// Catalog section override (path under category, e.g. `Books/Authors`).
    pub section: Option<String>,
    /// Catalog aliases (merged with front matter aliases).
    pub aliases: Vec<String>,
    /// Catalog slug override (else front matter `id:`).
    pub slug: Option<String>,
    /// When true, [`Self::slug`] is authoritative even when `None` (skips front matter id).
    pub slug_override: bool,
    /// When set, rewrite resolved `[[wikilinks]]` to Markdown UUID links before parse.
    pub wikilink_resolver: Option<WikilinkResolver>,
}

impl Default for MarkdownImportOptions {
    fn default() -> Self {
        Self {
            doc_kind: DocKind::Document,
            title: None,
            doc_id: None,
            doc_id_seed: None,
            tags: Vec::new(),
            category: None,
            section: None,
            aliases: Vec::new(),
            slug: None,
            slug_override: false,
            wikilink_resolver: None,
        }
    }
}

impl std::fmt::Debug for MarkdownImportOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarkdownImportOptions")
            .field("doc_kind", &self.doc_kind)
            .field("title", &self.title)
            .field("doc_id", &self.doc_id)
            .field("doc_id_seed", &self.doc_id_seed)
            .field("tags", &self.tags)
            .field("category", &self.category)
            .field("section", &self.section)
            .field("aliases", &self.aliases)
            .field("slug", &self.slug)
            .field("slug_override", &self.slug_override)
            .field(
                "wikilink_resolver",
                &self.wikilink_resolver.as_ref().map(|_| "<fn>"),
            )
            .finish()
    }
}

/// Parsed Obsidian-style YAML front matter (subset).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarkdownFrontMatter {
    /// Optional title.
    pub title: Option<String>,
    /// Obsidian string id (maps to slug / `doc_id` seed).
    pub id: Option<String>,
    /// Tags list.
    pub tags: Vec<String>,
    /// Aliases list.
    pub aliases: Vec<String>,
}

/// Result summary for a completed import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownImportReport {
    /// Source Markdown path.
    pub input: PathBuf,
    /// Sealed `.tes` path.
    pub output: PathBuf,
    /// Stable document id written to the catalog.
    pub doc_id: String,
    /// Catalog title.
    pub title: String,
    /// Number of semantic text chunks written.
    pub chunk_count: usize,
    /// Catalog slug when set.
    pub slug: Option<String>,
}

/// One semantic block produced by the Markdown parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownBlock {
    /// Tessera semantic header.
    pub header: TextHeader,
    /// Clean body text.
    pub body: String,
    /// Outbound links over [`Self::body`] (written to `TLNK` on seal).
    pub pending_links: Vec<OutboundLink>,
}
