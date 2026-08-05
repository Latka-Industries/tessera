//! Tessera Markdown (Tessprek) encode/decode for virtual editor buffers.
//!
//! Format (v2): hybrid plain Markdown for heading/paragraph/list/quote/table/
//! math/fenced-code, plus LaTeX-lite brace commands for structured chunks
//! (`\figure{}` / `\cite{}` / `\slide{}` / `\attach{}`) and an optional
//! `\text{title=… caption=… class=… …}` directive before a Markdown block when
//! those attrs cannot live in Markdown itself. Brace commands accept the same
//! multiline form as `\tessera{…}`. See `docs/tessprek.md`.
//!
//! `.tes` stays canonical; Tessprek is a lossy projection only.

mod format;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::catalog::TesFile;
use crate::catalog::chunk::{CitePayload, OrderedListNumbering, TextHeader, decode_text_payload};
use crate::catalog::document::DocumentCatalog;
use crate::catalog::index::ChunkType;
use crate::catalog::media::{AttachmentPayload, FigureRef, ImagePayload, ImagePlacement};
use crate::catalog::slide::{SlidePayload, SlideRegion};
use crate::error::{Result, TesError};

use super::ContentBlock;

pub use format::{normalize_tessprek, tessprek_needs_format};

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
        let lines: Vec<&str> = tessprek.lines().collect();
        let (attrs, start, _) = take_leading_tessera_header(&lines).ok()?;
        let map = parse_attrs(&attrs, start + 1).ok()?;
        let unknown = Self::unknown_keys(&map);
        if unknown.is_empty() {
            None
        } else {
            Some((start + 1, unknown))
        }
    }

    fn push_parts(&self, parts: &mut Vec<String>) {
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
        parts.push(format!("{key}={}", attr_token(v)));
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
    fn attr_parts(&self) -> Vec<String> {
        let mut parts = vec![format!("id={}", self.chunk_id)];
        if let Some(mime) = self.media_type.as_deref().filter(|s| !s.is_empty()) {
            parts.push(format!("media_type={}", attr_token(mime)));
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
/// groups, and the space-flattened form produced by [`take_brace_command`].
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

/// Tessprek v2 wire markers: `\tessera{}` header + `\ids{}` reading order,
/// LaTeX-lite brace commands for structured chunks. Shared by encode/decode
/// and LSP hover. No per-block `\id{N}`, no HTML comments, no YAML front
/// matter, no dual-read of the v1 HTML-comment format.
pub mod markers {
    /// `format=` value stamped in the document header.
    pub const FORMAT: &str = "tessprek";
    /// `version=` value stamped in the document header.
    pub const VERSION: &str = "2";
    /// Document header: `\tessera{format=… version=2 source-hash=… [doc meta…]}`.
    pub const TESSERA_PREFIX: &str = "\\tessera{";
    /// Known `\tessera{…}` attribute keys (wire + projected catalog fields).
    pub const TESSERA_HEADER_KEYS: &[&str] = &[
        "format",
        "version",
        "source-hash",
        "doc_id",
        "doc_kind",
        "title",
        "language",
        "cite_style_id",
        "theme_id",
        "template_id",
        "slug",
    ];
    /// Reading-order chunk id list: `\ids{1,2,3,…}`.
    pub const IDS_PREFIX: &str = "\\ids{";
    /// Media (image payload) header: `\media{ id=7 media_type=… sha256=… }`.
    ///
    /// These are **not** in `\ids{}` (media store, not reading-order blocks).
    /// They are what `media:N` / `\figure{image=N}` point at.
    pub const MEDIA_PREFIX: &str = "\\media{";
    /// Optional preserved-attrs directive before a Markdown block.
    pub const TEXT_PREFIX: &str = "\\text{";
    /// Figure directive: `\figure{image=… placement=… alt=…}` (no Markdown body).
    pub const FIGURE_PREFIX: &str = "\\figure{";
    /// Cite directive: `\cite{label=… target_chunk=…}` + quote body.
    pub const CITE_PREFIX: &str = "\\cite{";
    /// Slide directive: `\slide{layout=… regions=…}`.
    pub const SLIDE_PREFIX: &str = "\\slide{";
    /// Attachment directive: `\attach{filename=… media_type=… sha256=…}`.
    pub const ATTACH_PREFIX: &str = "\\attach{";
    /// Closing delimiter for every brace command.
    pub const BRACE_SUFFIX: &str = "}";

    /// Header-only brace lines (`\tessera` / `\ids` / `\media`).
    pub const HEADER_COMMANDS: &[(&str, &str)] = &[
        (TESSERA_PREFIX, "tessera"),
        (IDS_PREFIX, "ids"),
        (MEDIA_PREFIX, "media"),
    ];

    /// Body brace lines (structured chunks + optional `\text`).
    /// Kind `attachment` matches [`super::decode_named_directive`].
    pub const BODY_COMMANDS: &[(&str, &str)] = &[
        (TEXT_PREFIX, "text"),
        (FIGURE_PREFIX, "figure"),
        (CITE_PREFIX, "cite"),
        (SLIDE_PREFIX, "slide"),
        (ATTACH_PREFIX, "attachment"),
    ];

    /// Wire surface name for completions (`attachment` → `attach`).
    #[must_use]
    pub fn surface_name(kind: &str) -> &str {
        if kind == "attachment" { "attach" } else { kind }
    }

    /// Parse a **single-line** closed brace command → `(kind, inner attrs)`.
    ///
    /// When `include_header` is true, also matches `\tessera{…}` / `\ids{…}`
    /// (LSP hover). For multiline body/header commands use
    /// [`super::take_brace_command`] / [`match_body_opener`].
    #[must_use]
    pub fn parse_brace_command(
        trimmed: &str,
        include_header: bool,
    ) -> Option<(&'static str, &str)> {
        if include_header && let Some(hit) = match_brace_closed(trimmed, HEADER_COMMANDS) {
            return Some(hit);
        }
        match_brace_closed(trimmed, BODY_COMMANDS)
    }

    /// Match a body command opener (`\text{`, `\figure{`, …) even when `}` is
    /// on a later line. Returns `(kind, prefix)`.
    #[must_use]
    pub fn match_body_opener(trimmed: &str) -> Option<(&'static str, &'static str)> {
        for &(prefix, kind) in BODY_COMMANDS {
            if trimmed.starts_with(prefix) {
                return Some((kind, prefix));
            }
        }
        None
    }

    fn match_brace_closed<'a>(
        trimmed: &'a str,
        table: &'static [(&'static str, &'static str)],
    ) -> Option<(&'static str, &'a str)> {
        for &(prefix, kind) in table {
            if let Some(rest) = trimmed.strip_prefix(prefix)
                && let Some(attrs) = rest.strip_suffix(BRACE_SUFFIX)
            {
                return Some((kind, attrs));
            }
        }
        None
    }
}

/// Parse a brace command starting at `lines[start]` (0-based).
///
/// Accepts a single-line `\cmd{…}` or a multiline form:
///
/// ```text
/// \text{
///   title="…"
///   caption="…"
/// }
/// ```
///
/// Returns `(inner attrs, end_line_exclusive)`.
///
/// # Errors
///
/// Returns [`TesError::EditParse`] when the opener is missing or `}` is never
/// closed (respecting quoted attribute values).
pub(crate) fn take_brace_command(
    lines: &[&str],
    start: usize,
    prefix: &str,
    label: &str,
) -> Result<(String, usize)> {
    let header_line_no = start.saturating_add(1);
    let first = lines.get(start).map_or("", |l| l.trim());
    if !first.starts_with(prefix) {
        return Err(parse_err(
            header_line_no,
            1,
            format!("expected `{prefix}...{BRACE_SUFFIX}` {label}, found: {first}"),
        ));
    }

    let mut buf = String::new();
    let mut end = start;
    while end < lines.len() {
        let piece = lines[end].trim();
        if end > start {
            buf.push(' ');
        }
        buf.push_str(piece);
        let after = buf
            .strip_prefix(prefix)
            .expect("prefix checked on first line");
        if let Some(close) = find_unquoted_close_brace(after) {
            let trailing = after[close + 1..].trim();
            if !trailing.is_empty() {
                return Err(parse_err(
                    end + 1,
                    1,
                    format!("trailing junk after `{BRACE_SUFFIX}` in \\{label}: {trailing}"),
                ));
            }
            return Ok((after[..close].to_owned(), end + 1));
        }
        end += 1;
    }
    Err(parse_err(
        header_line_no,
        1,
        format!("unterminated `{prefix}` {label} (missing `{BRACE_SUFFIX}`)"),
    ))
}

/// Parse a `\tessera{…}` header starting at `lines[start]` (0-based).
///
/// # Errors
///
/// Same as [`take_brace_command`].
pub(crate) fn take_tessera_header(lines: &[&str], start: usize) -> Result<(String, usize)> {
    take_brace_command(lines, start, TESSERA_PREFIX, "tessera header")
}

/// Skip leading blank lines; return the first non-blank index (or `lines.len()`).
#[must_use]
pub(crate) fn skip_blank_lines(lines: &[&str], mut i: usize) -> usize {
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }
    i
}

/// Parse the leading `\tessera{…}` header after optional blanks.
///
/// Returns `(attrs, start_line, end_line_exclusive)` where `start_line` is the
/// first header line (0-based).
///
/// # Errors
///
/// Same as [`take_tessera_header`].
pub(crate) fn take_leading_tessera_header(lines: &[&str]) -> Result<(String, usize, usize)> {
    let start = skip_blank_lines(lines, 0);
    let (attrs, end) = take_tessera_header(lines, start)?;
    Ok((attrs, start, end))
}

fn find_unquoted_close_brace(s: &str) -> Option<usize> {
    let mut in_quote = false;
    let mut escape = false;
    for (i, ch) in s.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_quote => escape = true,
            '"' => in_quote = !in_quote,
            '}' if !in_quote => return Some(i),
            _ => {}
        }
    }
    None
}

