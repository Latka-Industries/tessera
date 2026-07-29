//! `tes-lsp` — Tessprek language server (stdio).
//!
//! Thin entry: protocol loop lives in [`tessera_doc::lsp`].

#[tokio::main]
async fn main() {
    tessera_doc::lsp::run().await;
}
