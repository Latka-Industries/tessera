//! Inline `\font{font_id}{text}` extraction for Tessprek bodies (D23 / THI-356).
//!
//! Rewrites macros to bare inner text + [`PendingFont`](crate::io::font::PendingFont)
//! ranges; [`crate::edit::compile`] seals them to [`InlineKind::Font`](crate::catalog::InlineKind::Font).

use crate::catalog::chunk::is_font_id;
use crate::error::{Result, TesError};
use crate::io::font::PendingFont;

use super::brace::find_unquoted_close_brace;

/// Rewrite `\font{id}{text}` to bare `text` + [`PendingFont`] spans.
///
/// # Errors
///
/// Returns [`TesError::EditParse`] for malformed openers / empty / bad ids.
pub(crate) fn extract_inline_fonts(body: &str) -> Result<(String, Vec<PendingFont>)> {
    let mut out = String::with_capacity(body.len());
    let mut pending = Vec::new();
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
            let start = u32::try_from(out.len()).unwrap_or(u32::MAX);
            out.push_str(text);
            let end = u32::try_from(out.len()).unwrap_or(u32::MAX);
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
        out.push(ch);
        i += ch.len_utf8();
    }
    Ok((out, pending))
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