use markers::{
    ATTACH_PREFIX, BRACE_SUFFIX, CITE_PREFIX, FIGURE_PREFIX, FORMAT, IDS_PREFIX, MEDIA_PREFIX,
    SLIDE_PREFIX, TESSERA_PREFIX, TEXT_PREFIX, VERSION,
};

/// Encode a `.tes` file as Tessprek, embedding `source_hash`.
///
/// # Errors
///
/// Returns decode errors for reading-order text/figure/cite/slide/attachment
/// payloads.
pub fn encode_tessprek(file: &TesFile, source_hash: &str) -> Result<String> {
    let mut blocks = Vec::new();
    for entry in file.reading_order_chunks() {
        let block = match entry.chunk_type {
            ChunkType::Text => {
                let raw = file.decode_payload(entry)?;
                let (header, body) = decode_text_payload(raw.as_ref())?;
                ContentBlock::Text {
                    chunk_id: Some(entry.chunk_id),
                    header,
                    body,
                    pending_links: Vec::new(),
                }
            }
            ChunkType::Figure => {
                let raw = file.decode_payload(entry)?;
                let figure = FigureRef::from_bytes(raw.as_ref())?;
                ContentBlock::Figure {
                    chunk_id: Some(entry.chunk_id),
                    figure,
                }
            }
            ChunkType::Cite => {
                let raw = file.decode_payload(entry)?;
                let cite = CitePayload::from_bytes(raw.as_ref())?;
                ContentBlock::Cite {
                    chunk_id: Some(entry.chunk_id),
                    cite,
                }
            }
            ChunkType::Slide => {
                let raw = file.decode_payload(entry)?;
                let slide = SlidePayload::from_bytes(raw.as_ref())?;
                ContentBlock::Slide {
                    chunk_id: Some(entry.chunk_id),
                    slide,
                }
            }
            ChunkType::Attachment => {
                let raw = file.decode_payload(entry)?;
                let att = AttachmentPayload::from_bytes(raw.as_ref())?;
                ContentBlock::Attachment {
                    chunk_id: Some(entry.chunk_id),
                    filename: att.filename,
                    media_type: att.media_type,
                    caption: att.caption,
                    sha256: att.sha256,
                }
            }
            _ => continue,
        };
        blocks.push(block);
    }
    let media = media_entries_from_file(file, &blocks);
    Ok(encode_content_blocks(
        &file.catalog().map_or_else(
            || TessprekDocMeta {
                source_hash: Some(source_hash.to_owned()),
                ..TessprekDocMeta::default()
            },
            |catalog| TessprekDocMeta::from_catalog(catalog, Some(source_hash)),
        ),
        &blocks,
        file.links(),
        &media,
    ))
}

