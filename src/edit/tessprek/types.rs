use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::brace::take_leading_tessera_header;
use super::markers;
use super::util::{kv_attr, parse_attrs};
use crate::catalog::document::DocumentCatalog;
use crate::catalog::media::ImagePayload;

/// Optional document-identity fields projected into `\tessera{…}`.
///
/// Encode fills these from [`DocumentCatalog`]; decode/normalize accept them for
/// display/LSP only — they do **not** silently overwrite the `.tes` catalog on
/// write-back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TessprekDocMeta {
    /// Hex SHA-256 of the on-disk `.tes` (mutation gate).
    pub source_hash: Option<String>,
    /// Catalog `doc_id`.
    pub doc_id: Option<String>,
    /// Catalog `doc_kind` (e.g. `note`).
    pub doc_kind: Option<String>,
    /// Catalog `title`.
    pub title: Option<String>,
    /// Catalog BCP-47 `language`.
    pub language: Option<String>,
    /// Catalog `cite_style_id`.
    pub cite_style_id: Option<String>,
    /// Catalog `theme_id`.
    pub theme_id: Option<String>,
    /// Catalog `template_id`.
    pub template_id: Option<String>,
    /// Catalog `slug`.
    pub slug: Option<String>,
}

impl TessprekDocMeta {
    /// Project catalog fields (plus optional `source_hash`) into Tessprek meta.
    #[must_use]
    pub fn from_catalog(catalog: &DocumentCatalog, source_hash: Option<&str>) -> Self {
        Self {
            source_hash: nonempty_owned(source_hash),
            doc_id: nonempty_owned(Some(catalog.doc_id.as_str())),
            doc_kind: nonempty_owned(Some(catalog.doc_kind.as_str())),
            title: nonempty_owned(Some(catalog.title.as_str())),
            language: nonempty_owned(catalog.language.as_deref()),
            cite_style_id: nonempty_owned(catalog.cite_style_id.as_deref()),
            theme_id: nonempty_owned(catalog.theme_id.as_deref()),
            template_id: nonempty_owned(catalog.template_id.as_deref()),
            slug: nonempty_owned(catalog.slug.as_deref()),
        }
    }

    /// Read known identity keys from a parsed `\tessera{…}` attr map.
    #[must_use]
    pub fn from_attrs(map: &BTreeMap<String, String>) -> Self {
        Self {
            source_hash: map_get(map, "source-hash"),
            doc_id: map_get(map, "doc_id"),
            doc_kind: map_get(map, "doc_kind"),
            title: map_get(map, "title"),
            language: map_get(map, "language"),
            cite_style_id: map_get(map, "cite_style_id"),
            theme_id: map_get(map, "theme_id"),
            template_id: map_get(map, "template_id"),
            slug: map_get(map, "slug"),
        }
    }

    /// Best-effort parse of the leading `\tessera{…}` header (for pack resolve).
    #[must_use]
    pub fn peek_from_tessprek(tessprek: &str) -> Option<Self> {
        let (map, _) = leading_attr_map(tessprek)?;
        Some(Self::from_attrs(&map))
    }

    /// Keys in `map` that are not in [`markers::TESSERA_HEADER_KEYS`].
    #[must_use]
    pub fn unknown_keys(map: &BTreeMap<String, String>) -> Vec<String> {
        map.keys()
            .filter(|k| !markers::TESSERA_HEADER_KEYS.contains(&k.as_str()))
            .cloned()
            .collect()
    }

    /// Unknown `\tessera{…}` keys in a Tessprek buffer, if any.
    ///
    /// Returns `(1-based header start line, unknown keys)` when the leading
    /// header parses and contains unknown keys.
    #[must_use]
    pub fn unknown_keys_in_buffer(tessprek: &str) -> Option<(usize, Vec<String>)> {
        let (map, start) = leading_attr_map(tessprek)?;
        let unknown = Self::unknown_keys(&map);
        if unknown.is_empty() {
            None
        } else {
            Some((start + 1, unknown))
        }
    }

    pub(super) fn push_parts(&self, parts: &mut Vec<String>) {
        push_plain(parts, "source-hash", self.source_hash.as_deref());
        push_plain(parts, "doc_id", self.doc_id.as_deref());
        push_plain(parts, "doc_kind", self.doc_kind.as_deref());
        push_plain(parts, "title", self.title.as_deref());
        push_plain(parts, "language", self.language.as_deref());
        push_plain(parts, "cite_style_id", self.cite_style_id.as_deref());
        push_plain(parts, "theme_id", self.theme_id.as_deref());
        push_plain(parts, "template_id", self.template_id.as_deref());
        push_plain(parts, "slug", self.slug.as_deref());
    }
}

