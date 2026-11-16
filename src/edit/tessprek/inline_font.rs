//! Inline `\font{font_id}{text}` / `\size{N}{text}` / `\icon{name}` extraction (D23 / THI-356).
//!
//! Rewrites macros to bare inner text + [`PendingFont`](crate::io::font::PendingFont)
//! ranges; [`crate::edit::compile`] seals them to [`InlineKind::Font`](crate::catalog::InlineKind::Font).

use crate::catalog::chunk::is_font_id;
use crate::catalog::icon_by_name;
use crate::error::{Result, TesError};
use crate::io::font::PendingFont;

use super::brace::find_unquoted_close_brace;

/// Result of stripping font/size/icon macros from a body.
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

/// Rewrite `\font{id}{text}` / `\size{N}{text}` / `\icon{name}` to bare text + pending spans.
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
        if body[i..].starts_with("\\icon{") {
            i = take_icon(body, i, 0, &mut out, &mut pending, &mut old_to_new)?;
            continue;
        }
        if body[i..].starts_with("\\font{") {
            i = take_font(body, i, 0, &mut out, &mut pending, &mut old_to_new)?;
            continue;
        }
        if body[i..].starts_with("\\size{") {
            i = take_size(body, i, 0, &mut out, &mut pending, &mut old_to_new)?;
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
    fill_non_boundary_slots(body, &mut old_to_new);
    Ok(FontExtract {
        body: out,
        pending,
        old_to_new,
    })
}

/// Emit Tessprek macros for a pending font span (encode / layout place).
#[must_use]
pub(crate) fn format_pending_font_macro(font: &PendingFont, inner: &str) -> String {
    let mut wrapped = inner.to_owned();
    if let Some(milli) = font.size_milli_pt {
        let pt = milli / 1000;
        wrapped = format!("\\size{{{pt}}}{{{wrapped}}}");
    }
    if let Some(ref id) = font.font_id {
        if let Some(name) = crate::catalog::icon_name_for_face_glyph(id, &wrapped) {
            format!("\\icon{{{name}}}")
        } else {
            format!("\\font{{{id}}}{{{wrapped}}}")
        }
    } else {
        wrapped
    }
}

fn take_icon(
    body: &str,
    local_open: usize,
    abs_base: usize,
    out: &mut String,
    pending: &mut Vec<PendingFont>,
    old_to_new: &mut [u32],
) -> Result<usize> {
    let abs_open = abs_base + local_open;
    let rest = &body[local_open + "\\icon{".len()..];
    let name_start = abs_open + "\\icon{".len();
    let Some(name_close) = find_unquoted_close_brace(rest) else {
        return Err(TesError::EditParse {
            line: 1,
            column: abs_open.saturating_add(1),
            message: "unclosed \\icon{…}".into(),
        });
    };
    let name = rest[..name_close].trim();
    let abs_close_end = name_start + name_close + 1;
    let Some(def) = icon_by_name(name) else {
        return Err(TesError::EditParse {
            line: 1,
            column: abs_open.saturating_add(1),
            message: format!("unknown \\icon{{{name}}}"),
        });
    };
    let start = u32::try_from(out.len()).unwrap_or(u32::MAX);
    for slot in old_to_new.iter_mut().take(abs_close_end).skip(abs_open) {
        *slot = start;
    }
    out.push(def.glyph);
    let end = u32::try_from(out.len()).unwrap_or(u32::MAX);
    pending.push(PendingFont {
        start,
        end,
        font_id: Some(def.face.to_owned()),
        size_milli_pt: None,
    });
    Ok(abs_close_end)
}