/// Collect `\media{…}` rows for figure-referenced image payloads in `file`.
fn media_entries_from_file(file: &TesFile, blocks: &[ContentBlock]) -> Vec<TessprekMediaEntry> {
    let mut ids = std::collections::BTreeSet::new();
    for block in blocks {
        if let ContentBlock::Figure { figure, .. } = block
            && figure.image_chunk_id != 0
        {
            ids.insert(figure.image_chunk_id);
        }
    }
    ids.into_iter()
        .map(|id| match file.chunk_by_id(id) {
            Ok(entry) if entry.chunk_type == ChunkType::Image => file
                .decode_payload(entry)
                .ok()
                .and_then(|raw| ImagePayload::from_bytes(raw.as_ref()).ok())
                .map_or_else(
                    || TessprekMediaEntry {
                        chunk_id: id,
                        ..TessprekMediaEntry::default()
                    },
                    |image| TessprekMediaEntry::from_payload(id, &image),
                ),
            _ => TessprekMediaEntry {
                chunk_id: id,
                ..TessprekMediaEntry::default()
            },
        })
        .collect()
}

/// Parse Tessprek v2 into typed content blocks.
///
/// Strict: requires a `\tessera{format=tessprek version=2 …}` header
/// immediately followed by `\ids{…}`, and the id count must match the number
/// of parsed blocks.
///
/// # Errors
///
/// Returns [`TesError::EditParse`] with line/column on malformed directives,
/// bodies, missing header/ids, or an id/block count mismatch.
pub fn decode_tessprek(input: &str) -> Result<Vec<ContentBlock>> {
    Ok(decode_tessprek_with_spans(input)?
        .into_iter()
        .map(|(_, _, block)| block)
        .collect())
}

/// Decode Tessprek with 0-based half-open line spans per block (for LSP hover).
///
/// # Errors
///
/// Same as [`decode_tessprek`].
pub(crate) fn decode_tessprek_with_spans(input: &str) -> Result<Vec<(usize, usize, ContentBlock)>> {
    let lines: Vec<&str> = input.lines().collect();

    let (header_inner, header_start, header_end) = take_leading_tessera_header(&lines)?;
    let header_line_no = header_start + 1;
    let header_attrs = parse_attrs(&header_inner, header_line_no)?;
    if header_attrs.get("format").map(String::as_str) != Some(FORMAT) {
        return Err(parse_err(
            header_line_no,
            1,
            format!("unsupported tessprek header (expected format={FORMAT})"),
        ));
    }
    if header_attrs.get("version").map(String::as_str) != Some(VERSION) {
        return Err(parse_err(
            header_line_no,
            1,
            format!(
                "unsupported tessprek version (expected version={VERSION}); v1 HTML-comment Tessprek is no longer supported"
            ),
        ));
    }
    let i = skip_blank_lines(&lines, header_end);
    let ids_line_no = i + 1;
    let ids_trimmed = lines.get(i).map_or("", |l| l.trim());
    let Some(("ids", ids_inner)) = markers::parse_brace_command(ids_trimmed, true) else {
        return Err(parse_err(
            ids_line_no,
            1,
            format!(
                "expected `{IDS_PREFIX}...{BRACE_SUFFIX}` reading-order id list, found: {ids_trimmed}"
            ),
        ));
    };
    let ids = parse_ids_list(ids_inner, ids_line_no)?;

    let mut spanned = format::build_content_blocks_with_spans(&lines)?;
    if spanned.len() != ids.len() {
        return Err(parse_err(
            ids_line_no,
            1,
            format!(
                "`{IDS_PREFIX}...{BRACE_SUFFIX}` declares {} id(s) but document has {} block(s); \
                 run `:TesseraFormat` (or enable format-on-save) / `tes format` to refresh `\\ids{{}}`",
                ids.len(),
                spanned.len()
            ),
        ));
    }
    for ((_, _, block), id) in spanned.iter_mut().zip(ids) {
        set_chunk_id(block, id);
    }
    Ok(spanned)
}

fn parse_ids_list(inner: &str, line_no: usize) -> Result<Vec<u64>> {
    let inner = inner.trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    inner
        .split(',')
        .map(|s| {
            let s = s.trim();
            s.parse::<u64>().map_err(|_| {
                parse_err(
                    line_no,
                    1,
                    format!("invalid id '{s}' in `{IDS_PREFIX}...{BRACE_SUFFIX}`"),
                )
            })
        })
        .collect()
}

pub(super) fn set_chunk_id(block: &mut ContentBlock, id: u64) {
    match block {
        ContentBlock::Text { chunk_id, .. }
        | ContentBlock::Figure { chunk_id, .. }
        | ContentBlock::Cite { chunk_id, .. }
        | ContentBlock::Slide { chunk_id, .. }
        | ContentBlock::Attachment { chunk_id, .. } => *chunk_id = Some(id),
    }
}

