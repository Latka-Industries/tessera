//! Inline `\face{face_id}{text}` extraction for Tessprek bodies (D23 / THI-356).

use crate::catalog::chunk::is_face_id;
use crate::error::{Result, TesError};
use crate::io::face::PendingFace;

use super::brace::find_unquoted_close_brace;

/// Rewrite `\face{id}{text}` to bare `text` + [`PendingFace`] spans.
///
/// # Errors
///
/// Returns [`TesError::EditParse`] for malformed openers / empty / bad ids.
pub(crate) fn extract_inline_faces(body: &str) -> Result<(String, Vec<PendingFace>)> {
    let mut out = String::with_capacity(body.len());
    let mut pending = Vec::new();
    let mut i = 0usize;
    while i < body.len() {
        if let Some(rest) = body[i..].strip_prefix("\\face{") {
            let abs_open = i;
            let id_start = abs_open + "\\face{".len();
            let Some(id_close) = find_unquoted_close_brace(rest) else {
                return Err(TesError::EditParse {
                    line: 1,
                    column: abs_open.saturating_add(1),
                    message: "unclosed \\face{…}".into(),
                });
            };
            let face_id = rest[..id_close].trim();
            let after_id = id_start + id_close + 1;
            if !is_face_id(face_id) {
                return Err(TesError::EditParse {
                    line: 1,
                    column: abs_open.saturating_add(1),
                    message: format!("invalid \\face id {face_id:?}"),
                });
            }
            let Some(text_rest) = body.get(after_id..).and_then(|s| s.strip_prefix('{')) else {
                return Err(TesError::EditParse {
                    line: 1,
                    column: after_id.saturating_add(1),
                    message: "\\face{id} requires a second {text} brace".into(),
                });
            };
            let text_inner_start = after_id + 1;
            let Some(text_close) = find_unquoted_close_brace(text_rest) else {
                return Err(TesError::EditParse {
                    line: 1,
                    column: text_inner_start.saturating_add(1),
                    message: "unclosed \\face{id}{…}".into(),
                });
            };
            let text = &text_rest[..text_close];
            let abs_close_end = text_inner_start + text_close + 1;
            let start = u32::try_from(out.len()).unwrap_or(u32::MAX);
            out.push_str(text);
            let end = u32::try_from(out.len()).unwrap_or(u32::MAX);
            pending.push(PendingFace {
                start,
                end,
                face_id: face_id.to_owned(),
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
    fn extracts_face_keep_inner() {
        let (body, faces) = extract_inline_faces("say \\face{armenian}{barev} now").unwrap();
        assert_eq!(body, "say barev now");
        assert_eq!(faces.len(), 1);
        assert_eq!(faces[0].face_id, "armenian");
        assert_eq!(
            &body[faces[0].start as usize..faces[0].end as usize],
            "barev"
        );
    }

    #[test]
    fn rejects_missing_text_brace() {
        let err = extract_inline_faces("\\face{armenian}").unwrap_err();
        assert!(matches!(err, TesError::EditParse { .. }));
    }

    #[test]
    fn rejects_bad_id() {
        let err = extract_inline_faces("\\face{bad-id}{x}").unwrap_err();
        assert!(matches!(err, TesError::EditParse { .. }));
    }
}
