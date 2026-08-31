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
    /// Footnote or endnote callout (THI-396). Body is the note text.
    Note {
        /// Page-bottom vs end dump.
        kind: NoteKind,
        /// Note body (plain text in v1).
        body: String,
    },
}

/// Footnote vs endnote (Tessprek `\footnote` / `\endnote`; THI-396).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    /// Page-bottom band (weave `NoteKind::Footnote`).
    Footnote,
    /// Dump after last body block (weave `NoteKind::Endnote`).
    Endnote,
}

/// Pending inline note from Tessprek `\footnote{…}` / `\endnote{…}` extraction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingNote {
    /// Inclusive start byte offset in the rewritten body (ZWSP marker).
    pub start: u32,
    /// Exclusive end byte offset in the rewritten body.
    pub end: u32,
    /// Footnote vs endnote.
    pub kind: NoteKind,
    /// Note body from the brace.
    pub body: String,
}

/// Zero-width marker left in the body so the sealed span is non-empty.
pub const NOTE_MARKER: &str = "\u{200b}";

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
///
/// Nested spans (e.g. Font glyph inside a longer Link label) are wrapped
/// innermost-first so outer `](url)` never lands inside `\font{…}` / `\icon{…}`.
pub(super) fn apply_spans_markdown(
    body: &str,
    spans: &[InlineSpan],
    links: &[LinkEntry],
) -> String {
    if spans.is_empty() {
        return body.to_owned();
    }
    let active: Vec<&InlineSpan> = spans.iter().filter(|s| s.end > s.start).collect();
    render_span_region(
        body,
        0,
        u32::try_from(body.len()).unwrap_or(u32::MAX),
        &active,
        links,
    )
}

fn render_span_region(
    body: &str,
    region_start: u32,
    region_end: u32,
    spans: &[&InlineSpan],
    links: &[LinkEntry],
) -> String {
    let rs = region_start as usize;
    let re = (region_end as usize).min(body.len());
    if rs >= re || !body.is_char_boundary(rs) || !body.is_char_boundary(re) {
        return String::new();
    }

    // Top-level = spans in this region not strictly inside another span here.
    let mut tops: Vec<&InlineSpan> = spans
        .iter()
        .copied()
        .filter(|s| s.start >= region_start && s.end <= region_end)
        .filter(|s| {
            !spans.iter().any(|o| {
                !std::ptr::eq(*o, *s)
                    && o.start <= s.start
                    && o.end >= s.end
                    && (o.start < s.start || o.end > s.end)
            })
        })
        .collect();
    // Right-to-left so earlier byte offsets stay stable while we build `out`
    // from the raw body slice... actually we build left-to-right from pieces.
    tops.sort_by_key(|s| (s.start, std::cmp::Reverse(s.end)));

    if tops.is_empty() {
        return body[rs..re].to_owned();
    }

    let mut out = String::new();
    let mut cursor = region_start;
    // Group coextensive tops and process left→right.
    let mut i = 0;
    while i < tops.len() {
        let start = tops[i].start;
        let end = tops[i].end;
        if start > cursor {
            let a = cursor as usize;
            let b = start as usize;
            if a < b && b <= body.len() && body.is_char_boundary(a) && body.is_char_boundary(b) {
                out.push_str(&body[a..b]);
            }
        }
        let mut group: Vec<&InlineSpan> = vec![tops[i]];
        i += 1;
        while i < tops.len() && tops[i].start == start && tops[i].end == end {
            group.push(tops[i]);
            i += 1;
        }
        // Children strictly inside this range (excluding the coextensive group).
        let children: Vec<&InlineSpan> = spans
            .iter()
            .copied()
            .filter(|s| s.start >= start && s.end <= end && (s.start > start || s.end < end))
            .filter(|s| !group.iter().any(|g| std::ptr::eq(*g, *s)))
            .collect();
        let mut inner = render_span_region(body, start, end, &children, links);
        // Font → style → Link (link outermost), same as before for coextensive.
        let order = |s: &&InlineSpan| -> u8 {
            match s.kind {
                InlineKind::Font { .. } => 0,
                InlineKind::Link { .. } => 2,
                _ => 1,
            }
        };
        group.sort_by_key(order);
        for span in group {
            inner = wrap_markdown_span(inner, span, links);
        }
        out.push_str(&inner);
        cursor = end;
    }
    if cursor < region_end {
        let a = cursor as usize;
        let b = re;
        if a < b && body.is_char_boundary(a) {
            out.push_str(&body[a..b]);
        }
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
        InlineKind::Font { font_id } => {
            if let Some(name) = crate::catalog::icon_name_for_face_glyph(font_id, &inner) {
                format!("\\icon{{{name}}}")
            } else {
                format!("\\font{{{font_id}}}{{{inner}}}")
            }
        }
        InlineKind::Note { kind, body } => match kind {
            NoteKind::Footnote => format!("\\footnote{{{body}}}"),
            NoteKind::Endnote => format!("\\endnote{{{body}}}"),
        },
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
