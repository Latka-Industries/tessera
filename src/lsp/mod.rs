//! Tessprek language server (`tes-lsp`).
//!
//! Thin LSP over stdio. Opens `.tes` files as Tessprek via [`edit_read`]
//! (THI-242), keeps an in-memory Tessprek buffer via `didChange` (THI-243),
//! and publishes verify / source-hash diagnostics (THI-244). Write-back is next.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, InitializeParams, InitializeResult, InitializedParams, MessageType,
    NumberOrString, Position, Range, ServerCapabilities, ServerInfo,
    TextDocumentContentChangeEvent, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, Url,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::edit::{edit_read, file_source_hash};
use crate::verify::{Finding, Severity, verify_tes_file};

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
                // Re-check on-disk verify + hash (external edits / corruption).
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

/// File-level range — v1 does not map every verify finding onto Tessprek spans.
fn file_level_range() -> Range {
    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line: 0,
            character: 1,
        },
    }
}

fn file_diagnostic(
    severity: DiagnosticSeverity,
    code: &str,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic {
        range: file_level_range(),
        severity: Some(severity),
        code: Some(NumberOrString::String(code.to_owned())),
        source: Some("tes-lsp".into()),
        message: message.into(),
        ..Default::default()
    }
}

fn finding_to_diagnostic(finding: &Finding) -> Diagnostic {
    let severity = match finding.severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Info => DiagnosticSeverity::INFORMATION,
    };
    file_diagnostic(severity, &finding.check, finding.message.clone())
}

/// Build diagnostics from `verify_*` plus an optional source-hash expectation.
fn collect_diagnostics(path: &Path, expected_hash: Option<&str>) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    if let Some(expected) = expected_hash {
        match file_source_hash(path) {
            Ok(found) if found != expected => {
                out.push(file_diagnostic(
                    DiagnosticSeverity::ERROR,
                    "source-hash",
                    format!(
                        "source-hash mismatch: expected {}, found {} (refusing silent overwrite)",
                        &expected[..expected.len().min(12)],
                        &found[..found.len().min(12)]
                    ),
                ));
            }
            Ok(_) => {}
            Err(err) => {
                out.push(file_diagnostic(
                    DiagnosticSeverity::ERROR,
                    "source-hash",
                    format!("source-hash check failed: {err}"),
                ));
            }
        }
    }

    match verify_tes_file(path, true) {
        Ok(report) => {
            out.extend(report.findings.iter().map(finding_to_diagnostic));
        }
        Err(err) => {
            out.push(file_diagnostic(
                DiagnosticSeverity::ERROR,
                "verify",
                format!("verify failed: {err}"),
            ));
        }
    }

    out
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

    fn reject_bad_magic() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/conformance/reject/bad_magic.tes")
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

    #[test]
    fn collect_diagnostics_clean_fixture_empty_or_ok() {
        let path = fixture_tes();
        let hash = file_source_hash(&path).unwrap();
        let diags = collect_diagnostics(&path, Some(&hash));
        assert!(
            diags
                .iter()
                .all(|d| d.severity != Some(DiagnosticSeverity::ERROR)),
            "unexpected errors: {diags:?}"
        );
    }

    #[test]
    fn collect_diagnostics_bad_magic_has_error() {
        let path = reject_bad_magic();
        let diags = collect_diagnostics(&path, None);
        assert!(
            diags.iter().any(|d| {
                d.severity == Some(DiagnosticSeverity::ERROR)
                    && matches!(&d.code, Some(NumberOrString::String(c)) if c.contains("magic") || c == "verify")
            }),
            "expected magic/verify error, got {diags:?}"
        );
    }

    #[test]
    fn collect_diagnostics_hash_mismatch() {
        let path = fixture_tes();
        let diags = collect_diagnostics(&path, Some("deadbeef"));
        assert!(
            diags.iter().any(|d| {
                d.severity == Some(DiagnosticSeverity::ERROR)
                    && d.code == Some(NumberOrString::String("source-hash".into()))
            }),
            "expected source-hash error, got {diags:?}"
        );
    }
}
