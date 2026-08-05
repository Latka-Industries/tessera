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
/// Optional preserved-attrs directive before a Markdown block.
pub const TEXT_PREFIX: &str = "\\text{";
/// Figure directive: `\figure{image=… placement=… alt=…}` (no Markdown body).
pub const FIGURE_PREFIX: &str = "\\figure{";
/// Cite directive: `\cite{label=… target_chunk=… [target_byte_start=…] …}` + quote body.
pub const CITE_PREFIX: &str = "\\cite{";
/// Slide directive: `\slide{layout=… regions=…}`.
pub const SLIDE_PREFIX: &str = "\\slide{";
/// Attachment directive: `\attach{filename=… media_type=… sha256=…}`.
pub const ATTACH_PREFIX: &str = "\\attach{";
/// Closing delimiter for every brace command.
pub const BRACE_SUFFIX: &str = "}";

/// Preferred attribute keys for `\text{…}` (completion + hover order).
pub const TEXT_ATTR_KEYS: &[&str] = &["title", "caption", "class", "lang", "align", "code_lang"];
/// Preferred attribute keys for `\figure{…}`.
pub const FIGURE_ATTR_KEYS: &[&str] = &["image", "placement", "alt", "region", "title", "caption"];
/// Preferred attribute keys for `\cite{…}` (includes ranged target spans).
pub const CITE_ATTR_KEYS: &[&str] = &[
    "label",
    "key",
    "target_doc",
    "target_chunk",
    "target_byte_start",
    "target_byte_end",
    "page",
];
/// Preferred attribute keys for `\slide{…}`.
pub const SLIDE_ATTR_KEYS: &[&str] = &["layout", "regions"];
/// Preferred attribute keys for `\attach{…}`.
pub const ATTACH_ATTR_KEYS: &[&str] = &["filename", "media_type", "sha256", "caption"];
/// Preferred attribute keys for `\media{…}` header rows.
pub const MEDIA_ATTR_KEYS: &[&str] = &["id", "media_type", "sha256", "width", "height"];

/// Attribute keys for a Tessprek command kind (`text`, `cite`, `attach`, …).
#[must_use]
pub fn command_attr_keys(kind: &str) -> Option<&'static [&'static str]> {
    Some(match kind {
        "tessera" => TESSERA_HEADER_KEYS,
        "text" => TEXT_ATTR_KEYS,
        "figure" => FIGURE_ATTR_KEYS,
        "cite" => CITE_ATTR_KEYS,
        "slide" => SLIDE_ATTR_KEYS,
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
pub fn parse_brace_command(trimmed: &str, include_header: bool) -> Option<(&'static str, &str)> {
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
