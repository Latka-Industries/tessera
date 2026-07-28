//! [`Vault`]: indexed folder of `.tes` files and graph queries.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::catalog::{TesFile, decode_text_payload};
use crate::error::{Result, TesError};

use super::{Backlink, BrokenLink, ResolvedTarget, VaultDocument};

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
                let Some(target_uuid) = link.target_uuid() else {
                    // External / attachment edges are not vault graph nodes.
                    continue;
                };
                backlinks.push(Backlink {
                    source_doc_id: id.to_string(),
                    source_title: catalog.title.clone(),
                    source_path: path.clone(),
                    source_chunk_id: link.source_chunk_id,
                    target_doc_id: target_uuid.to_string(),
                    target_chunk_id: link.target_chunk_id().unwrap_or(0),
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
