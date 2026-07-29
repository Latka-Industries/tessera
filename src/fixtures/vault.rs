//! Sample vault under `fixtures/vault/` (notes + optional `vault.tes` TOC).

use std::fs;
use std::path::Path;

use crate::catalog::{DocumentCatalog, TesWriterSession, TextHeader};
use crate::error::Result;
use crate::layout::DocKind;
use crate::vault::rebuild_vault_index;

fn write_note(
    dir: &Path,
    name: &str,
    doc_id: &str,
    title: &str,
    tags: &[&str],
    body: &str,
) -> Result<()> {
    let path = dir.join(name);
    let _ = fs::remove_file(&path);
    let mut session = TesWriterSession::create(&path, DocKind::Note);
    let mut catalog = DocumentCatalog::new(
        doc_id,
        title,
        "2026-07-28T00:00:00Z",
        "2026-07-28T00:00:00Z",
        DocKind::Note,
    );
    catalog.tags = tags.iter().map(|s| (*s).to_owned()).collect();
    session.set_catalog(catalog)?;
    session.add_text_chunk(&TextHeader::paragraph(), body)?;
    session.commit()?;
    println!("wrote {}", path.display());
    Ok(())
}

/// Write a small tagged vault and seal `vault.tes`.
///
/// `vault.tes` embeds member mtimes, so re-run this generator after cloning if
/// `tes vault list` reports a stale index.
///
/// # Errors
///
/// Returns IO / writer / vault rebuild errors.
pub fn write_sample(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    write_note(
        dir,
        "alpha.tes",
        "550e8400-e29b-41d4-a716-446655440080",
        "Alpha note",
        &["notes", "demo"],
        "First vault member — listed via vault.tes when fresh.",
    )?;
    write_note(
        dir,
        "beta.tes",
        "550e8400-e29b-41d4-a716-446655440081",
        "Beta research",
        &["research", "citations"],
        "Second vault member tagged for filtered list demos.",
    )?;
    write_note(
        dir,
        "gamma.tes",
        "550e8400-e29b-41d4-a716-446655440082",
        "Gamma media",
        &["media", "figures"],
        "Third vault member covering media-oriented tags.",
    )?;
    let index = rebuild_vault_index(dir)?;
    println!("wrote {}", index.display());
    Ok(())
}