/// Dispatch a brace-command body (`figure` / `cite` / `slide` / `attachment`)
/// to its typed decoder. `kind` comes from which prefix matched during
/// scanning (see [`markers::parse_brace_command`]), not from a `type=` attribute.
fn decode_named_directive(
    kind: &str,
    map: &BTreeMap<String, String>,
    body: &str,
    line_no: usize,
) -> Result<ContentBlock> {
    match kind {
        "figure" => decode_figure_block(map, body, line_no),
        "cite" => Ok(decode_cite_block(map, body)),
        "slide" => decode_slide_block(map, line_no),
        "attachment" => decode_attachment_block(map, line_no),
        other => Err(parse_err(
            line_no,
            1,
            format!("unknown tessprek directive '\\{other}{{...}}'"),
        )),
    }
}

fn decode_figure_block(
    map: &BTreeMap<String, String>,
    body: &str,
    line_no: usize,
) -> Result<ContentBlock> {
    let image_chunk_id = required_u64(map, "image", line_no)?;
    let placement = parse_placement(
        map.get("placement").map_or("flow", String::as_str),
        map.get("region").map(String::as_str),
        line_no,
    )?;
    let title = map.get("title").cloned().filter(|s| !s.is_empty());
    let caption = map.get("caption").cloned().filter(|s| !s.is_empty());
    let alt_attr = map.get("alt").cloned().filter(|s| !s.is_empty());
    // Legacy: body `![alt](media:N)` after `\figure{…}` (pre-alt-attr Tessprek).
    let (alt_md, img_from_md) = if body.trim().is_empty() {
        (None, None)
    } else {
        let (alt, id) = parse_figure_markdown(body, line_no)?;
        (Some(alt), id)
    };
    let alt_text = alt_md.or(alt_attr).ok_or_else(|| {
        parse_err(
            line_no,
            1,
            "figure requires alt=\"…\" (or legacy ![alt](media:N) body)",
        )
    })?;
    let image_chunk_id = img_from_md.unwrap_or(image_chunk_id);
    Ok(ContentBlock::Figure {
        chunk_id: None,
        figure: FigureRef {
            image_chunk_id,
            alt_text,
            title,
            caption,
            placement,
        },
    })
}

fn decode_cite_block(map: &BTreeMap<String, String>, body: &str) -> ContentBlock {
    ContentBlock::Cite {
        chunk_id: None,
        cite: CitePayload {
            quote: strip_quote_body(body),
            target_doc_id: map.get("target_doc").cloned().filter(|s| !s.is_empty()),
            target_chunk_id: optional_u64(map, "target_chunk"),
            target_byte_start: None,
            target_byte_end: None,
            label: map.get("label").cloned().filter(|s| !s.is_empty()),
            page: optional_u32(map, "page"),
            source: None,
        },
    }
}

fn decode_slide_block(map: &BTreeMap<String, String>, line_no: usize) -> Result<ContentBlock> {
    let layout_id = map
        .get("layout")
        .cloned()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| parse_err(line_no, 1, "slide requires layout=…"))?;
    let regions = parse_slide_regions(map.get("regions").map_or("", String::as_str), line_no)?;
    Ok(ContentBlock::Slide {
        chunk_id: None,
        slide: SlidePayload { layout_id, regions },
    })
}

fn decode_attachment_block(map: &BTreeMap<String, String>, line_no: usize) -> Result<ContentBlock> {
    let filename = map
        .get("filename")
        .cloned()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| parse_err(line_no, 1, "attachment requires filename=…"))?;
    let media_type = map
        .get("media_type")
        .cloned()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| parse_err(line_no, 1, "attachment requires media_type=…"))?;
    let sha256 = map
        .get("sha256")
        .cloned()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| parse_err(line_no, 1, "attachment requires sha256=…"))?;
    Ok(ContentBlock::Attachment {
        chunk_id: None,
        filename,
        media_type,
        caption: map.get("caption").cloned().filter(|s| !s.is_empty()),
        sha256,
    })
}

/// Encode typed content blocks as Tessprek v2.
///
/// `meta` supplies `source-hash` and optional catalog identity fields for the
/// `\tessera{…}` header (encode prefers a multiline block). `links` resolves
/// `InlineKind::Link` spans on blocks whose `pending_links` is empty (e.g.
/// blocks freshly decoded from a `.tes` file); pass `&[]` when blocks already
/// carry `pending_links` (normalize / typed ops).
///
/// Used by [`normalize_tessprek`], [`encode_tessprek`], and tests.
///
/// `media` supplies `\media{…}` rows (mime / sha256 / dimensions). When empty,
/// figure-referenced ids are still emitted as bare `id=N` rows.
#[must_use]
pub fn encode_content_blocks(
    meta: &TessprekDocMeta,
    blocks: &[ContentBlock],
    links: &[crate::catalog::LinkEntry],
    media: &[TessprekMediaEntry],
) -> String {
    let mut out = String::new();
    write_header(&mut out, meta);
    write_ids(&mut out, blocks);
    write_media(&mut out, blocks, media);
    out.push('\n');

    let mut ordered = OrderedListNumbering::default();
    for (i, block) in blocks.iter().enumerate() {
        let next = blocks.get(i + 1);
        match block {
            ContentBlock::Text {
                header,
                body,
                pending_links,
                ..
            } => {
                let ordered_index = ordered.take_for_text(header);
                write_text_directive(&mut out, header);
                out.push_str(
                    render_text_body(header, body, pending_links, links, ordered_index).trim_end(),
                );
                // Tight lists: consecutive list items share a single newline
                // (CommonMark). Blank line separates other blocks / list runs.
                out.push_str(
                    if block.is_list_item() && next.is_some_and(ContentBlock::is_list_item) {
                        "\n"
                    } else {
                        "\n\n"
                    },
                );
            }
            other => {
                ordered.clear();
                match other {
                    ContentBlock::Figure { figure, .. } => {
                        write_figure_directive(&mut out, figure);
                        out.push('\n');
                    }
                    ContentBlock::Cite { cite, .. } => {
                        write_cite_directive(&mut out, cite);
                        out.push_str(&render_quote_body(&cite.quote));
                        out.push_str("\n\n");
                    }
                    ContentBlock::Slide { slide, .. } => {
                        write_slide_directive(&mut out, slide);
                        out.push('\n');
                    }
                    ContentBlock::Attachment {
                        filename,
                        media_type,
                        caption,
                        sha256,
                        ..
                    } => {
                        write_attachment_directive(
                            &mut out,
                            &AttachmentPayload {
                                filename: filename.clone(),
                                media_type: media_type.clone(),
                                caption: caption.clone(),
                                sha256: sha256.clone(),
                                // Bytes are not projected in Tessprek.
                                data: Vec::new(),
                            },
                        );
                        out.push('\n');
                    }
                    ContentBlock::Text { .. } => unreachable!("text handled above"),
                }
            }
        }
    }
    out
}

