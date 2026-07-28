//! Vault graph value types ([`VaultDocument`], [`Backlink`], …).

use std::path::PathBuf;

use serde::Serialize;

/// One document known to a [`Vault`](super::Vault).
#[derive(Debug, Clone, Serialize)]
pub struct VaultDocument {
    /// Stable UUID.
    pub doc_id: String,
    /// Display title.
    pub title: String,
    /// Document kind string.
    pub doc_kind: String,
    /// File path.
    pub path: PathBuf,
    /// Number of indexed chunks.
    pub chunk_count: usize,
}

/// One inbound graph edge.
#[derive(Debug, Clone, Serialize)]
pub struct Backlink {
    /// Source document UUID.
    pub source_doc_id: String,
    /// Source title.
    pub source_title: String,
    /// Source file.
    pub source_path: PathBuf,
    /// Source chunk containing the anchor.
    pub source_chunk_id: u64,
    /// Target document UUID.
    pub target_doc_id: String,
    /// Target chunk (`0` = whole document).
    pub target_chunk_id: u64,
    /// Edge semantics.
    pub link_kind: &'static str,
}

/// Result of resolving `UUID[/chunk]`.
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedTarget {
    /// Target document.
    pub document: VaultDocument,
    /// Requested chunk, when any.
    pub chunk_id: Option<u64>,
    /// Text body for a text chunk.
    pub text: Option<String>,
    /// Semantic role for a text chunk.
    pub role: Option<&'static str>,
}

/// Broken graph edge found by [`Vault::check`](super::Vault::check).
#[derive(Debug, Clone, Serialize)]
pub struct BrokenLink {
    /// Source document.
    pub source_doc_id: String,
    /// Source chunk.
    pub source_chunk_id: u64,
    /// Missing target document.
    pub target_doc_id: String,
    /// Target chunk (`0` = whole document).
    pub target_chunk_id: u64,
    /// Why resolution failed.
    pub message: String,
}
