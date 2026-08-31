//! Inline `\footnote{…}` / `\endnote{…}` extraction for Tessprek bodies.

use crate::catalog::chunk::{NOTE_MARKER, NoteKind, PendingNote};
use crate::error::{Result, TesError};

use super::brace::find_unquoted_close_brace;

/// Rewrite inline `\footnote{…}` / `\endnote{…}` to a ZWSP marker + pending notes.
///
/// # Errors
///
/// Unclosed braces or empty note bodies.
pub(crate) fn extract_inline_notes(body: &str) -> Result<(String, Vec<PendingNote>)> {
    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len());
    let mut pending = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if let Some((kind, prefix)) = note_opener(&body[i..]) {
            let abs_open = i;
            let rest = &body[i + prefix.len()..];
            let Some(rel_close) = find_unquoted_close_brace(rest) else {
                return Err(TesError::EditParse {
                    line: 1,
                    column: abs_open.saturating_add(1),
                    message: format!("unclosed \\{}{{…}}", kind.tessprek_name()),
                });
            };
            let inner = rest[..rel_close].trim();
            if inner.is_empty() {
                return Err(TesError::EditParse {
                    line: 1,
                    column: abs_open.saturating_add(1),
                    message: format!("empty \\{}{{…}}", kind.tessprek_name()),
                });
            }
            let abs_close_end = i + prefix.len() + rel_close + 1;
            let start = u32::try_from(out.len()).unwrap_or(u32::MAX);
            out.push_str(NOTE_MARKER);
            let end = u32::try_from(out.len()).unwrap_or(u32::MAX);
            pending.push(PendingNote {
                start,
                end,
                kind,
                body: inner.to_owned(),
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

fn note_opener(rest: &str) -> Option<(NoteKind, &'static str)> {
    if rest.starts_with("\\footnote{") {
        Some((NoteKind::Footnote, "\\footnote{"))
    } else if rest.starts_with("\\endnote{") {
        Some((NoteKind::Endnote, "\\endnote{"))
    } else {
        None
    }
}

impl NoteKind {
    fn tessprek_name(self) -> &'static str {
        match self {
            Self::Footnote => "footnote",
            Self::Endnote => "endnote",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_footnote_and_leaves_marker() {
        let (body, notes) = extract_inline_notes("see \\footnote{A note.} now").unwrap();
        assert_eq!(body, format!("see {NOTE_MARKER} now"));
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].kind, NoteKind::Footnote);
        assert_eq!(notes[0].body, "A note.");
    }

    #[test]
    fn extracts_endnote() {
        let (body, notes) = extract_inline_notes("x\\endnote{End.}y").unwrap();
        assert_eq!(body, format!("x{NOTE_MARKER}y"));
        assert_eq!(notes[0].kind, NoteKind::Endnote);
        assert_eq!(notes[0].body, "End.");
    }
}