fn write_brace_line(out: &mut String, prefix: &str, parts: &[String]) {
    let _ = writeln!(out, "{prefix}{}{BRACE_SUFFIX}", parts.join(" "));
}

/// Prefer multiline brace blocks (same shape as `\tessera{…}`) for readability.
fn write_brace_block(out: &mut String, prefix: &str, parts: &[String]) {
    if parts.is_empty() {
        let _ = writeln!(out, "{prefix}{BRACE_SUFFIX}");
        return;
    }
    let _ = writeln!(out, "{prefix}");
    for part in parts {
        let _ = writeln!(out, "  {part}");
    }
    let _ = writeln!(out, "{BRACE_SUFFIX}");
}

fn write_header(out: &mut String, meta: &TessprekDocMeta) {
    let mut parts = vec![format!("format={FORMAT}"), format!("version={VERSION}")];
    meta.push_parts(&mut parts);
    // Multiline so long identity keys stay readable (single-line still accepted).
    let _ = writeln!(out, "{TESSERA_PREFIX}");
    for part in &parts {
        let _ = writeln!(out, "  {part}");
    }
    let _ = writeln!(out, "{BRACE_SUFFIX}");
}

fn write_ids(out: &mut String, blocks: &[ContentBlock]) {
    let ids = blocks
        .iter()
        .map(|b| b.chunk_id().unwrap_or(0).to_string())
        .collect::<Vec<_>>()
        .join(",");
    write_brace_line(out, IDS_PREFIX, &[ids]);
}

/// Emit `\media{…}` for image payloads referenced by figures (sorted by id).
///
/// One attr per line; blank line between payloads when there are several.
/// Omitted when the document has no figures. Prefer rows from `media`; fill
/// missing figure targets as bare `id=N`. Regenerated on every encode.
fn write_media(out: &mut String, blocks: &[ContentBlock], media: &[TessprekMediaEntry]) {
    let mut by_id: BTreeMap<u64, TessprekMediaEntry> = BTreeMap::new();
    for entry in media {
        if entry.chunk_id != 0 {
            by_id.insert(entry.chunk_id, entry.clone());
        }
    }
    for block in blocks {
        if let ContentBlock::Figure { figure, .. } = block {
            let id = figure.image_chunk_id;
            if id != 0 {
                by_id.entry(id).or_insert(TessprekMediaEntry {
                    chunk_id: id,
                    ..TessprekMediaEntry::default()
                });
            }
        }
    }
    if by_id.is_empty() {
        return;
    }
    let _ = writeln!(out, "{MEDIA_PREFIX}");
    let mut first = true;
    for entry in by_id.values() {
        if !first {
            out.push('\n');
        }
        first = false;
        for part in entry.attr_parts() {
            let _ = writeln!(out, "  {part}");
        }
    }
    let _ = writeln!(out, "{BRACE_SUFFIX}");
}

fn render_text_body(
    header: &TextHeader,
    body: &str,
    pending_links: &[crate::catalog::OutboundLink],
    links: &[crate::catalog::LinkEntry],
    ordered_index: Option<u32>,
) -> String {
    use crate::catalog::{InlineKind, InlineSpan, LinkKind};

    if pending_links.is_empty() {
        return header.render_markdown_with_links_indexed(body, links, ordered_index);
    }

    let mut header = header.clone();
    let mut synthetic_links = Vec::new();
    for link in pending_links {
        let link_id = u64::try_from(synthetic_links.len()).unwrap_or(u64::MAX);
        if let Ok(entry) = link.clone().into_entry(0, LinkKind::Wiki) {
            synthetic_links.push(entry);
            header.spans.push(InlineSpan {
                start: link.start,
                end: link.end,
                kind: InlineKind::Link { link_id },
            });
        }
    }
    header.render_markdown_with_links_indexed(body, &synthetic_links, ordered_index)
}

/// Write `\text{title=… caption=… class=… …}` when the header carries attrs
/// that cannot live in plain Markdown. Emits nothing otherwise.
fn write_text_directive(out: &mut String, header: &TextHeader) {
    if header.classes.is_empty()
        && header.lang.is_none()
        && header.align.is_none()
        && header.title.is_none()
        && header.caption.is_none()
    {
        return;
    }
    let mut parts = Vec::new();
    if let Some(title) = header.title.as_deref() {
        parts.push(format!("title=\"{}\"", escape_attr(title)));
    }
    if let Some(caption) = header.caption.as_deref() {
        parts.push(format!("caption=\"{}\"", escape_attr(caption)));
    }
    if !header.classes.is_empty() {
        parts.push(format!("class=\"{}\"", header.classes.join(" ")));
    }
    if let Some(lang) = header.lang.as_deref() {
        parts.push(format!("lang={}", attr_token(lang)));
    }
    if let Some(align) = header.align {
        parts.push(format!("align={}", align.as_str()));
    }
    write_brace_block(out, TEXT_PREFIX, &parts);
}

