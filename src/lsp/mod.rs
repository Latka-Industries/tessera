//! Tessprek language server (`tes-lsp`).
//!
//! Thin LSP over stdio. Document sync, diagnostics, and write-back land in
//! later THI-231 children; this module only completes the handshake.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    InitializeParams, InitializeResult, InitializedParams, MessageType, ServerCapabilities,
    ServerInfo,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};

/// LSP backend for Tessprek ↔ `.tes` (scaffold: handshake only).
#[derive(Debug)]
struct Backend {
    client: Client,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities::default(),
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
}

/// Run `tes-lsp` over stdio. Logs must not go to stdout (LSP framing).
pub async fn run() {
    // Keep any incidental diagnostics off the LSP stdout channel.
    eprintln!("tes-lsp: listening on stdio");

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend { client });
    Server::new(stdin, stdout, socket).serve(service).await;
}
