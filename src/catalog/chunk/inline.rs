//! Inline formatting kinds, spans, and Markdown span projection.

use serde::{Deserialize, Serialize};

use crate::catalog::LinkEntry;
use crate::error::{Result, TesError};

/// Semantic horizontal alignment (never physical left/right).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    /// Start edge in writing direction.
    Start,
    /// Centered.
    Center,
    /// End edge in writing direction.
    End,
    /// Justified.
    Justify,
}

impl TextAlign {
    /// Lowercase wire / Tessprek name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Center => "center",
            Self::End => "end",
            Self::Justify => "justify",
        }
    }

    /// Parse a lowercase align name.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::InvalidTextHeader`] for unknown names.
    pub fn from_name(name: &str) -> Result<Self> {
        Ok(match name {
            "start" => Self::Start,
            "center" => Self::Center,
            "end" => Self::End,
            "justify" => Self::Justify,
            other => {
                return Err(TesError::InvalidTextHeader {
                    message: format!("unknown text align '{other}'"),
                });
            }
        })
    }
}

/// Closed inline formatting vocabulary (`docs/structure_v1.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InlineKind {
    /// Italic / emphasis.
    Emphasis,
    /// Bold / strong.
    Strong,
    /// Underline (projected as `<u>…</u>` in Tessprek / HTML).
    Underline,
    /// Inline code.
    Code,
    /// Defined term.
    Term,
    /// Inline quotation.
    Quote,
    /// Inline math (LaTeX).
    Math {
        /// LaTeX source.
        tex: String,
    },
    /// Reference into the link table / link records.
    Link {
        /// Link record id.
        link_id: u64,
    },
    /// Reference to a cite chunk.
    Citation {
        /// Cite chunk id.
        cite_chunk_id: u64,
    },
    /// Pack-pinned font (`\font{font_id}{…}` → weave `TextRun.face` / `pinned_faces`).
    Font {
        /// Pack font id (ASCII identifier).
        font_id: String,
    },
}

/// Half-open UTF-8 byte range with a typed kind over a text body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineSpan {
    /// Inclusive start byte offset into the body.
    pub start: u32,
    /// Exclusive end byte offset into the body.
    pub end: u32,
    /// Formatting / reference kind.
    pub kind: InlineKind,
}

/// Reject empty, inverted, out-of-bounds, non-char-boundary, or overlapping spans.
pub(super) fn validate_spans(body: &str, spans: &[InlineSpan]) -> Result<()> {
    let body_len = u32::try_from(body.len()).unwrap_or(u32::MAX);
    for span in spans {
        if span.start >= span.end {
            return Err(TesError::InvalidTextHeader {
                message: format!("empty or inverted span {}..{}", span.start, span.end),
            });
        }
        if span.end > body_len {
            return Err(TesError::InvalidTextHeader {
                message: format!(
                    "span {}..{} out of bounds for body length {body_len}",
                    span.start, span.end
                ),
            });
        }
        if !body.is_char_boundary(span.start as usize) || !body.is_char_boundary(span.end as usize)
        {
            return Err(TesError::InvalidTextHeader {
                message: format!(
                    "span {}..{} is not on a UTF-8 character boundary",
                    span.start, span.end
                ),
            });
        }
        if let InlineKind::Math { tex } = &span.kind
            && tex.is_empty()
        {
            return Err(TesError::InvalidTextHeader {
                message: "inline math tex must be non-empty".into(),
            });
        }
        if let InlineKind::Font { font_id } = &span.kind
            && !is_font_id(font_id)
        {
            return Err(TesError::InvalidTextHeader {
                message: format!("invalid font id {font_id:?}"),
            });
        }
    }
    let mut ordered: Vec<&InlineSpan> = spans.iter().collect();
    ordered.sort_by_key(|s| (s.start, s.end));
    let mut stack: Vec<&InlineSpan> = Vec::new();
    for span in ordered {
        while stack.last().is_some_and(|outer| outer.end <= span.start) {
            stack.pop();
        }
        if let Some(outer) = stack.last()
            && span.end > outer.end
        {
            return Err(TesError::InvalidTextHeader {
                message: format!(
                    "span {}..{} partially overlaps {}..{}",
                    span.start, span.end, outer.start, outer.end
                ),
            });
        }
        stack.push(span);
    }
    Ok(())
}

/// Apply spanned formatting as Markdown (links resolved via the document link table).
pub(super) fn apply_spans_markdown(
    body: &str,
    spans: &[InlineSpan],
    links: &[LinkEntry],
) -> String {
    if spans.is_empty() {
        return body.to_owned();
    }

    // Coextensive spans share one replacement (e.g. Font+Link on the same glyph)
    // so we never re-wrap with stale offsets into a longer `\font{…}{…}` string.
    let mut groups: std::collections::BTreeMap<(u32, u32), Vec<&InlineSpan>> =
        std::collections::BTreeMap::new();
    for span in spans {
        if span.end > span.start {
            groups.entry((span.start, span.end)).or_default().push(span);
        }
    }

    let mut keys: Vec<(u32, u32)> = groups.keys().copied().collect();
    // Right-to-left: earlier byte offsets stay valid for disjoint ranges.
    keys.sort_by_key(|&(start, end)| std::cmp::Reverse((start, end)));

    let mut out = body.to_owned();
    for (start_u, end_u) in keys {
        let start = start_u as usize;
        let end = end_u as usize;
        if end > out.len()
            || start >= end
            || !out.is_char_boundary(start)
            || !out.is_char_boundary(end)
        {
            continue;
        }
        let group = &groups[&(start_u, end_u)];
        let mut inner = out[start..end].to_owned();
        // Font → style → Link (link outermost).
        let order = |s: &&InlineSpan| -> u8 {
            match s.kind {
                InlineKind::Font { .. } => 0,
                InlineKind::Link { .. } => 2,
                _ => 1,
            }
        };
        let mut ordered = group.clone();
        ordered.sort_by_key(order);
        for span in ordered {
            inner = wrap_markdown_span(inner, span, links);
        }
        out.replace_range(start..end, &inner);
    }
    out
}

fn wrap_markdown_span(inner: String, span: &InlineSpan, links: &[LinkEntry]) -> String {
    match &span.kind {
        InlineKind::Emphasis | InlineKind::Term => format!("*{inner}*"),
        InlineKind::Strong => format!("**{inner}**"),
        InlineKind::Underline => format!("<u>{inner}</u>"),
        InlineKind::Code => format!("`{inner}`"),
        InlineKind::Quote => format!("\u{201c}{inner}\u{201d}"),
        InlineKind::Math { tex } => format!("${tex}$"),
        InlineKind::Link { link_id } => match links.get(*link_id as usize) {
            Some(entry) => {
                let dest = entry.target.markdown_destination();
                format!("[{inner}]({dest})")
            }
            None => inner,
        },
        InlineKind::Citation { cite_chunk_id } => {
            let _ = cite_chunk_id;
            inner
        }
        InlineKind::Font { font_id } => format!("\\font{{{font_id}}}{{{inner}}}"),
    }
}

/// ASCII identifier: `[A-Za-z_][A-Za-z0-9_]*` (aliases, phrases, font ids).
#[must_use]
pub fn is_ascii_ident(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// ASCII identifier for pack font ids (`armenian`, `test`, …).
#[must_use]
pub fn is_font_id(name: &str) -> bool {
    is_ascii_ident(name)
}