fn take_font(
    body: &str,
    local_open: usize,
    abs_base: usize,
    out: &mut String,
    pending: &mut Vec<PendingFont>,
    old_to_new: &mut [u32],
) -> Result<usize> {
    let abs_open = abs_base + local_open;
    let rest = &body[local_open + "\\font{".len()..];
    let Some(id_close) = find_unquoted_close_brace(rest) else {
        return Err(TesError::EditParse {
            line: 1,
            column: abs_open.saturating_add(1),
            message: "unclosed \\font{…}".into(),
        });
    };
    let font_id = rest[..id_close].trim();
    let after_id_local = local_open + "\\font{".len() + id_close + 1;
    if !is_font_id(font_id) {
        return Err(TesError::EditParse {
            line: 1,
            column: abs_open.saturating_add(1),
            message: format!("invalid \\font id {font_id:?}"),
        });
    }
    let Some(text_rest) = body.get(after_id_local..).and_then(|s| s.strip_prefix('{')) else {
        return Err(TesError::EditParse {
            line: 1,
            column: abs_base.saturating_add(after_id_local).saturating_add(1),
            message: "\\font{id} requires a second {text} brace".into(),
        });
    };
    let text_local_start = after_id_local + 1;
    let text_inner_start = abs_base + text_local_start;
    let Some(text_close) = find_unquoted_close_brace(text_rest) else {
        return Err(TesError::EditParse {
            line: 1,
            column: text_inner_start.saturating_add(1),
            message: "unclosed \\font{id}{…}".into(),
        });
    };
    let text = &body[text_local_start..text_local_start + text_close];
    let abs_close_end = text_inner_start + text_close + 1;

    for slot in old_to_new.iter_mut().take(text_inner_start).skip(abs_open) {
        *slot = u32::try_from(out.len()).unwrap_or(u32::MAX);
    }
    let pending_base = pending.len();
    let start = u32::try_from(out.len()).unwrap_or(u32::MAX);
    extract_brace_inner(text, out, pending, old_to_new, text_inner_start)?;
    let end = u32::try_from(out.len()).unwrap_or(u32::MAX);
    map_tail(
        old_to_new,
        text_inner_start + text_close,
        abs_close_end,
        end,
    );
    merge_pending_inherit(
        &mut pending[pending_base..],
        start,
        end,
        &Some(font_id.to_owned()),
        None,
    );
    // Nested `\font{id}{\size{N}{…}}` already covers this range after inherit;
    // pushing an outer font-only pending double-wraps on re-encode (THI-723).
    if !child_covers_font_id(&pending[pending_base..], start, end, font_id) {
        pending.push(PendingFont {
            start,
            end,
            font_id: Some(font_id.to_owned()),
            size_milli_pt: None,
        });
    }
    Ok(abs_close_end)
}

fn take_size(
    body: &str,
    local_open: usize,
    abs_base: usize,
    out: &mut String,
    pending: &mut Vec<PendingFont>,
    old_to_new: &mut [u32],
) -> Result<usize> {
    let abs_open = abs_base + local_open;
    let rest = &body[local_open + "\\size{".len()..];
    let Some(pt_close) = find_unquoted_close_brace(rest) else {
        return Err(TesError::EditParse {
            line: 1,
            column: abs_open.saturating_add(1),
            message: "unclosed \\size{…}".into(),
        });
    };
    let pt_str = rest[..pt_close].trim();
    let pt: u32 = pt_str.parse().map_err(|_| TesError::EditParse {
        line: 1,
        column: abs_open.saturating_add(1),
        message: format!("invalid \\size points {pt_str:?}"),
    })?;
    let after_pt_local = local_open + "\\size{".len() + pt_close + 1;
    let Some(text_rest) = body.get(after_pt_local..).and_then(|s| s.strip_prefix('{')) else {
        return Err(TesError::EditParse {
            line: 1,
            column: abs_base.saturating_add(after_pt_local).saturating_add(1),
            message: "\\size{N} requires a second {text} brace".into(),
        });
    };
    let text_local_start = after_pt_local + 1;
    let text_inner_start = abs_base + text_local_start;
    let Some(text_close) = find_unquoted_close_brace(text_rest) else {
        return Err(TesError::EditParse {
            line: 1,
            column: text_inner_start.saturating_add(1),
            message: "unclosed \\size{N}{…}".into(),
        });
    };
    let text = &body[text_local_start..text_local_start + text_close];
    let abs_close_end = text_inner_start + text_close + 1;
    let size_milli_pt = pt.saturating_mul(1000);

    for slot in old_to_new.iter_mut().take(text_inner_start).skip(abs_open) {
        *slot = u32::try_from(out.len()).unwrap_or(u32::MAX);
    }
    let pending_base = pending.len();
    let start = u32::try_from(out.len()).unwrap_or(u32::MAX);
    extract_brace_inner(text, out, pending, old_to_new, text_inner_start)?;
    let end = u32::try_from(out.len()).unwrap_or(u32::MAX);
    map_tail(
        old_to_new,
        text_inner_start + text_close,
        abs_close_end,
        end,
    );
    merge_pending_inherit(
        &mut pending[pending_base..],
        start,
        end,
        &None,
        Some(size_milli_pt),
    );
    // Nested `\size{N}{\font{id}{…}}` already covers this range after inherit;
    // skip coextensive outer size-only (THI-723).
    if !child_covers_size(&pending[pending_base..], start, end, size_milli_pt) {
        pending.push(PendingFont {
            start,
            end,
            font_id: None,
            size_milli_pt: Some(size_milli_pt),
        });
    }
    Ok(abs_close_end)
}

