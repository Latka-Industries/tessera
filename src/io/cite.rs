//! Cite-key indexing + inline citation projection under [`crate::io`].
//!
//! Canonical biblio data still lives on type-`4` cite chunks ([`CitePayload`]).
//! Tessprek `\cite{key}` extraction lives in [`crate::edit::tessprek`] (THI-321).

use std::collections::{BTreeMap, HashMap};

use crate::catalog::chunk::CitePayload;
use crate::catalog::file::TesFile;
use crate::catalog::index::ChunkType;
use crate::error::{Result, TesError};
use crate::io::bib::{format_numeric_marker, format_pandoc_cite};

/// Pending inline cite discovered in Tessprek / Markdown body text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingCite {
    /// Inclusive start byte offset in the rewritten body (over the bare key).
    pub start: u32,
    /// Exclusive end byte offset in the rewritten body.
    pub end: u32,
    /// Cite key (`label` / `source.cite_key`).
    pub key: String,
}

/// In-text citation projection style (catalog `cite_style_id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CiteStyle {
    /// `[1]`, `[2]`, …
    Numeric,
    /// Pandoc Markdown `[@key]`.
    Pandoc,
}

/// Resolve catalog `cite_style_id` (default numeric).
#[must_use]
pub fn parse_cite_style(id: Option<&str>) -> CiteStyle {
    match id.map(str::trim) {
        Some("pandoc") => CiteStyle::Pandoc,
        _ => CiteStyle::Numeric,
    }
}

/// Prefer `label`, then `source.cite_key`.
#[must_use]
pub fn cite_key_from_payload(cite: &CitePayload) -> Option<String> {
    if let Some(label) = cite
        .label
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(label.to_owned());
    }
    cite.source
        .as_ref()
        .map(|s| s.cite_key.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Insert `key → chunk_id`; duplicate keys with different ids error.
///
/// # Errors
///
/// Returns [`TesError::InvalidCite`] when the same key maps to two chunk ids.
pub fn insert_cite_key(index: &mut BTreeMap<String, u64>, key: &str, chunk_id: u64) -> Result<()> {
    let key = key.trim();
    if key.is_empty() {
        return Ok(());
    }
    if let Some(existing) = index.get(key)
        && *existing != chunk_id
    {
        return Err(TesError::InvalidCite {
            message: format!("duplicate cite key '{key}' (chunks {existing} and {chunk_id})"),
        });
    }
    index.insert(key.to_owned(), chunk_id);
    Ok(())
}

/// Build key → cite chunk id from every cite payload in `file`.
///
/// # Errors
///
/// Propagates decode / duplicate-key errors.
pub fn build_cite_key_index(file: &TesFile) -> Result<BTreeMap<String, u64>> {
    let mut index = BTreeMap::new();
    for entry in file.chunks() {
        if entry.chunk_type != ChunkType::Cite {
            continue;
        }
        let raw = file.decode_payload(entry)?;
        let cite = CitePayload::from_bytes(raw.as_ref())?;
        if let Some(key) = cite_key_from_payload(&cite) {
            insert_cite_key(&mut index, &key, entry.chunk_id)?;
        }
    }
    Ok(index)
}

/// Inverse map for Tessprek encode (`chunk_id → key`).
#[must_use]
pub fn cite_id_key_map(index: &BTreeMap<String, u64>) -> BTreeMap<u64, String> {
    index.iter().map(|(k, id)| (*id, k.clone())).collect()
}

/// Best-effort `chunk_id → key` + catalog style (empty keys if index build fails).
#[must_use]
pub fn projection_maps(file: &TesFile) -> (BTreeMap<u64, String>, CiteStyle) {
    let keys = cite_id_key_map(&build_cite_key_index(file).unwrap_or_default());
    let style = parse_cite_style(file.catalog().and_then(|c| c.cite_style_id.as_deref()));
    (keys, style)
}

/// Fallback label when a cite chunk has no key in the index.
#[must_use]
pub fn cite_key_or_fallback(keys: &BTreeMap<u64, String>, cite_chunk_id: u64) -> String {
    keys.get(&cite_chunk_id)
        .cloned()
        .unwrap_or_else(|| format!("chunk-{cite_chunk_id}"))
}

/// Format an inline citation marker for export.
#[must_use]
pub fn format_inline_cite(style: CiteStyle, n: usize, key: &str) -> String {
    match style {
        CiteStyle::Numeric => format_numeric_marker(n),
        CiteStyle::Pandoc => format_pandoc_cite(key),
    }
}

/// Shared view for projecting [`crate::catalog::InlineKind::Citation`] spans.
#[derive(Clone, Copy)]
pub struct CiteProj<'a> {
    pub numbers: &'a HashMap<u64, usize>,
    pub keys: &'a BTreeMap<u64, String>,
    pub style: CiteStyle,
}

impl CiteProj<'_> {
    /// Bibliographic number for `cite_chunk_id` (0 if unknown).
    #[must_use]
    pub fn number(self, cite_chunk_id: u64) -> usize {
        self.numbers.get(&cite_chunk_id).copied().unwrap_or(0)
    }

    /// Cite key, or `chunk-{id}` fallback.
    #[must_use]
    pub fn key(self, cite_chunk_id: u64) -> String {
        cite_key_or_fallback(self.keys, cite_chunk_id)
    }

    /// Formatted in-text marker for this cite chunk.
    #[must_use]
    pub fn marker(self, cite_chunk_id: u64) -> String {
        format_inline_cite(
            self.style,
            self.number(cite_chunk_id),
            &self.key(cite_chunk_id),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_key_errors() {
        let mut idx = BTreeMap::new();
        insert_cite_key(&mut idx, "a", 1).unwrap();
        assert!(insert_cite_key(&mut idx, "a", 2).is_err());
        insert_cite_key(&mut idx, "a", 1).unwrap();
    }

    #[test]
    fn cite_proj_marker_numeric() {
        let numbers = HashMap::from([(7, 2)]);
        let keys = BTreeMap::from([(7, "keller2020".into())]);
        let proj = CiteProj {
            numbers: &numbers,
            keys: &keys,
            style: CiteStyle::Numeric,
        };
        assert_eq!(proj.marker(7), "[2]");
        assert_eq!(proj.key(99), "chunk-99");
    }
}
