//! `tessera.write` / `edit_write` helpers for LSP write-back.

use tower_lsp::jsonrpc::{Error, ErrorCode, Result};
use tower_lsp::lsp_types::{ExecuteCommandParams, Url};

use crate::edit::{EditWriteOptions, edit_write};
use crate::error::TesError;

use super::document::OpenDocument;

/// LSP command: write the in-memory Tessprek buffer back to the `.tes` file.
pub const COMMAND_WRITE: &str = "tessera.write";

#[derive(Debug)]
pub(super) enum WriteBackError {
    HashMismatch {
        expected: String,
        found: String,
    },
    Parse {
        line: usize,
        column: usize,
        message: String,
    },
    Other(String),
}

/// Call [`edit_write`] and update `doc.source_hash` on success.
pub(super) fn write_back_document(
    doc: &mut OpenDocument,
) -> std::result::Result<String, WriteBackError> {
    let report = edit_write(
        &doc.path,
        &doc.tessprek,
        &EditWriteOptions::new(doc.source_hash.clone(), false),
    )
    .map_err(|err| match err {
        TesError::SourceHashMismatch { expected, found } => {
            WriteBackError::HashMismatch { expected, found }
        }
        TesError::EditParse {
            line,
            column,
            message,
        } => WriteBackError::Parse {
            line,
            column,
            message,
        },
        other => WriteBackError::Other(other.to_string()),
    })?;
    let new_hash = report
        .new_source_hash
        .ok_or_else(|| WriteBackError::Other("edit_write returned no new_source_hash".into()))?;
    doc.source_hash.clone_from(&new_hash);
    Ok(new_hash)
}

pub(super) fn parse_write_uri(params: &ExecuteCommandParams) -> Result<Url> {
    let arg = params.arguments.first().ok_or_else(|| Error {
        code: ErrorCode::InvalidParams,
        message: "tessera.write requires a document URI argument".into(),
        data: None,
    })?;
    let uri_str = if let Some(s) = arg.as_str() {
        s.to_owned()
    } else if let Some(obj) = arg.as_object() {
        obj.get("uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error {
                code: ErrorCode::InvalidParams,
                message: "tessera.write argument object must include string \"uri\"".into(),
                data: None,
            })?
            .to_owned()
    } else {
        return Err(Error {
            code: ErrorCode::InvalidParams,
            message: "tessera.write argument must be a URI string or {\"uri\":...}".into(),
            data: None,
        });
    };
    Url::parse(&uri_str).map_err(|e| Error {
        code: ErrorCode::InvalidParams,
        message: format!("tessera.write invalid URI: {e}").into(),
        data: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::edit::{edit_read, file_source_hash};
    use crate::verify::verify_tes_file;

    use super::super::document::load_open_document;

    fn fixture_tes() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/v0/note_one_chunk.tes")
    }

    #[test]
    fn write_back_round_trip_updates_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.tes");
        std::fs::copy(fixture_tes(), &path).unwrap();
        let mut doc = load_open_document(path.clone()).unwrap();
        let old = doc.source_hash.clone();
        assert!(
            doc.tessprek.contains("Hello from Tessera"),
            "fixture body missing: {}",
            doc.tessprek
        );
        doc.tessprek = doc
            .tessprek
            .replacen("Hello from Tessera", "Hallo from Tessera", 1);
        let new_hash = write_back_document(&mut doc).expect("write_back");
        assert_ne!(new_hash, old);
        assert_eq!(doc.source_hash, new_hash);
        assert_eq!(file_source_hash(&path).unwrap(), new_hash);
        assert!(verify_tes_file(&path, true).unwrap().ok);
        let again = edit_read(&path).unwrap();
        assert!(again.tessprek.contains("Hallo from Tessera"));
    }

    #[test]
    fn write_back_stale_hash_refuses() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.tes");
        std::fs::copy(fixture_tes(), &path).unwrap();
        let mut doc = load_open_document(path).unwrap();
        doc.source_hash = "deadbeef".into();
        let err = write_back_document(&mut doc).unwrap_err();
        assert!(
            matches!(err, WriteBackError::HashMismatch { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn write_back_parse_error_preserves_span() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.tes");
        std::fs::copy(fixture_tes(), &path).unwrap();
        let mut doc = load_open_document(path).unwrap();
        doc.tessprek = "\
<!-- tessera: format=tessprek version=1 -->\n\
\n\
<!-- tes chunk=1 role=not-a-real-role -->\n\
body\n\
"
        .into();
        let err = write_back_document(&mut doc).unwrap_err();
        match err {
            WriteBackError::Parse {
                line,
                column,
                message,
            } => {
                assert_eq!(line, 3);
                assert_eq!(column, 1);
                assert!(message.contains("unknown role"), "{message}");
            }
            other => panic!("expected Parse, got {other:?}"),
        }
    }
}