/// Recursively strip nested font/size/icon macros inside a `{text}` brace payload.
fn extract_brace_inner(
    text: &str,
    out: &mut String,
    pending: &mut Vec<PendingFont>,
    old_to_new: &mut [u32],
    abs_text_inner_start: usize,
) -> Result<()> {
    let mut i = 0usize;
    let out_base = out.len();
    while i < text.len() {
        let abs = abs_text_inner_start + i;
        if text[i..].starts_with("\\icon{") {
            let rel = take_icon(text, i, abs_text_inner_start, out, pending, old_to_new)?;
            i = rel - abs_text_inner_start;
            continue;
        }
        if text[i..].starts_with("\\font{") {
            let rel = take_font(text, i, abs_text_inner_start, out, pending, old_to_new)?;
            i = rel - abs_text_inner_start;
            continue;
        }
        if text[i..].starts_with("\\size{") {
            let rel = take_size(text, i, abs_text_inner_start, out, pending, old_to_new)?;
            i = rel - abs_text_inner_start;
            continue;
        }
        let Some(ch) = text[i..].chars().next() else {
            break;
        };
        let new_pos = u32::try_from(out.len()).unwrap_or(u32::MAX);
        old_to_new[abs] = new_pos;
        out.push(ch);
        i += ch.len_utf8();
    }
    let _ = out_base;
    Ok(())
}

fn merge_pending_inherit(
    pending: &mut [PendingFont],
    outer_start: u32,
    outer_end: u32,
    outer_font_id: &Option<String>,
    outer_size: Option<u32>,
) {
    for p in pending.iter_mut() {
        if p.start >= outer_start && p.end <= outer_end {
            if p.font_id.is_none() {
                p.font_id.clone_from(outer_font_id);
            }
            if p.size_milli_pt.is_none() {
                p.size_milli_pt = outer_size;
            }
        }
    }
}

fn child_covers_font_id(pending: &[PendingFont], start: u32, end: u32, font_id: &str) -> bool {
    pending
        .iter()
        .any(|p| p.start == start && p.end == end && p.font_id.as_deref() == Some(font_id))
}

fn child_covers_size(pending: &[PendingFont], start: u32, end: u32, size_milli_pt: u32) -> bool {
    pending
        .iter()
        .any(|p| p.start == start && p.end == end && p.size_milli_pt == Some(size_milli_pt))
}

fn map_tail(old_to_new: &mut [u32], from: usize, to: usize, pos: u32) {
    for slot in old_to_new.iter_mut().take(to).skip(from) {
        *slot = pos;
    }
}

