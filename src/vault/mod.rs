//! Multi-file vault graph: stable document ids, resolve, backlinks, and checks.
//!
//! - [`Vault`] — scan membership (in-tree ∪ registered externals) and query the link graph.
//! - [`parse_target`] — parse `UUID` or `UUID/chunk` CLI/link targets.
//! - Optional [`vault.tes`](index) TOC index for list without a full graph scan.
//! - Vault search: parallel scan or Tantivy under [`.tessera/fts`](search).
//! - Membership: [`register_member`] / [`unregister_member`] for out-of-tree paths.
//! - Markdown vault import: [`import_markdown_vault`].
//! - Value types: [`VaultDocument`], [`Backlink`], [`ResolvedTarget`], [`BrokenLink`].

mod graph;
mod import;
mod index;
mod members;
mod search;
mod types;

pub use graph::Vault;
pub use import::{
    VaultMarkdownImportEntry, VaultMarkdownImportOptions, VaultMarkdownImportReport,
    import_markdown_vault,
};
pub use index::{
    VAULT_INDEX_NAME, VaultIndex, VaultIndexEntry, VaultListReport, list_vault_documents,
    list_vault_documents_filtered, load_vault_index, rebuild_vault_index, vault_index_is_fresh,
    vault_index_path,
};
pub use members::{
    VaultMember, VaultMemberKind, load_registered_members, membership_document_paths,
    register_member, unregister_member,
};
pub use search::{
    AUTO_INDEX_DOC_THRESHOLD, VAULT_DOT_DIR, VAULT_FTS_DIRNAME, VaultSearchHit, VaultSearchMode,
    VaultSearchOptions, VaultSearchReport, rebuild_vault_fts, search_vault, vault_fts_is_fresh,
    vault_fts_path,
};
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
pub(super) mod test_util {
    use std::path::Path;

    use crate::catalog::{DocumentCatalog, TesWriterSession, TextHeader};
    use crate::layout::DocKind;
    use uuid::Uuid;

    /// Write a minimal note under `dir/name` for vault unit tests.
    pub fn write_note(dir: &Path, name: &str, title: &str, tags: &[&str]) {
        let path = dir.join(name);
        let mut session = TesWriterSession::create(&path, DocKind::Note);
        let mut catalog = DocumentCatalog::new(
            Uuid::new_v4().to_string(),
            title,
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Note,
        );
        catalog.tags = tags.iter().map(|s| (*s).to_owned()).collect();
        session.set_catalog(catalog).unwrap();
        session
            .add_text_chunk(&TextHeader::paragraph(), "body")
            .unwrap();
        session.commit().unwrap();
    }
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
