//! `[[wikilink]]` scan / rewrite helpers for vault import.

use crate::catalog::format_internal_destination;
use crate::error::{Result, TesError};
use uuid::Uuid;

/// One `[[target]]` / `[[target|label]]` / `[[note#section|label]]` span in Markdown source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WikilinkSpan<'a> {
    /// Byte offset of the opening `[[`.
    pub start: usize,
    /// Byte offset immediately after the closing `]]`.
    pub end: usize,
    /// Note name (left of `#` and `|`, trimmed).
    pub target: &'a str,
    /// Heading / section name after `#` (before `|`), when present.
    pub section: Option<&'a str>,
    /// Display label (right of `|`, or the full inner text when unlabeled).
    pub label: &'a str,
}

/// Invoke `visitor` for each Obsidian-style wikilink in `markdown`.
pub fn visit_wikilinks(markdown: &str, mut visitor: impl FnMut(WikilinkSpan<'_>)) {
    let bytes = markdown.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'['
            && bytes[i + 1] == b'['
            && let Some(close) = find_wikilink_end(markdown, i + 2)
        {
            let inner = &markdown[i + 2..close];
            let (raw_target, label) = if let Some((t, l)) = inner.split_once('|') {
                (t.trim(), l.trim())
            } else {
                let t = inner.trim();
                (t, t)
            };
            let (target, section) = split_note_section(raw_target);
            visitor(WikilinkSpan {
                start: i,
                end: close + 2,
                target,
                section,
                label,
            });
            i = close + 2;
            continue;
        }
        i += 1;
    }
}

/// Split `Note#Section` into note name and optional section (first `#`).
#[must_use]
pub fn split_note_section(raw_target: &str) -> (&str, Option<&str>) {
    match raw_target.split_once('#') {
        Some((note, section)) => {
            let note = note.trim();
            let section = section.trim();
            if section.is_empty() {
                (note, None)
            } else {
                (note, Some(section))
            }
        }
        None => (raw_target, None),
    }
}

/// Collect wikilink note names for which `is_resolved` returns false (unique via `out`).
///
/// Section fragments are ignored for resolve — a missing heading is a hard error
/// at rewrite time, not an unresolved-note soft warning.
pub fn collect_unresolved_wikilinks(
    markdown: &str,
    is_resolved: impl Fn(&str) -> bool,
    out: &mut std::collections::HashSet<String>,
) {
    visit_wikilinks(markdown, |span| {
        if !span.target.is_empty() && !is_resolved(span.target) {
            out.insert(span.target.to_owned());
        }
    });
}

/// Rewrite `[[target]]` / `[[note#section|label]]` to `[label](dest)` when resolved.
///
/// `resolve_note` maps the note name (title / slug / alias / stem) to a document UUID.
/// When a `#section` is present, `resolve_section(doc_uuid, section)` must return that
/// heading's chunk id — a miss is [`TesError::InvalidLink`] (not a silent whole-note link).
///
/// Unresolved *notes* are left unchanged in the output.
///
/// # Errors
///
/// Returns [`TesError::InvalidLink`] when the note resolves but the section heading does not.
pub fn rewrite_wikilinks(
    markdown: &str,
    resolve_note: &dyn Fn(&str) -> Option<String>,
    resolve_section: Option<&dyn Fn(&str, &str) -> Result<u64>>,
) -> Result<String> {
    let mut out = String::with_capacity(markdown.len());
    let mut cursor = 0;
    let mut error: Option<TesError> = None;
    visit_wikilinks(markdown, |span| {
        if error.is_some() {
            return;
        }
        let Some(uuid) = resolve_note(span.target) else {
            return;
        };
        let dest = if let Some(section) = span.section {
            let Some(resolve_section) = resolve_section else {
                // No section resolver: leave `[[Note#Section]]` unchanged rather
                // than flattening to a whole-note link.
                return;
            };
            let chunk_id = match resolve_section(&uuid, section) {
                Ok(id) => id,
                Err(err) => {
                    error = Some(err);
                    return;
                }
            };
            let doc_id = match Uuid::parse_str(&uuid) {
                Ok(id) => id,
                Err(_) => {
                    error = Some(TesError::InvalidDocId {
                        value: uuid.clone(),
                    });
                    return;
                }
            };
            format_internal_destination(doc_id, chunk_id)
        } else {
            uuid
        };
        out.push_str(&markdown[cursor..span.start]);
        out.push('[');
        out.push_str(span.label);
        out.push_str("](");
        out.push_str(&dest);
        out.push(')');
        cursor = span.end;
    });
    if let Some(err) = error {
        return Err(err);
    }
    out.push_str(&markdown[cursor..]);
    Ok(out)
}

fn find_wikilink_end(markdown: &str, start: usize) -> Option<usize> {
    let bytes = markdown.as_bytes();
    let mut j = start;
    while j + 1 < bytes.len() {
        if bytes[j] == b']' && bytes[j + 1] == b']' {
            return Some(j);
        }
        j += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_note_and_section() {
        let mut seen = Vec::new();
        visit_wikilinks("[[Resume#Experience|the job]] and [[Resume]]", |span| {
            seen.push((
                span.target.to_owned(),
                span.section.map(str::to_owned),
                span.label.to_owned(),
            ));
        });
        assert_eq!(
            seen,
            vec![
                (
                    "Resume".to_owned(),
                    Some("Experience".to_owned()),
                    "the job".to_owned()
                ),
                ("Resume".to_owned(), None, "Resume".to_owned()),
            ]
        );
    }

    #[test]
    fn rewrite_section_requires_heading() {
        let note = "550e8400-e29b-41d4-a716-446655440000";
        let err = rewrite_wikilinks(
            "See [[Resume#Missing]].",
            &|name| (name == "Resume").then(|| note.to_owned()),
            Some(&|_uuid, section| {
                Err(TesError::InvalidLink {
                    message: format!("wikilink section heading not found: {section}"),
                })
            }),
        )
        .unwrap_err();
        assert!(matches!(err, TesError::InvalidLink { .. }));
        assert!(err.to_string().contains("Missing"));
    }

    #[test]
    fn rewrite_section_encodes_chunk_fragment() {
        let note = "550e8400-e29b-41d4-a716-446655440000";
        let out = rewrite_wikilinks(
            "See [[Resume#Experience|the job]].",
            &|name| (name == "Resume").then(|| note.to_owned()),
            Some(&|_uuid, section| {
                assert_eq!(section, "Experience");
                Ok(4)
            }),
        )
        .unwrap();
        assert_eq!(
            out,
            "See [the job](550e8400-e29b-41d4-a716-446655440000#chunk-4)."
        );
    }
}
