//! Multi-file vault graph: stable document ids, resolve, backlinks, and checks.
//!
//! - [`Vault`] — scan a folder of `.tes` files and query the link graph.
//! - [`parse_target`] — parse `UUID` or `UUID/chunk` CLI/link targets.
//! - Value types: [`VaultDocument`], [`Backlink`], [`ResolvedTarget`], [`BrokenLink`].

mod graph;
mod types;

pub use graph::Vault;
pub use types::{Backlink, BrokenLink, ResolvedTarget, VaultDocument};

use uuid::Uuid;

use crate::error::{Result, TesError};

/// Parse `UUID` or `UUID/chunk`.
///
/// # Errors
///
/// Returns [`TesError::InvalidDocId`] if the UUID is malformed, or [`TesError::Io`]
/// if the optional chunk id is not a valid `u64`.
pub fn parse_target(value: &str) -> Result<(Uuid, Option<u64>)> {
    let (doc, chunk) = match value.split_once('/') {
        Some((doc, chunk)) => (doc, Some(chunk)),
        None => (value, None),
    };
    let doc_id = Uuid::parse_str(doc).map_err(|_| TesError::InvalidDocId {
        value: doc.to_owned(),
    })?;
    let chunk_id = chunk.map(str::parse::<u64>).transpose().map_err(|_| {
        TesError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("invalid target chunk in '{value}'"),
        ))
    })?;
    Ok((doc_id, chunk_id))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::catalog::{
        DocumentCatalog, LinkEntry, LinkKind, TesWriterSession, TextHeader, TextRole,
    };
    use crate::layout::DocKind;
    use tempfile::tempdir;
    use uuid::Uuid;

    const A: &str = "11111111-1111-4111-8111-111111111111";
    const B: &str = "22222222-2222-4222-8222-222222222222";

    fn write_doc(path: &Path, id: &str, title: &str, target: Option<Uuid>) {
        let mut session = TesWriterSession::create(path, DocKind::Note);
        session
            .set_catalog(DocumentCatalog::new(
                id,
                title,
                "2026-06-05T12:00:00Z",
                "2026-06-05T12:00:00Z",
                DocKind::Note,
            ))
            .unwrap();
        session
            .add_text_chunk(&TextHeader::paragraph(), title)
            .unwrap();
        if let Some(target) = target {
            session
                .add_link(LinkEntry::new(
                    1,
                    0,
                    title.len() as u32,
                    target,
                    1,
                    LinkKind::Wiki,
                ))
                .unwrap();
        }
        session.commit().unwrap();
    }

    #[test]
    fn resolves_and_finds_backlinks() {
        let dir = tempdir().unwrap();
        write_doc(
            &dir.path().join("a.tes"),
            A,
            "Source",
            Some(Uuid::parse_str(B).unwrap()),
        );
        write_doc(&dir.path().join("b.tes"), B, "Target", None);
        let vault = Vault::open(dir.path()).unwrap();
        assert_eq!(vault.documents().count(), 2);
        let target = vault.resolve(Uuid::parse_str(B).unwrap(), Some(1)).unwrap();
        assert_eq!(target.document.title, "Target");
        assert_eq!(target.text.as_deref(), Some("Target"));
        assert_eq!(target.role, Some(TextRole::Paragraph.as_str()));
        let backlinks = vault.backlinks(Uuid::parse_str(B).unwrap());
        assert_eq!(backlinks.len(), 1);
        assert_eq!(backlinks[0].source_title, "Source");
        assert!(vault.check().unwrap().is_empty());
    }

    #[test]
    fn reports_missing_target() {
        let dir = tempdir().unwrap();
        write_doc(
            &dir.path().join("a.tes"),
            A,
            "Source",
            Some(Uuid::parse_str(B).unwrap()),
        );
        let broken = Vault::open(dir.path()).unwrap().check().unwrap();
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0].message, "target document is missing");
    }

    #[test]
    fn parses_target_syntax() {
        let (doc, chunk) = parse_target(&format!("{B}/12")).unwrap();
        assert_eq!(doc, Uuid::parse_str(B).unwrap());
        assert_eq!(chunk, Some(12));
    }
}
