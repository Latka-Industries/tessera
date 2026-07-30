//! Open document map helpers: URI paths, Tessprek load, `didChange` apply.

use std::path::{Path, PathBuf};

use tower_lsp::lsp_types::{TextDocumentContentChangeEvent, Url};

use crate::edit::edit_read;

use super::position::position_to_utf8_offset;

/// In-memory Tessprek projection for one open `.tes` URI.
#[derive(Debug, Clone)]
pub(super) struct OpenDocument {
    pub(super) path: PathBuf,
    /// Last known on-disk hash from open (or last successful write-back).
    pub(super) source_hash: String,
    pub(super) tessprek: String,
}

pub(super) fn uri_to_path(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok()
}

pub(super) fn is_tes_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("tes"))
}

pub(super) fn load_open_document(path: PathBuf) -> Result<OpenDocument, String> {
    let report = edit_read(&path).map_err(|e| e.to_string())?;
    Ok(OpenDocument {
        path,
        source_hash: report.source_hash,
        tessprek: report.tessprek,
    })
}

/// Apply LSP content changes to `text` (full replace and/or incremental).
pub(super) fn apply_content_changes(
    text: &mut String,
    changes: &[TextDocumentContentChangeEvent],
) -> Result<(), String> {
    for change in changes {
        match change.range {
            None => {
                text.clone_from(&change.text);
            }
            Some(range) => {
                let start = position_to_utf8_offset(text, range.start).ok_or_else(|| {
                    format!(
                        "invalid start {}:{}",
                        range.start.line, range.start.character
                    )
                })?;
                let end = position_to_utf8_offset(text, range.end).ok_or_else(|| {
                    format!("invalid end {}:{}", range.end.line, range.end.character)
                })?;
                if start > end || end > text.len() {
                    return Err(format!("invalid range offsets {start}..{end}"));
                }
                text.replace_range(start..end, &change.text);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tower_lsp::lsp_types::{Position, Range};

    fn fixture_tes() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/v0/note_one_chunk.tes")
    }

    #[test]
    fn uri_round_trip_file_path() {
        let path = fixture_tes();
        let uri = Url::from_file_path(&path).expect("file URL");
        let back = uri_to_path(&uri).expect("path from URI");
        assert_eq!(back, path);
    }

    #[test]
    fn is_tes_path_accepts_tes_only() {
        assert!(is_tes_path(Path::new("/tmp/doc.tes")));
        assert!(is_tes_path(Path::new("/tmp/doc.TES")));
        assert!(!is_tes_path(Path::new("/tmp/doc.md")));
    }

    #[test]
    fn load_open_document_reads_tessprek_and_hash() {
        let path = fixture_tes();
        let doc = load_open_document(path.clone()).expect("edit_read");
        assert_eq!(doc.path, path);
        assert_eq!(doc.source_hash.len(), 64);
        assert!(
            doc.tessprek.contains("tessprek") || doc.tessprek.contains("tes chunk"),
            "expected Tessprek markers, got: {}",
            &doc.tessprek[..doc.tessprek.len().min(200)]
        );
    }

    #[test]
    fn apply_full_change_replaces_text() {
        let mut text = String::from("old");
        let changes = vec![TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "new tessprek".into(),
        }];
        apply_content_changes(&mut text, &changes).unwrap();
        assert_eq!(text, "new tessprek");
    }

    #[test]
    fn apply_incremental_change_utf16_safe() {
        let mut text = String::from("ab🙂cd");
        let changes = vec![TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 0,
                    character: 2,
                },
                end: Position {
                    line: 0,
                    character: 4,
                },
            }),
            range_length: None,
            text: "X".into(),
        }];
        apply_content_changes(&mut text, &changes).unwrap();
        assert_eq!(text, "abXcd");
    }

    #[test]
    fn did_change_preserves_source_hash_semantics() {
        let mut doc = load_open_document(fixture_tes()).unwrap();
        let hash = doc.source_hash.clone();
        apply_content_changes(
            &mut doc.tessprek,
            &[TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "<!-- edited -->\n".into(),
            }],
        )
        .unwrap();
        assert_eq!(doc.source_hash, hash);
        assert_eq!(doc.tessprek, "<!-- edited -->\n");
    }
}
