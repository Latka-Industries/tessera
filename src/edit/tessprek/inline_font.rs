//! Inline `\font{font_id}{text}` extraction for Tessprek bodies (D23 / THI-356).
//!
//! Rewrites macros to bare inner text + [`PendingFont`](crate::io::font::PendingFont)
//! ranges; [`crate::edit::compile`] seals them to [`InlineKind::Font`](crate::catalog::InlineKind::Font).

use crate::catalog::chunk::is_font_id;
use crate::error::{Result, TesError};
use crate::io::font::PendingFont;

use super::brace::find_unquoted_close_brace;

/// Result of stripping `\font{…}{…}` macros from a body.
#[derive(Debug, Clone)]
pub(crate) struct FontExtract {
    /// Body with macros replaced by their inner text.
    pub body: String,
    /// Font ranges into [`Self::body`].
    pub pending: Vec<PendingFont>,
    /// Maps each original byte offset → offset in [`Self::body`].
    ///
    /// Length is `original.len() + 1` (includes EOF). Deleted scaffolding bytes
    /// map to the output position where the next kept byte lands.
    pub old_to_new: Vec<u32>,
}

impl FontExtract {
    /// Remap a half-open span from the pre-extract body into [`Self::body`].
    #[must_use]
    pub fn remap_range(&self, start: u32, end: u32) -> Option<(u32, u32)> {
        let start = self.map_offset(start)?;
        let end = self.map_offset(end)?;
        (end > start).then_some((start, end))
    }

    fn map_offset(&self, old: u32) -> Option<u32> {
        let idx = usize::try_from(old).ok()?;
        self.old_to_new.get(idx).copied()
    }
}

/// Rewrite `\font{id}{text}` to bare `text` + [`PendingFont`] spans.
///
/// # Errors
///
/// Returns [`TesError::EditParse`] for malformed openers / empty / bad ids.
pub(crate) fn extract_inline_fonts(body: &str) -> Result<(String, Vec<PendingFont>)> {
    let extracted = extract_inline_fonts_mapped(body)?;
    Ok((extracted.body, extracted.pending))
}

/// Like [`extract_inline_fonts`], but also returns an offset remap table.
///
/// Use the remap when Markdown spans/links were measured on the pre-strip body
/// (common after `parse_markdown_blocks` then font extract).
pub(crate) fn extract_inline_fonts_mapped(body: &str) -> Result<FontExtract> {
    let mut out = String::with_capacity(body.len());
    let mut pending = Vec::new();
    let mut old_to_new = vec![0u32; body.len().saturating_add(1)];
    let mut i = 0usize;
    while i < body.len() {
        if let Some(rest) = body[i..].strip_prefix("\\font{") {
            let abs_open = i;
            let id_start = abs_open + "\\font{".len();
            let Some(id_close) = find_unquoted_close_brace(rest) else {
                return Err(TesError::EditParse {
                    line: 1,
                    column: abs_open.saturating_add(1),
                    message: "unclosed \\font{…}".into(),
                });
            };
            let font_id = rest[..id_close].trim();
            let after_id = id_start + id_close + 1;
            if !is_font_id(font_id) {
                return Err(TesError::EditParse {
                    line: 1,
                    column: abs_open.saturating_add(1),
                    message: format!("invalid \\font id {font_id:?}"),
                });
            }
            let Some(text_rest) = body.get(after_id..).and_then(|s| s.strip_prefix('{')) else {
                return Err(TesError::EditParse {
                    line: 1,
                    column: after_id.saturating_add(1),
                    message: "\\font{id} requires a second {text} brace".into(),
                });
            };
            let text_inner_start = after_id + 1;
            let Some(text_close) = find_unquoted_close_brace(text_rest) else {
                return Err(TesError::EditParse {
                    line: 1,
                    column: text_inner_start.saturating_add(1),
                    message: "unclosed \\font{id}{…}".into(),
                });
            };
            let text = &text_rest[..text_close];
            let abs_close_end = text_inner_start + text_close + 1;

            // Scaffolding before inner text maps to the start of the kept text.
            let start = u32::try_from(out.len()).unwrap_or(u32::MAX);
            for slot in old_to_new.iter_mut().take(text_inner_start).skip(abs_open) {
                *slot = start;
            }
            out.push_str(text);
            let end = u32::try_from(out.len()).unwrap_or(u32::MAX);
            // Each kept inner byte maps 1:1.
            let mut new_pos = start;
            let mut k = text_inner_start;
            while k < text_inner_start + text_close {
                old_to_new[k] = new_pos;
                let ch = body[k..].chars().next().expect("inner text char");
                let n = ch.len_utf8();
                new_pos = new_pos.saturating_add(u32::try_from(n).unwrap_or(0));
                k += n;
            }
            // Closing `}` and any multi-byte edge: map to end of kept text.
            for slot in old_to_new
                .iter_mut()
                .take(abs_close_end)
                .skip(text_inner_start + text_close)
            {
                *slot = end;
            }
            pending.push(PendingFont {
                start,
                end,
                font_id: font_id.to_owned(),
            });
            i = abs_close_end;
            continue;
        }
        let Some(ch) = body[i..].chars().next() else {
            break;
        };
        let new_pos = u32::try_from(out.len()).unwrap_or(u32::MAX);
        old_to_new[i] = new_pos;
        out.push(ch);
        i += ch.len_utf8();
    }
    old_to_new[body.len()] = u32::try_from(out.len()).unwrap_or(u32::MAX);
    // Fill any gaps left by multi-byte chars (only first byte of each char was set
    // in the plain-copy path). Walk and forward-fill within each char.
    let mut last = 0u32;
    for (idx, slot) in old_to_new.iter_mut().enumerate() {
        if idx < body.len() && body.is_char_boundary(idx) {
            last = *slot;
        } else if idx < body.len() {
            *slot = last;
        }
    }
    Ok(FontExtract {
        body: out,
        pending,
        old_to_new,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_font_keep_inner() {
        let (body, fonts) = extract_inline_fonts("say \\font{armenian}{barev} now").unwrap();
        assert_eq!(body, "say barev now");
        assert_eq!(fonts.len(), 1);
        assert_eq!(fonts[0].font_id, "armenian");
        assert_eq!(
            &body[fonts[0].start as usize..fonts[0].end as usize],
            "barev"
        );
    }

    #[test]
    fn remaps_spans_around_multibyte_pua() {
        // Font Awesome PUA (e.g. linkedin) is typically U+F0E1 (3 UTF-8 bytes).
        let glyph = "\u{f0e1}";
        let raw = format!("pre\\font{{fab}}{{{glyph}}}post");
        let extracted = extract_inline_fonts_mapped(&raw).unwrap();
        assert_eq!(extracted.body, format!("pre{glyph}post"));
        let macro_start = raw.find('\\').unwrap() as u32;
        let macro_end = (raw.rfind('}').unwrap() + 1) as u32;
        let (s, e) = extracted
            .remap_range(macro_start, macro_end)
            .expect("remap");
        assert_eq!(&extracted.body[s as usize..e as usize], glyph);
        assert!(extracted.body.is_char_boundary(s as usize));
        assert!(extracted.body.is_char_boundary(e as usize));
    }

    #[test]
    fn rejects_missing_text_brace() {
        let err = extract_inline_fonts("\\font{armenian}").unwrap_err();
        assert!(matches!(err, TesError::EditParse { .. }));
    }

    #[test]
    fn rejects_bad_id() {
        let err = extract_inline_fonts("\\font{bad-id}{x}").unwrap_err();
        assert!(matches!(err, TesError::EditParse { .. }));
    }
}