fn write_figure_directive(out: &mut String, figure: &FigureRef) {
    let mut parts = vec![
        format!("image={}", figure.image_chunk_id),
        format!("placement={}", figure.placement.as_str()),
        format!("alt=\"{}\"", escape_attr(&figure.alt_text)),
    ];
    if let ImagePlacement::Region { name } = &figure.placement {
        parts.push(format!("region=\"{}\"", escape_attr(name)));
    }
    if let Some(title) = figure.title.as_deref() {
        parts.push(format!("title=\"{}\"", escape_attr(title)));
    }
    if let Some(caption) = figure.caption.as_deref() {
        parts.push(format!("caption=\"{}\"", escape_attr(caption)));
    }
    write_brace_block(out, FIGURE_PREFIX, &parts);
}

fn write_cite_directive(out: &mut String, cite: &CitePayload) {
    let mut parts = Vec::new();
    if let Some(label) = cite.label.as_deref() {
        parts.push(format!("label={}", attr_token(label)));
    }
    if let Some(doc) = cite.target_doc_id.as_deref() {
        parts.push(format!("target_doc={doc}"));
    }
    if let Some(chunk) = cite.target_chunk_id {
        parts.push(format!("target_chunk={chunk}"));
    }
    if let Some(page) = cite.page {
        parts.push(format!("page={page}"));
    }
    write_brace_block(out, CITE_PREFIX, &parts);
}

fn write_slide_directive(out: &mut String, slide: &SlidePayload) {
    let regions = slide
        .regions
        .iter()
        .map(|r| format!("{}:{}", r.name, r.chunk_id))
        .collect::<Vec<_>>()
        .join(",");
    write_brace_block(
        out,
        SLIDE_PREFIX,
        &[
            format!("layout={}", attr_token(&slide.layout_id)),
            format!("regions=\"{}\"", escape_attr(&regions)),
        ],
    );
}

fn write_attachment_directive(out: &mut String, att: &AttachmentPayload) {
    let mut parts = vec![
        format!("filename=\"{}\"", escape_attr(&att.filename)),
        format!("media_type={}", attr_token(&att.media_type)),
        format!("sha256={}", att.sha256),
    ];
    if let Some(caption) = att.caption.as_deref() {
        parts.push(format!("caption=\"{}\"", escape_attr(caption)));
    }
    write_brace_block(out, ATTACH_PREFIX, &parts);
}

fn parse_slide_regions(raw: &str, line_no: usize) -> Result<Vec<SlideRegion>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(parse_err(line_no, 1, "slide regions= must be non-empty"));
    }
    let mut regions = Vec::new();
    for part in raw.split(',') {
        let part = part.trim();
        let Some((name, id)) = part.split_once(':') else {
            return Err(parse_err(
                line_no,
                1,
                format!("bad region '{part}' (expected name:chunk_id)"),
            ));
        };
        let chunk_id = id
            .trim()
            .parse::<u64>()
            .map_err(|_| parse_err(line_no, 1, format!("invalid region chunk id in '{part}'")))?;
        regions.push(SlideRegion {
            name: name.trim().to_owned(),
            chunk_id,
        });
    }
    Ok(regions)
}

