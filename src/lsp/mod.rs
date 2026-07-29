//! Tessprek language server (`tes-lsp`).
//!
//! Thin LSP over stdio. Opens `.tes` files as Tessprek via [`edit_read`]
//! (THI-242) and keeps an in-memory Tessprek buffer via `didChange` (THI-243).
//! Diagnostics and write-back land in later children.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    InitializeParams, InitializeResult, InitializedParams, MessageType, Position,
    ServerCapabilities, ServerInfo, TextDocumentContentChangeEvent, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextDocumentSyncOptions, Url,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::edit::edit_read;

/// In-memory Tessprek projection for one open `.tes` URI.
#[derive(Debug, Clone)]
struct OpenDocument {
    path: PathBuf,
    /// Last known on-disk hash from open (or last successful write-back later).
    source_hash: String,
    tessprek: String,
}

/// LSP backend for Tessprek ↔ `.tes`.
#[derive(Debug)]
struct Backend {
    client: Client,
    documents: Mutex<HashMap<Url, OpenDocument>>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            documents: Mutex::new(HashMap::new()),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        // Full sync is enough for MVP; apply_content_changes also
                        // accepts incremental events if a client sends them.
                        change: Some(TextDocumentSyncKind::FULL),
                        ..Default::default()
                    },
                )),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "tes-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "tes-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let Some(path) = uri_to_path(&uri) else {
            let msg = format!("tes-lsp: skip open (not a file URI): {uri}");
            eprintln!("{msg}");
            self.client.log_message(MessageType::WARNING, msg).await;
            return;
        };

        if !is_tes_path(&path) {
            let msg = format!("tes-lsp: skip open (not a .tes file): {}", path.display());
            eprintln!("{msg}");
            self.client.log_message(MessageType::WARNING, msg).await;
            return;
        }

        let path_for_read = path.clone();
        let opened = tokio::task::spawn_blocking(move || load_open_document(path_for_read))
            .await
            .unwrap_or_else(|e| Err(format!("join error: {e}")));

        match opened {
            Ok(doc) => {
                let msg = format!(
                    "tes-lsp: opened {} source-hash={} tessprek-bytes={}",
                    doc.path.display(),
                    &doc.source_hash[..doc.source_hash.len().min(12)],
                    doc.tessprek.len()
                );
                eprintln!("{msg}");
                self.client.log_message(MessageType::INFO, msg).await;
                self.documents
                    .lock()
                    .expect("documents lock")
                    .insert(uri, doc);
            }
            Err(err) => {
                let msg = format!("tes-lsp: edit_read failed for {}: {err}", path.display());
                eprintln!("{msg}");
                self.client.log_message(MessageType::ERROR, msg).await;
            }
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let (level, msg) = {
            let mut docs = self.documents.lock().expect("documents lock");
            match docs.get_mut(&uri) {
                None => (
                    MessageType::WARNING,
                    format!("tes-lsp: didChange for unknown URI: {uri}"),
                ),
                Some(doc) => {
                    match apply_content_changes(&mut doc.tessprek, &params.content_changes) {
                        Ok(()) => (
                            MessageType::INFO,
                            format!(
                                "tes-lsp: changed {} tessprek-bytes={} source-hash={} (unchanged)",
                                doc.path.display(),
                                doc.tessprek.len(),
                                &doc.source_hash[..doc.source_hash.len().min(12)]
                            ),
                        ),
                        Err(err) => (
                            MessageType::ERROR,
                            format!(
                                "tes-lsp: didChange apply failed for {}: {err}",
                                doc.path.display()
                            ),
                        ),
                    }
                }
            }
        };
        eprintln!("{msg}");
        self.client.log_message(level, msg).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        let removed = self.documents.lock().expect("documents lock").remove(&uri);
        if let Some(doc) = removed {
            let msg = format!("tes-lsp: closed {}", doc.path.display());
            eprintln!("{msg}");
            self.client.log_message(MessageType::INFO, msg).await;
        }
    }
}

/// Run `tes-lsp` over stdio. Logs must not go to stdout (LSP framing).
pub async fn run() {
    eprintln!("tes-lsp: listening on stdio");

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

fn uri_to_path(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok()
}

fn is_tes_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("tes"))
}

fn load_open_document(path: PathBuf) -> std::result::Result<OpenDocument, String> {
    let report = edit_read(&path).map_err(|e| e.to_string())?;
    Ok(OpenDocument {
        path,
        source_hash: report.source_hash,
        tessprek: report.tessprek,
    })
}

/// Apply LSP content changes to `text` (full replace and/or incremental).
fn apply_content_changes(
    text: &mut String,
    changes: &[TextDocumentContentChangeEvent],
) -> std::result::Result<(), String> {
    for change in changes {
        match change.range {
            None => {
                *text = change.text.clone();
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

/// LSP positions are UTF-16 code units; map to a UTF-8 byte offset.
fn position_to_utf8_offset(text: &str, pos: Position) -> Option<usize> {
    let mut line = 0u32;
    let mut utf16_col = 0u32;
    for (byte_idx, ch) in text.char_indices() {
        if line == pos.line && utf16_col == pos.character {
            return Some(byte_idx);
        }
        if ch == '\n' {
            if line == pos.line {
                // Past end of this line.
                return None;
            }
            line += 1;
            utf16_col = 0;
        } else {
            utf16_col += u32::try_from(ch.len_utf16()).ok()?;
        }
    }
    if line == pos.line && utf16_col == pos.character {
        return Some(text.len());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tower_lsp::lsp_types::Range;

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
        // Replace "🙂" (UTF-16 len 2) at line 0, chars 2..4 with "X"
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