fn fill_non_boundary_slots(body: &str, old_to_new: &mut [u32]) {
    let mut last = 0u32;
    for (idx, slot) in old_to_new.iter_mut().enumerate() {
        if idx < body.len() && body.is_char_boundary(idx) {
            last = *slot;
        } else if idx < body.len() {
            *slot = last;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_font_keep_inner() {
        let (body, fonts) = extract_inline_fonts("say \\font{armenian}{barev} now").unwrap();
        assert_eq!(body, "say barev now");
        assert_eq!(fonts.len(), 1);
        assert_eq!(fonts[0].font_id.as_deref(), Some("armenian"));
        assert_eq!(fonts[0].size_milli_pt, None);
        assert_eq!(
            &body[fonts[0].start as usize..fonts[0].end as usize],
            "barev"
        );
    }

    #[test]
    fn extracts_size_only() {
        let (body, fonts) = extract_inline_fonts("big \\size{14}{word} end").unwrap();
        assert_eq!(body, "big word end");
        assert_eq!(fonts.len(), 1);
        assert_eq!(fonts[0].font_id, None);
        assert_eq!(fonts[0].size_milli_pt, Some(14_000));
        assert_eq!(
            &body[fonts[0].start as usize..fonts[0].end as usize],
            "word"
        );
    }

    #[test]
    fn extracts_nested_font_and_size() {
        let (body, fonts) =
            extract_inline_fonts("x \\font{armenian}{\\size{12}{barev}} y").unwrap();
        assert_eq!(body, "x barev y");
        assert_eq!(fonts.len(), 1, "coextensive nest must be one merged pending: {fonts:?}");
        assert_eq!(fonts[0].font_id.as_deref(), Some("armenian"));
        assert_eq!(fonts[0].size_milli_pt, Some(12_000));
        assert_eq!(
            &body[fonts[0].start as usize..fonts[0].end as usize],
            "barev"
        );
    }

    #[test]
    fn extracts_nested_size_then_font() {
        let (body, fonts) =
            extract_inline_fonts("x \\size{12}{\\font{armenian}{barev}} y").unwrap();
        assert_eq!(body, "x barev y");
        assert_eq!(fonts.len(), 1, "coextensive nest must be one merged pending: {fonts:?}");
        assert_eq!(fonts[0].font_id.as_deref(), Some("armenian"));
        assert_eq!(fonts[0].size_milli_pt, Some(12_000));
        assert_eq!(
            &body[fonts[0].start as usize..fonts[0].end as usize],
            "barev"
        );
    }

    #[test]
    fn outer_font_still_covers_partial_inner_size() {
        let (body, fonts) =
            extract_inline_fonts("\\font{armenian}{Hi \\size{9}{Lo}}").unwrap();
        assert_eq!(body, "Hi Lo");
        assert_eq!(fonts.len(), 2, "{fonts:?}");
        let full = fonts
            .iter()
            .find(|f| f.size_milli_pt.is_none())
            .expect("outer font-only");
        assert_eq!(full.font_id.as_deref(), Some("armenian"));
        assert_eq!(&body[full.start as usize..full.end as usize], "Hi Lo");
        let sized = fonts
            .iter()
            .find(|f| f.size_milli_pt == Some(9_000))
            .expect("inner size");
        assert_eq!(sized.font_id.as_deref(), Some("armenian"));
        assert_eq!(&body[sized.start as usize..sized.end as usize], "Lo");
    }

    #[test]
    fn format_merged_face_and_size_nests_once() {
        let font = PendingFont {
            start: 0,
            end: 5,
            font_id: Some("greek".into()),
            size_milli_pt: Some(9_000),
        };
        assert_eq!(
            format_pending_font_macro(&font, "Hello"),
            "\\font{greek}{\\size{9}{Hello}}"
        );
    }

    #[test]
    fn extracts_icon_to_fab_glyph() {
        let (body, fonts) = extract_inline_fonts("go \\icon{github} now").unwrap();
        assert_eq!(body, "go \u{f09b} now");
        assert_eq!(fonts.len(), 1);
        assert_eq!(fonts[0].font_id.as_deref(), Some("fab"));
        assert_eq!(
            &body[fonts[0].start as usize..fonts[0].end as usize],
            "\u{f09b}"
        );
    }

    #[test]
    fn remaps_spans_around_multibyte_pua() {
        let glyph = "\u{f0e1}";
        let raw = format!("pre\\font{{fab}}{{{glyph}}}post");
        let extracted = extract_inline_fonts_mapped(&raw).unwrap();
        assert_eq!(extracted.body, format!("pre{glyph}post"));
        let (s, e) = extracted
            .remap_range(0, raw.len() as u32)
            .expect("full remap");
        assert_eq!(&extracted.body[s as usize..e as usize], extracted.body);
    }

    #[test]
    fn rejects_unknown_icon() {
        let err = extract_inline_fonts("\\icon{nope}").unwrap_err();
        assert!(format!("{err}").contains("unknown"), "{err}");
    }

    #[test]
    fn rejects_font_without_text() {
        let err = extract_inline_fonts("\\font{armenian}").unwrap_err();
        assert!(format!("{err}").contains("second"), "{err}");
    }

    #[test]
    fn rejects_bad_font_id() {
        let err = extract_inline_fonts("\\font{bad-id}{x}").unwrap_err();
        assert!(format!("{err}").contains("invalid"), "{err}");
    }

    #[test]
    fn rejects_size_without_text() {
        let err = extract_inline_fonts("\\size{12}").unwrap_err();
        assert!(format!("{err}").contains("second"), "{err}");
    }
}