fn leading_attr_map(tessprek: &str) -> Option<(BTreeMap<String, String>, usize)> {
    let lines: Vec<&str> = tessprek.lines().collect();
    let (attrs, start, _) = take_leading_tessera_header(&lines).ok()?;
    let map = parse_attrs(&attrs, start + 1).ok()?;
    Some((map, start))
}

fn nonempty_owned(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn map_get(map: &BTreeMap<String, String>, key: &str) -> Option<String> {
    map.get(key)
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn push_plain(parts: &mut Vec<String>, key: &str, value: Option<&str>) {
    if let Some(v) = value.filter(|s| !s.is_empty()) {
        parts.push(kv_attr(key, v));
    }
}

/// One image-payload row in the Tessprek `\media{…}` header (not reading-order).
///
/// Bytes stay in the `.tes`; this projects identity metadata so `media:N` is
/// inspectable in the editor. Regenerated on encode from the sealed file;
/// normalize preserves declared attrs when the `.tes` is not open.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TessprekMediaEntry {
    /// Image chunk id (target of `media:N` / `\figure{image=N}`).
    pub chunk_id: u64,
    /// IANA type, e.g. `image/png`.
    pub media_type: Option<String>,
    /// Hex SHA-256 of the image bytes.
    pub sha256: Option<String>,
    /// Intrinsic width in pixels (`None` / omit when unknown).
    pub width_px: Option<u32>,
    /// Intrinsic height in pixels (`None` / omit when unknown).
    pub height_px: Option<u32>,
}

impl TessprekMediaEntry {
    /// Build a full entry from a sealed [`ImagePayload`].
    #[must_use]
    pub fn from_payload(chunk_id: u64, image: &ImagePayload) -> Self {
        Self {
            chunk_id,
            media_type: Some(image.media_type.clone()),
            sha256: Some(image_sha256_hex(&image.data)),
            width_px: (image.width_px > 0).then_some(image.width_px),
            height_px: (image.height_px > 0).then_some(image.height_px),
        }
    }

    /// One `key=value` per line (same shape as `\figure` / `\attach`).
    pub(super) fn attr_parts(&self) -> Vec<String> {
        let mut parts = vec![format!("id={}", self.chunk_id)];
        if let Some(mime) = self.media_type.as_deref().filter(|s| !s.is_empty()) {
            parts.push(kv_attr("media_type", mime));
        }
        if let Some(hash) = self.sha256.as_deref().filter(|s| !s.is_empty()) {
            parts.push(format!("sha256={hash}"));
        }
        if let Some(w) = self.width_px {
            parts.push(format!("width={w}"));
        }
        if let Some(h) = self.height_px {
            parts.push(format!("height={h}"));
        }
        parts
    }
}

/// Parse `\media{…}` inner text into payload rows.
///
/// Accepts legacy `\media{7,12}`, packed one-line entries, pretty one-attr-per-line
/// groups, and the space-flattened form produced by [`take_brace_command`](super::brace::take_brace_command).
#[must_use]
pub(crate) fn parse_media_header(inner: &str) -> Vec<TessprekMediaEntry> {
    let trimmed = inner.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    // Legacy: `\media{7}` / `\media{7,12}`
    if !trimmed.contains('=') {
        return trimmed
            .split(',')
            .filter_map(|s| {
                let id = s.trim().parse::<u64>().ok()?;
                (id != 0).then_some(TessprekMediaEntry {
                    chunk_id: id,
                    ..TessprekMediaEntry::default()
                })
            })
            .collect();
    }

    // Tokenize; each `id=` starts a new entry (works after brace flatten).
    let mut chunks: Vec<String> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for tok in trimmed.split_whitespace() {
        if tok.starts_with("id=") && !cur.is_empty() {
            chunks.push(cur.join(" "));
            cur.clear();
        }
        cur.push(tok);
    }
    if !cur.is_empty() {
        chunks.push(cur.join(" "));
    }
    chunks
        .iter()
        .filter_map(|attrs| media_entry_from_attrs(attrs))
        .collect()
}

fn media_entry_from_attrs(attrs: &str) -> Option<TessprekMediaEntry> {
    let map = parse_attrs(attrs, 1).ok()?;
    let chunk_id = map.get("id")?.parse::<u64>().ok()?;
    (chunk_id != 0).then(|| TessprekMediaEntry {
        chunk_id,
        media_type: map.get("media_type").cloned().filter(|s| !s.is_empty()),
        sha256: map.get("sha256").cloned().filter(|s| !s.is_empty()),
        width_px: map.get("width").and_then(|s| s.parse().ok()),
        height_px: map.get("height").and_then(|s| s.parse().ok()),
    })
}

fn image_sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(data);
    let mut out = String::with_capacity(64);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}
