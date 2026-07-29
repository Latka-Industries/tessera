//! Tessprek language server (`tes-lsp`).
//!
//! Thin LSP over stdio. Opens `.tes` files as Tessprek via [`edit_read`]
//! (THI-242), keeps an in-memory Tessprek buffer via `didChange` (THI-243),
//! publishes verify / source-hash diagnostics (THI-244), writes back via
//! `tessera.write` / `willSave` using [`edit_write`] (THI-245), and hovers
//! Tessprek markers (THI-246).

mod diagnostics;
mod document;
mod hover;
mod write;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use serde_json::Value;
use tower_lsp::jsonrpc::{Error, ErrorCode, Result};
use tower_lsp::lsp_types::{
    DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, ExecuteCommandOptions, ExecuteCommandParams, Hover, HoverParams,
    HoverProviderCapability, InitializeParams, InitializeResult, InitializedParams, MessageType,
    ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, TextDocumentSyncSaveOptions, Url, WillSaveTextDocumentParams,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};

use self::diagnostics::{collect_diagnostics, file_diagnostic};
use self::document::{
    OpenDocument, apply_content_changes, is_tes_path, load_open_document, uri_to_path,
};
use self::hover::hover_at;
use self::write::{WriteBackError, parse_write_uri, write_back_document};

pub use self::write::COMMAND_WRITE;

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

    async fn publish_for_path(&self, uri: &Url, path: &Path, expected_hash: Option<&str>) {
        let path = path.to_path_buf();
        let expected = expected_hash.map(str::to_owned);
        let diagnostics =
            tokio::task::spawn_blocking(move || collect_diagnostics(&path, expected.as_deref()))
                .await
                .unwrap_or_else(|e| {
                    vec![file_diagnostic(
                        DiagnosticSeverity::ERROR,
                        "tes-lsp.join",
                        format!("diagnostics join error: {e}"),
                    )]
                });
        let n = diagnostics.len();
        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
        let msg = format!("tes-lsp: diagnostics {n} for {uri}");
        eprintln!("{msg}");
        self.client.log_message(MessageType::INFO, msg).await;
    }

    /// Write open document `uri` via [`edit_write`]; refresh hash or publish conflict.
    async fn write_document_uri(&self, uri: &Url) -> Result<Value> {
        let snapshot = {
            let docs = self.documents.lock().expect("documents lock");
            docs.get(uri).cloned()
        };
        let Some(mut doc) = snapshot else {
            return Err(Error {
                code: ErrorCode::InvalidParams,
                message: format!("tessera.write: document not open: {uri}").into(),
                data: None,
            });
        };

        let path = doc.path.clone();
        let write_result = tokio::task::spawn_blocking(move || write_back_document(&mut doc))
            .await
            .map_err(|e| Error {
                code: ErrorCode::InternalError,
                message: format!("tessera.write join error: {e}").into(),
                data: None,
            })?;

        match write_result {
            Ok(new_hash) => {
                {
                    let mut docs = self.documents.lock().expect("documents lock");
                    if let Some(open) = docs.get_mut(uri) {
                        open.source_hash.clone_from(&new_hash);
                    }
                }
                let msg = format!(
                    "tes-lsp: wrote {} source-hash={}",
                    path.display(),
                    &new_hash[..new_hash.len().min(12)]
                );
                eprintln!("{msg}");
                self.client.log_message(MessageType::INFO, msg).await;
                self.publish_for_path(uri, &path, Some(&new_hash)).await;
                Ok(serde_json::json!({
                    "ok": true,
                    "path": path.to_string_lossy(),
                    "source_hash": new_hash,
                }))
            }
            Err(WriteBackError::HashMismatch { expected, found }) => {
                let diag = file_diagnostic(
                    DiagnosticSeverity::ERROR,
                    "source-hash",
                    format!(
                        "source-hash mismatch: expected {}, found {} (refusing silent overwrite)",
                        &expected[..expected.len().min(12)],
                        &found[..found.len().min(12)]
                    ),
                );
                self.client
                    .publish_diagnostics(uri.clone(), vec![diag], None)
                    .await;
                let msg = format!(
                    "tes-lsp: write refused (source-hash) for {}",
                    path.display()
                );
                eprintln!("{msg}");
                self.client.log_message(MessageType::ERROR, msg).await;
                Ok(serde_json::json!({
                    "ok": false,
                    "code": "source-hash",
                    "expected": expected,
                    "found": found,
                }))
            }
            Err(WriteBackError::Other(err)) => {
                let diag = file_diagnostic(DiagnosticSeverity::ERROR, "edit-write", err.clone());
                self.client
                    .publish_diagnostics(uri.clone(), vec![diag], None)
                    .await;
                let msg = format!("tes-lsp: write failed for {}: {err}", path.display());
                eprintln!("{msg}");
                self.client.log_message(MessageType::ERROR, msg).await;
                Ok(serde_json::json!({
                    "ok": false,
                    "code": "edit-write",
                    "error": err,
                }))
            }
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
                        change: Some(TextDocumentSyncKind::FULL),
                        will_save: Some(true),
                        save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                        ..Default::default()
                    },
                )),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec![write::COMMAND_WRITE.to_owned()],
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
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
                let hash = doc.source_hash.clone();
                let path = doc.path.clone();
                self.documents
                    .lock()
                    .expect("documents lock")
                    .insert(uri.clone(), doc);
                self.publish_for_path(&uri, &path, Some(&hash)).await;
            }
            Err(err) => {
                let msg = format!("tes-lsp: edit_read failed for {}: {err}", path.display());
                eprintln!("{msg}");
                self.client.log_message(MessageType::ERROR, msg).await;
                self.client
                    .publish_diagnostics(
                        uri,
                        vec![file_diagnostic(DiagnosticSeverity::ERROR, "edit-read", err)],
                        None,
                    )
                    .await;
            }
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let outcome = {
            let mut docs = self.documents.lock().expect("documents lock");
            match docs.get_mut(&uri) {
                None => None,
                Some(doc) => {
                    let log = match apply_content_changes(
                        &mut doc.tessprek,
                        &params.content_changes,
                    ) {
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
                    };
                    Some((log, doc.path.clone(), doc.source_hash.clone()))
                }
            }
        };

        match outcome {
            None => {
                let msg = format!("tes-lsp: didChange for unknown URI: {uri}");
                eprintln!("{msg}");
                self.client.log_message(MessageType::WARNING, msg).await;
            }
            Some(((level, msg), path, hash)) => {
                eprintln!("{msg}");
                self.client.log_message(level, msg).await;
                self.publish_for_path(&uri, &path, Some(&hash)).await;
            }
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        let removed = self.documents.lock().expect("documents lock").remove(&uri);
        if let Some(doc) = removed {
            let msg = format!("tes-lsp: closed {}", doc.path.display());
            eprintln!("{msg}");
            self.client.log_message(MessageType::INFO, msg).await;
        }
        self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }

    async fn will_save(&self, params: WillSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        match self.write_document_uri(&uri).await {
            Ok(value) => {
                let msg = format!("tes-lsp: willSave write result for {uri}: {value}");
                eprintln!("{msg}");
            }
            Err(err) => {
                let msg = format!("tes-lsp: willSave write error for {uri}: {err}");
                eprintln!("{msg}");
                self.client.log_message(MessageType::ERROR, msg).await;
            }
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let tessprek = {
            let docs = self.documents.lock().expect("documents lock");
            docs.get(&uri).map(|d| d.tessprek.clone())
        };
        let Some(text) = tessprek else {
            return Ok(None);
        };
        Ok(hover_at(&text, position))
    }

    async fn execute_command(&self, params: ExecuteCommandParams) -> Result<Option<Value>> {
        if params.command != write::COMMAND_WRITE {
            return Err(Error {
                code: ErrorCode::MethodNotFound,
                message: format!("unknown command: {}", params.command).into(),
                data: None,
            });
        }
        let uri = parse_write_uri(&params)?;
        let value = self.write_document_uri(&uri).await?;
        Ok(Some(value))
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