/// Render a `CitePayload.quote` as a Markdown-blockquote-styled body.
fn render_quote_body(quote: &str) -> String {
    let trimmed = quote.trim_end();
    if trimmed.is_empty() {
        return String::from(">");
    }
    trimmed
        .lines()
        .map(|l| {
            if l.is_empty() {
                ">".to_owned()
            } else {
                format!("> {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Strip the `> ` blockquote styling from a `\cite{}` body.
fn strip_quote_body(body: &str) -> String {
    body.lines()
        .map(|line| {
            line.strip_prefix("> ")
                .or_else(|| line.strip_prefix('>'))
                .unwrap_or(line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_figure_markdown(body: &str, line_no: usize) -> Result<(String, Option<u64>)> {
    let body = body.trim();
    // ![alt](media:N) — also accept legacy `media:chunk-N`
    let Some(rest) = body.strip_prefix("![") else {
        return Err(parse_err(line_no, 1, "figure body must be ![alt](media:N)"));
    };
    let Some((alt, after_alt)) = rest.split_once("](") else {
        return Err(parse_err(line_no, 1, "figure markdown missing ']('"));
    };
    let Some(url) = after_alt.strip_suffix(')') else {
        return Err(parse_err(line_no, 1, "figure markdown missing closing ')'"));
    };
    let id_str = url
        .strip_prefix("media:")
        .map(|s| s.strip_prefix("chunk-").unwrap_or(s));
    let image_id = id_str.and_then(|s| s.parse::<u64>().ok());
    Ok((unescape_alt(alt), image_id))
}

pub(crate) fn parse_attrs(attrs: &str, line_no: usize) -> Result<BTreeMap<String, String>> {
    let mut map = BTreeMap::new();
    let mut rest = attrs.trim();
    while !rest.is_empty() {
        let eq = rest.find('=').ok_or_else(|| {
            parse_err(
                line_no,
                1,
                format!("malformed attribute near '{rest}' (expected key=value)"),
            )
        })?;
        let key = rest[..eq].trim();
        if key.is_empty() {
            return Err(parse_err(line_no, 1, "empty attribute key"));
        }
        rest = rest[eq + 1..].trim_start();
        let (value, next) = if let Some(quoted) = rest.strip_prefix('"') {
            let end = quoted
                .find('"')
                .ok_or_else(|| parse_err(line_no, 1, "unterminated quoted attribute"))?;
            let value = quoted[..end].to_owned();
            (value, quoted[end + 1..].trim_start())
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            (rest[..end].to_owned(), rest[end..].trim_start())
        };
        map.insert(key.to_owned(), value);
        rest = next;
    }
    Ok(map)
}

fn parse_placement(raw: &str, region: Option<&str>, line_no: usize) -> Result<ImagePlacement> {
    match raw {
        "flow" => Ok(ImagePlacement::Flow),
        "full_width" => Ok(ImagePlacement::FullWidth),
        "float_start" => Ok(ImagePlacement::FloatStart),
        "float_end" => Ok(ImagePlacement::FloatEnd),
        "inline" => Ok(ImagePlacement::Inline),
        "background" => Ok(ImagePlacement::Background),
        "region" => Ok(ImagePlacement::Region {
            name: region.unwrap_or("default").to_owned(),
        }),
        other => Err(parse_err(
            line_no,
            1,
            format!("unknown placement '{other}'"),
        )),
    }
}

fn required_u64(map: &BTreeMap<String, String>, key: &str, line_no: usize) -> Result<u64> {
    let raw = map
        .get(key)
        .ok_or_else(|| parse_err(line_no, 1, format!("missing required attribute '{key}'")))?;
    raw.parse::<u64>()
        .map_err(|_| parse_err(line_no, 1, format!("invalid {key} value '{raw}'")))
}

fn optional_u64(map: &BTreeMap<String, String>, key: &str) -> Option<u64> {
    map.get(key)?.parse().ok()
}

fn optional_u32(map: &BTreeMap<String, String>, key: &str) -> Option<u32> {
    map.get(key)?.parse().ok()
}

pub(super) fn trim_block_body(lines: &[&str]) -> String {
    let mut start = 0;
    let mut end = lines.len();
    while start < end && lines[start].trim().is_empty() {
        start += 1;
    }
    while end > start && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    lines[start..end].join("\n")
}

fn escape_attr(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn attr_token(s: &str) -> String {
    if s.chars().any(|c| c.is_whitespace() || c == '"' || c == '=') {
        format!("\"{}\"", escape_attr(s))
    } else {
        s.to_owned()
    }
}

fn unescape_alt(s: &str) -> String {
    s.replace("\\[", "[")
        .replace("\\]", "]")
        .replace("\\\\", "\\")
}

fn parse_err(line: usize, column: usize, message: impl Into<String>) -> TesError {
    TesError::EditParse {
        line,
        column,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::chunk::TextRole;
    use crate::catalog::{DocumentCatalog, TesWriterSession};
    use crate::layout::DocKind;
    use tempfile::tempdir;

    #[test]
    fn round_trip_text_classes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.tes");
        let mut session = TesWriterSession::create(&path, DocKind::Note);
        session
            .set_catalog(DocumentCatalog::new(
                "550e8400-e29b-41d4-a716-446655440000",
                "Demo",
                "2026-07-27T00:00:00Z",
                "2026-07-27T00:00:00Z",
                DocKind::Note,
            ))
            .unwrap();
        let mut header = TextHeader::heading(1);
        header.classes = vec!["lead".into()];
        session.add_text_chunk(&header, "Hello").unwrap();
        session
            .add_text_chunk(&TextHeader::paragraph(), "Body")
            .unwrap();
        session.commit().unwrap();

        let file = TesFile::open(&path).unwrap();
        let text = encode_tessprek(&file, "abc").unwrap();
        assert!(text.contains("format=tessprek"), "{text}");
        assert!(text.contains("version=2"), "{text}");
        assert!(text.contains("source-hash=abc"), "{text}");
        assert!(
            text.contains("doc_id=550e8400-e29b-41d4-a716-446655440000"),
            "{text}"
        );
        assert!(
            text.contains("title=Demo") || text.contains("title=\"Demo\""),
            "{text}"
        );
        assert!(text.contains("class=\"lead\""), "{text}");
        assert!(text.contains("\\text{"), "{text}");
        assert!(text.contains("# Hello"), "{text}");
        let blocks = decode_tessprek(&text).unwrap();
        assert_eq!(blocks.len(), 2);
        match &blocks[0] {
            ContentBlock::Text { header, body, .. } => {
                assert_eq!(header.role, TextRole::Heading);
                assert_eq!(header.classes, vec!["lead"]);
                assert_eq!(body, "Hello");
            }
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn round_trip_figure_cite_slide_attachment() {
        let blocks = vec![
            ContentBlock::Text {
                chunk_id: Some(1),
                header: TextHeader::heading(1),
                body: "Doc".into(),
                pending_links: Vec::new(),
            },
            ContentBlock::Figure {
                chunk_id: Some(2),
                figure: FigureRef {
                    image_chunk_id: 3,
                    alt_text: "A photo".into(),
                    title: None,
                    caption: Some("Cap".into()),
                    placement: ImagePlacement::Flow,
                },
            },
            ContentBlock::Cite {
                chunk_id: Some(4),
                cite: CitePayload {
                    quote: "Some quoted text".into(),
                    target_doc_id: None,
                    target_chunk_id: Some(1),
                    target_byte_start: None,
                    target_byte_end: None,
                    label: Some("Smith2024".into()),
                    page: None,
                    source: None,
                },
            },
            ContentBlock::Slide {
                chunk_id: Some(5),
                slide: SlidePayload {
                    layout_id: "title".into(),
                    regions: vec![SlideRegion {
                        name: "body".into(),
                        chunk_id: 1,
                    }],
                },
            },
            ContentBlock::Attachment {
                chunk_id: Some(6),
                filename: "notes.pdf".into(),
                media_type: "application/pdf".into(),
                caption: Some("Handout".into()),
                sha256: "deadbeef".into(),
            },
        ];
        let text = encode_content_blocks(&TessprekDocMeta::default(), &blocks, &[], &[]);
        assert!(text.contains("\\ids{1,2,4,5,6}"), "{text}");
        assert!(text.contains("id=3"), "{text}");
        assert!(text.contains("\\media{\n"), "{text}");
        assert!(text.contains("\\figure{"), "{text}");
        assert!(text.contains("\\cite{"), "{text}");
        assert!(text.contains("> Some quoted text"), "{text}");
        assert!(text.contains("\\slide{"), "{text}");
        assert!(text.contains("\\attach{"), "{text}");
        let decoded = decode_tessprek(&text).unwrap();
        assert_eq!(decoded, blocks);
    }

    #[test]
    fn decode_skips_rich_media_header() {
        let text = "\
\\tessera{format=tessprek version=2}\n\
\\ids{1,2}\n\
\\media{\n\
  id=9\n\
  media_type=image/png\n\
  sha256=deadbeef\n\
  width=1\n\
  height=1\n\
}\n\
\n\
# Title\n\
\n\
\\figure{\n\
  image=9\n\
  placement=flow\n\
  alt=\"alt\"\n\
}\n\
";
        let blocks = decode_tessprek(text).unwrap();
        assert_eq!(blocks.len(), 2);
        match &blocks[1] {
            ContentBlock::Figure { figure, .. } => {
                assert_eq!(figure.image_chunk_id, 9);
                assert_eq!(figure.alt_text, "alt");
            }
            other => panic!("expected figure, got {other:?}"),
        }
    }

    #[test]
    fn encode_media_blank_line_between_payloads() {
        let blocks = vec![
            ContentBlock::Figure {
                chunk_id: Some(1),
                figure: FigureRef {
                    image_chunk_id: 2,
                    alt_text: "a".into(),
                    title: None,
                    caption: None,
                    placement: ImagePlacement::Flow,
                },
            },
            ContentBlock::Figure {
                chunk_id: Some(3),
                figure: FigureRef {
                    image_chunk_id: 4,
                    alt_text: "b".into(),
                    title: None,
                    caption: None,
                    placement: ImagePlacement::Flow,
                },
            },
        ];
        let media = vec![
            TessprekMediaEntry {
                chunk_id: 2,
                media_type: Some("image/png".into()),
                sha256: Some("aa".into()),
                width_px: Some(1),
                height_px: Some(1),
            },
            TessprekMediaEntry {
                chunk_id: 4,
                media_type: Some("image/jpeg".into()),
                sha256: Some("bb".into()),
                width_px: Some(2),
                height_px: Some(2),
            },
        ];
        let text = encode_content_blocks(&TessprekDocMeta::default(), &blocks, &[], &media);
        assert!(
            text.contains(
                "\\media{\n  id=2\n  media_type=image/png\n  sha256=aa\n  width=1\n  height=1\n\n  id=4\n"
            ),
            "{text}"
        );
    }

    #[test]
    fn encode_projects_catalog_meta_into_header() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("meta.tes");
        let mut session = TesWriterSession::create(&path, DocKind::Note);
        let mut catalog = DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440099",
            "Text roles tour",
            "2026-07-27T00:00:00Z",
            "2026-07-27T00:00:00Z",
            DocKind::Note,
        );
        catalog.language = Some("en".into());
        catalog.cite_style_id = Some("numeric".into());
        catalog.theme_id = Some("default".into());
        catalog.template_id = Some("article".into());
        catalog.slug = Some("text-roles".into());
        session.set_catalog(catalog).unwrap();
        session
            .add_text_chunk(&TextHeader::paragraph(), "Hi")
            .unwrap();
        session.commit().unwrap();

        let file = TesFile::open(&path).unwrap();
        let text = encode_tessprek(&file, "deadbeef").unwrap();
        assert!(text.contains("\\tessera{\n"), "{text}");
        assert!(text.contains("  format=tessprek\n"), "{text}");
        assert!(text.contains("  version=2\n"), "{text}");
        assert!(text.contains("  source-hash=deadbeef\n"), "{text}");
        assert!(
            text.contains("  doc_id=550e8400-e29b-41d4-a716-446655440099\n"),
            "{text}"
        );
        assert!(text.contains("  doc_kind=note\n"), "{text}");
        assert!(text.contains("  title=\"Text roles tour\"\n"), "{text}");
        assert!(text.contains("  language=en\n"), "{text}");
        assert!(text.contains("  cite_style_id=numeric\n"), "{text}");
        assert!(text.contains("  theme_id=default\n"), "{text}");
        assert!(text.contains("  template_id=article\n"), "{text}");
        assert!(text.contains("  slug=text-roles\n"), "{text}");
        assert!(text.contains("}\n\\ids{"), "{text}");
        assert_eq!(decode_tessprek(&text).unwrap().len(), 1);
    }

    #[test]
    fn decode_accepts_multiline_and_single_line_header() {
        let multi = "\
\\tessera{\n\
  format=tessprek\n\
  version=2\n\
  title=\"Hello\"\n\
}\n\
\\ids{1}\n\
\n\
# Hello\n\
";
        assert_eq!(decode_tessprek(multi).unwrap().len(), 1);
        let single = "\
\\tessera{format=tessprek version=2 title=\"Hello\"}\n\
\\ids{1}\n\
\n\
# Hello\n\
";
        assert_eq!(decode_tessprek(single).unwrap().len(), 1);
    }

    #[test]
    fn unknown_header_keys_are_listed() {
        let mut map = BTreeMap::new();
        map.insert("format".into(), "tessprek".into());
        map.insert("bogus".into(), "x".into());
        map.insert("tags".into(), "a,b".into());
        let unknown = TessprekDocMeta::unknown_keys(&map);
        assert_eq!(unknown, vec!["bogus".to_string(), "tags".to_string()]);
    }

    #[test]
    fn decode_rejects_missing_header() {
        let err = decode_tessprek("# Title\n").unwrap_err();
        assert!(matches!(err, TesError::EditParse { .. }));
    }

    #[test]
    fn decode_rejects_id_count_mismatch() {
        let text = "\\tessera{format=tessprek version=2}\n\\ids{1,2}\n\n# Title\n";
        let err = decode_tessprek(text).unwrap_err();
        match err {
            TesError::EditParse { message, .. } => {
                assert!(message.contains("id(s)"), "{message}");
                assert!(
                    message.contains("TesseraFormat") || message.contains("tes format"),
                    "{message}"
                );
            }
            other => panic!("expected EditParse, got {other:?}"),
        }
    }

    #[test]
    fn decode_rejects_v1_version() {
        let text = "\\tessera{format=tessprek version=1}\n\\ids{}\n";
        let err = decode_tessprek(text).unwrap_err();
        match err {
            TesError::EditParse { message, .. } => {
                assert!(message.contains("v1"), "{message}");
            }
            other => panic!("expected EditParse, got {other:?}"),
        }
    }
}
