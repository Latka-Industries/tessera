//! Tessprek v2 wire markers: `\tessera{}` header + `\ids{}` reading order,
//! LaTeX-lite brace commands for structured chunks. Shared by encode/decode
//! and LSP hover. No per-block `\id{N}`, no HTML comments, no YAML front
//! matter, no dual-read of the v1 HTML-comment format.

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
/// Optional preserved-attrs directive before a Markdown block (table/math/code).
pub const BLOCK_PREFIX: &str = "\\block{";
/// Legacy alias for [`BLOCK_PREFIX`] — still accepted on read; encode emits `\block{}`.
pub const TEXT_PREFIX: &str = "\\text{";
/// Figure directive: `\figure{image=… placement=… alt=…}` (no Markdown body).
pub const FIGURE_PREFIX: &str = "\\figure{";
/// Cite directive: bibliography stub `\cite{label=… [author=…] …}` (attrs only).
pub const CITE_PREFIX: &str = "\\cite{";
/// Quote directive: passage from a doc/chunk `\quote{target_… quote="…"}`.
pub const QUOTE_PREFIX: &str = "\\quote{";
/// Ref directive: pointer to a doc/chunk `\ref{target_…}` (no excerpt).
pub const REF_PREFIX: &str = "\\ref{";
/// Slide directive: `\slide{layout=… regions=…}`.
pub const SLIDE_PREFIX: &str = "\\slide{";
/// Layout directive: `\layout{ place … / vspace=… / rule … }` (D24).
pub const LAYOUT_PREFIX: &str = "\\layout{";
/// In-document TOC: `\toc{depth=… title="…"}` (THI-390). Bare `\toc` also accepted.
pub const TOC_PREFIX: &str = "\\toc{";
/// Meta-row directive opener: `\row{left}{right}…` (2+ content braces).
pub const ROW_PREFIX: &str = "\\row{";
/// Attachment directive: `\attach{filename=… media_type=… sha256=…}`.
pub const ATTACH_PREFIX: &str = "\\attach{";
/// Closing delimiter for every brace command.
pub const BRACE_SUFFIX: &str = "}";

/// Preferred attribute keys for `\block{…}` (completion + hover order).
pub const BLOCK_ATTR_KEYS: &[&str] = &[
    "title",
    "caption",
    "class",
    "lang",
    "align",
    "indent",
    "code_lang",
];
/// Deprecated alias for [`BLOCK_ATTR_KEYS`].
pub const TEXT_ATTR_KEYS: &[&str] = BLOCK_ATTR_KEYS;
/// Preferred attribute keys for `\figure{…}`.
pub const FIGURE_ATTR_KEYS: &[&str] = &["image", "placement", "alt", "region", "title", "caption"];
/// Preferred attribute keys for bibliography `\cite{…}`.
pub const CITE_ATTR_KEYS: &[&str] = &[
    "label",
    "key",
    "entry_type",
    "author",
    "title",
    "journal",
    "year",
    "volume",
    "number",
    "pages",
    "doi",
    "publisher",
    "note",
    "howpublished",
    "url",
    "page",
];
/// Preferred attribute keys for `\quote{…}` / `\ref{…}`.
pub const QUOTE_ATTR_KEYS: &[&str] = &[
    "label",
    "target_doc",
    "target_chunk",
    "target_byte_start",
    "target_byte_end",
    "page",
    "quote",
];
/// Preferred attribute keys for `\slide{…}`.
pub const SLIDE_ATTR_KEYS: &[&str] = &["layout", "regions"];
/// Preferred attribute keys / op hints for `\layout{…}` (D24).
pub const LAYOUT_ATTR_KEYS: &[&str] = &["place", "vspace", "rule", "frac", "em", "content"];
/// Preferred attribute keys for `\toc{…}` (THI-390).
pub const TOC_ATTR_KEYS: &[&str] = &["depth", "title", "page_numbers", "section_numbers"];
/// Preferred attribute keys for `\attach{…}`.
pub const ATTACH_ATTR_KEYS: &[&str] = &["chunk", "filename", "media_type", "sha256", "caption"];
/// Preferred attribute keys for `\media{…}` header rows.
pub const MEDIA_ATTR_KEYS: &[&str] = &["id", "media_type", "sha256", "width", "height"];

/// Attribute keys for a Tessprek command kind (`block`, `cite`, `attach`, …).
#[must_use]
pub fn command_attr_keys(kind: &str) -> Option<&'static [&'static str]> {
    Some(match kind {
        "tessera" => TESSERA_HEADER_KEYS,
        "block" | "text" => BLOCK_ATTR_KEYS,
        "figure" => FIGURE_ATTR_KEYS,
        "cite" => CITE_ATTR_KEYS,
        "quote" | "ref" => QUOTE_ATTR_KEYS,
        "slide" => SLIDE_ATTR_KEYS,
        "layout" => LAYOUT_ATTR_KEYS,
        "toc" => TOC_ATTR_KEYS,
        "attach" | "attachment" => ATTACH_ATTR_KEYS,
        "media" => MEDIA_ATTR_KEYS,
        _ => return None,
    })
}

/// Header-only brace lines (`\tessera` / `\ids` / `\media`).
pub const HEADER_COMMANDS: &[(&str, &str)] = &[
    (TESSERA_PREFIX, "tessera"),
    (IDS_PREFIX, "ids"),
    (MEDIA_PREFIX, "media"),
];

/// Body brace lines (structured chunks + optional `\block`).
/// Kind `attachment` matches [`super::decode_named_directive`].
pub const BODY_COMMANDS: &[(&str, &str)] = &[
    (BLOCK_PREFIX, "block"),
    (FIGURE_PREFIX, "figure"),
    (CITE_PREFIX, "cite"),
    (QUOTE_PREFIX, "quote"),
    (REF_PREFIX, "ref"),
    (SLIDE_PREFIX, "slide"),
    (LAYOUT_PREFIX, "layout"),
    (TOC_PREFIX, "toc"),
    (ATTACH_PREFIX, "attachment"),
];

/// Legacy body openers — parsed as the same kinds as [`BODY_COMMANDS`].
/// Not offered in completions; encode never emits these.
pub const LEGACY_BODY_OPENERS: &[(&str, &str)] = &[(TEXT_PREFIX, "block")];

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
pub fn parse_brace_command(trimmed: &str, include_header: bool) -> Option<(&'static str, &str)> {
    if include_header && let Some(hit) = match_brace_closed(trimmed, HEADER_COMMANDS) {
        return Some(hit);
    }
    match_brace_closed(trimmed, BODY_COMMANDS)
        .or_else(|| match_brace_closed(trimmed, LEGACY_BODY_OPENERS))
}

/// Match a body command opener (`\block{`, `\figure{`, …) even when `}` is
/// on a later line. Returns `(kind, prefix)`. Legacy `\text{` maps to `block`.
#[must_use]
pub fn match_body_opener(trimmed: &str) -> Option<(&'static str, &'static str)> {
    for &(prefix, kind) in BODY_COMMANDS.iter().chain(LEGACY_BODY_OPENERS.iter()) {
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
