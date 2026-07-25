//! Multi-file vault graph: stable document ids, resolve, backlinks, and checks.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use uuid::Uuid;

use crate::catalog::{TesFile, decode_text_payload};
use crate::error::{Result, TesError};

/// One document known to a [`Vault`].
#[derive(Debug, Clone, Serialize)]
pub struct VaultDocument {
    /// Stable UUID.
    pub doc_id: String,
    /// Display title.
    pub title: String,
    /// Document kind string.
    pub doc_kind: String,
    /// File path.
    pub path: PathBuf,
    /// Number of indexed chunks.
    pub chunk_count: usize,
}

/// One inbound graph edge.
#[derive(Debug, Clone, Serialize)]
pub struct Backlink {
    /// Source document UUID.
    pub source_doc_id: String,
    /// Source title.
    pub source_title: String,
    /// Source file.
    pub source_path: PathBuf,
    /// Source chunk containing the anchor.
    pub source_chunk_id: u64,
    /// Target document UUID.
    pub target_doc_id: String,
    /// Target chunk (`0` = whole document).
    pub target_chunk_id: u64,
    /// Edge semantics.
    pub link_kind: &'static str,
}

/// Result of resolving `UUID[/chunk]`.
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedTarget {
    /// Target document.
    pub document: VaultDocument,
    /// Requested chunk, when any.
    pub chunk_id: Option<u64>,
    /// Text body for a text chunk.
    pub text: Option<String>,
    /// Semantic role for a text chunk.
    pub role: Option<&'static str>,
}

/// Broken graph edge found by [`Vault::check`].
#[derive(Debug, Clone, Serialize)]
pub struct BrokenLink {
    /// Source document.
    pub source_doc_id: String,
    /// Source chunk.
    pub source_chunk_id: u64,
    /// Missing target document.
    pub target_doc_id: String,
    /// Target chunk (`0` = whole document).
    pub target_chunk_id: u64,
    /// Why resolution failed.
    pub message: String,
}

/// An indexed folder of `.tes` files.
pub struct Vault {
    root: PathBuf,
    documents: BTreeMap<Uuid, VaultDocument>,
    backlinks: Vec<Backlink>,
}

impl Vault {
    /// Recursively scan `root` for `.tes` files and build the graph index.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::Io`] while scanning, open/parse errors from [`TesFile::open`],
    /// [`TesError::InvalidDocId`] for a bad catalog UUID, or [`TesError::DuplicateDocId`]
    /// when two files share a document id.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let mut paths = Vec::new();
        collect_tes_paths(&root, &mut paths)?;
        paths.sort();

        let mut documents = BTreeMap::new();
        let mut backlinks = Vec::new();
        for path in paths {
            let file = TesFile::open(&path)?;
            let Some(catalog) = file.catalog() else {
                continue;
            };
            let id = Uuid::parse_str(&catalog.doc_id).map_err(|_| TesError::InvalidDocId {
                value: catalog.doc_id.clone(),
            })?;
            let document = VaultDocument {
                doc_id: id.to_string(),
                title: catalog.title.clone(),
                doc_kind: catalog.doc_kind.clone(),
                path: path.clone(),
                chunk_count: file.chunks().len(),
            };
            if let Some(first) = documents.insert(id, document.clone()) {
                return Err(TesError::DuplicateDocId {
                    doc_id: id.to_string(),
                    first: first.path.display().to_string(),
                    second: path.display().to_string(),
                });
            }

            for link in file.links() {
                backlinks.push(Backlink {
                    source_doc_id: id.to_string(),
                    source_title: catalog.title.clone(),
                    source_path: path.clone(),
                    source_chunk_id: link.source_chunk_id,
                    target_doc_id: link.target_uuid().to_string(),
                    target_chunk_id: link.target_chunk_id,
                    link_kind: link.link_kind.as_str(),
                });
            }
        }
        backlinks.sort_by(|a, b| {
            (&a.target_doc_id, &a.source_doc_id, a.source_chunk_id).cmp(&(
                &b.target_doc_id,
                &b.source_doc_id,
                b.source_chunk_id,
            ))
        });

        Ok(Self {
            root,
            documents,
            backlinks,
        })
    }

    /// Vault root directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Documents sorted by UUID.
    pub fn documents(&self) -> impl Iterator<Item = &VaultDocument> {
        self.documents.values()
    }

    /// Resolve a document and optional chunk.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::DocumentNotFound`] if `doc_id` is unknown, or open/decode
    /// errors when resolving a chunk.
    pub fn resolve(&self, doc_id: Uuid, chunk_id: Option<u64>) -> Result<ResolvedTarget> {
        let document = self
            .documents
            .get(&doc_id)
            .ok_or_else(|| TesError::DocumentNotFound {
                doc_id: doc_id.to_string(),
            })?
            .clone();

        let (text, role) = if let Some(chunk_id) = chunk_id {
            let file = TesFile::open(&document.path)?;
            let entry = file.chunk_by_id(chunk_id)?;
            if entry.chunk_type == crate::catalog::ChunkType::Text {
                let raw = file.decode_payload(entry)?;
                let (header, body) = decode_text_payload(&raw)?;
                (Some(body), Some(header.role.as_str()))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        Ok(ResolvedTarget {
            document,
            chunk_id,
            text,
            role,
        })
    }

    /// Inbound edges targeting a document.
    #[must_use]
    pub fn backlinks(&self, doc_id: Uuid) -> Vec<&Backlink> {
        let needle = doc_id.to_string();
        self.backlinks
            .iter()
            .filter(|link| link.target_doc_id == needle)
            .collect()
    }

    /// Return all missing document/chunk targets.
    ///
    /// # Errors
    ///
    /// Returns open errors from [`TesFile::open`] when validating chunk targets.
    ///
    /// # Panics
    ///
    /// Never in practice: backlink `target_doc_id` values originate from
    /// parsed link tables and are always valid UUID strings.
    pub fn check(&self) -> Result<Vec<BrokenLink>> {
        let mut broken = Vec::new();
        for link in &self.backlinks {
            let target_id = Uuid::parse_str(&link.target_doc_id).expect("wire UUID is valid");
            let Some(target) = self.documents.get(&target_id) else {
                broken.push(BrokenLink {
                    source_doc_id: link.source_doc_id.clone(),
                    source_chunk_id: link.source_chunk_id,
                    target_doc_id: link.target_doc_id.clone(),
                    target_chunk_id: link.target_chunk_id,
                    message: "target document is missing".to_owned(),
                });
                continue;
            };
            if link.target_chunk_id != 0 {
                let file = TesFile::open(&target.path)?;
                if file.chunk_by_id(link.target_chunk_id).is_err() {
                    broken.push(BrokenLink {
                        source_doc_id: link.source_doc_id.clone(),
                        source_chunk_id: link.source_chunk_id,
                        target_doc_id: link.target_doc_id.clone(),
                        target_chunk_id: link.target_chunk_id,
                        message: "target chunk is missing".to_owned(),
                    });
                }
            }
        }
        Ok(broken)
    }
}

fn collect_tes_paths(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_tes_paths(&path, out)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("tes") {
            out.push(path);
        }
    }
    Ok(())
}

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
    use super::*;
    use crate::catalog::{
        DocumentCatalog, LinkEntry, LinkKind, TesWriterSession, TextHeader, TextRole,
    };
    use crate::layout::DocKind;
    use tempfile::tempdir;

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
