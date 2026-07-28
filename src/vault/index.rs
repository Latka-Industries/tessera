//! Optional `vault.tes` catalog index (`doc_kind = index`).
//!
//! A sealed TOC-style sidecar listing vault documents by id/title/tags without
//! opening every note for list/search. Link-graph commands still scan real files.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::catalog::{DocumentCatalog, TesFile, TesWriterSession, TextHeader, decode_text_payload};
use crate::error::{Result, TesError};
use crate::layout::DocKind;

use super::graph::collect_tes_paths;

/// Conventional filename for the vault catalog sidecar.
pub const VAULT_INDEX_NAME: &str = "vault.tes";

const INDEX_FORMAT: &str = "tessera.vault_index";
const INDEX_VERSION: u32 = 1;
/// Stable id for every `vault.tes` (one per directory).
const INDEX_DOC_ID: &str = "550e8400-e29b-41d4-a716-446655440099";

/// One row in the vault TOC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultIndexEntry {
    /// Stable document UUID.
    pub doc_id: String,
    /// Display title.
    pub title: String,
    /// Document kind string.
    pub doc_kind: String,
    /// Catalog tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Catalog `modified` (RFC 3339).
    pub modified: String,
    /// Path relative to the vault root (forward slashes).
    pub path: String,
    /// File mtime as Unix seconds when the index was built.
    pub mtime_secs: u64,
}

/// Parsed `vault.tes` TOC payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultIndex {
    /// Wire marker (`tessera.vault_index`).
    pub format: String,
    /// Payload version.
    pub version: u32,
    /// Document rows (excludes the index file itself).
    pub entries: Vec<VaultIndexEntry>,
}

/// Result of listing a vault via index or full catalog scan.
#[derive(Debug, Clone, Serialize)]
pub struct VaultListReport {
    /// Rows shown to the user.
    pub entries: Vec<VaultIndexEntry>,
    /// Whether `vault.tes` was used.
    pub used_index: bool,
    /// True when an on-disk index existed but was stale/unusable.
    pub index_stale: bool,
}

impl VaultIndex {
    fn new(entries: Vec<VaultIndexEntry>) -> Self {
        Self {
            format: INDEX_FORMAT.into(),
            version: INDEX_VERSION,
            entries,
        }
    }

    fn from_json(bytes: &[u8]) -> Result<Self> {
        let index: Self = serde_json::from_slice(bytes)?;
        if index.format != INDEX_FORMAT {
            return Err(TesError::InvalidTextHeader {
                message: format!(
                    "vault index format '{}', expected '{INDEX_FORMAT}'",
                    index.format
                ),
            });
        }
        if index.version != INDEX_VERSION {
            return Err(TesError::UnsupportedVersion {
                structure: "vault.tes index",
                found: index.version,
                supported: INDEX_VERSION,
            });
        }
        Ok(index)
    }
}

/// Absolute path to `root/vault.tes`.
#[must_use]
pub fn vault_index_path(root: impl AsRef<Path>) -> PathBuf {
    root.as_ref().join(VAULT_INDEX_NAME)
}

/// Rebuild `vault.tes` under `root` from a fresh catalog scan.
///
/// # Errors
///
/// Returns IO / open / encode errors while scanning or writing the index.
pub fn rebuild_vault_index(root: impl AsRef<Path>) -> Result<PathBuf> {
    let root = root.as_ref();
    let index = VaultIndex::new(scan_catalog_entries(root)?);
    let body = serde_json::to_string_pretty(&index)?;
    let path = vault_index_path(root);
    let _ = fs::remove_file(&path);

    let now = rfc3339_now();
    let mut session = TesWriterSession::create(&path, DocKind::Index);
    let mut catalog = DocumentCatalog::new(
        INDEX_DOC_ID,
        "Vault index",
        now.clone(),
        now,
        DocKind::Index,
    );
    catalog.tags = vec!["vault-index".into()];
    session.set_catalog(catalog)?;
    session.add_text_chunk(&TextHeader::code_block(Some("json")), &body)?;
    session.commit()?;
    Ok(path)
}

/// Load `vault.tes` if present.
///
/// # Errors
///
/// Returns open/decode errors when the file exists but is not a valid index.
pub fn load_vault_index(root: impl AsRef<Path>) -> Result<Option<VaultIndex>> {
    let path = vault_index_path(&root);
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(read_vault_index_file(&path)?))
}

/// Whether every indexed path still exists with the recorded mtime and no
/// extra `.tes` documents appeared.
///
/// # Errors
///
/// Returns IO errors while listing the vault.
pub fn vault_index_is_fresh(root: impl AsRef<Path>, index: &VaultIndex) -> Result<bool> {
    let root = root.as_ref();
    let paths = document_tes_paths(root)?;
    if paths.len() != index.entries.len() {
        return Ok(false);
    }

    let actual = path_signatures(root, &paths)?;
    let mut expected: Vec<(String, u64)> = index
        .entries
        .iter()
        .map(|e| (e.path.clone(), e.mtime_secs))
        .collect();
    expected.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(expected == actual)
}

/// List vault documents using `vault.tes` when fresh; otherwise scan catalogs.
///
/// When `force_scan` is true, always scan catalogs and ignore any index.
///
/// # Errors
///
/// Returns IO / open errors from the index or catalog scan.
pub fn list_vault_documents(
    root: impl AsRef<Path>,
    tag: Option<&str>,
    force_scan: bool,
) -> Result<VaultListReport> {
    let root = root.as_ref();
    let (mut entries, used_index, index_stale) = if force_scan {
        (scan_catalog_entries(root)?, false, false)
    } else {
        match load_vault_index(root)? {
            Some(index) if vault_index_is_fresh(root, &index)? => (index.entries, true, false),
            Some(_) => (scan_catalog_entries(root)?, false, true),
            None => (scan_catalog_entries(root)?, false, false),
        }
    };

    if let Some(tag) = tag {
        entries.retain(|e| e.tags.iter().any(|t| t == tag));
    }
    entries.sort_by(|a, b| a.title.cmp(&b.title).then(a.doc_id.cmp(&b.doc_id)));

    Ok(VaultListReport {
        entries,
        used_index,
        index_stale,
    })
}

fn read_vault_index_file(path: &Path) -> Result<VaultIndex> {
    let file = TesFile::open(path)?;
    if file.superblock().doc_kind != DocKind::Index {
        return Err(TesError::InvalidTextHeader {
            message: format!(
                "{}: expected doc_kind=index, got {}",
                path.display(),
                file.superblock().doc_kind.as_str()
            ),
        });
    }
    let entry = file
        .chunks()
        .iter()
        .find(|e| e.chunk_type == crate::catalog::ChunkType::Text)
        .ok_or_else(|| TesError::InvalidTextHeader {
            message: format!("{}: missing index JSON chunk", path.display()),
        })?;
    let raw = file.decode_payload(entry)?;
    let (_header, body) = decode_text_payload(&raw)?;
    VaultIndex::from_json(body.as_bytes())
}

fn scan_catalog_entries(root: &Path) -> Result<Vec<VaultIndexEntry>> {
    let paths = document_tes_paths(root)?;
    let mut entries = Vec::with_capacity(paths.len());
    for path in paths {
        let file = TesFile::open(&path)?;
        let Some(catalog) = file.catalog() else {
            continue;
        };
        if catalog.doc_kind == DocKind::Index.as_str() {
            continue;
        }
        let _id = Uuid::parse_str(&catalog.doc_id).map_err(|_| TesError::InvalidDocId {
            value: catalog.doc_id.clone(),
        })?;
        entries.push(VaultIndexEntry {
            doc_id: catalog.doc_id.clone(),
            title: catalog.title.clone(),
            doc_kind: catalog.doc_kind.clone(),
            tags: catalog.tags.clone(),
            modified: catalog.modified.clone(),
            path: relative_vault_path(root, &path)?,
            mtime_secs: file_mtime_secs(&path)?,
        });
    }
    Ok(entries)
}

/// Sorted `.tes` paths under `root`, excluding the root `vault.tes` sidecar.
fn document_tes_paths(root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    collect_tes_paths(root, &mut paths)?;
    paths.retain(|p| !is_vault_index_path(root, p));
    paths.sort();
    Ok(paths)
}

fn path_signatures(root: &Path, paths: &[PathBuf]) -> Result<Vec<(String, u64)>> {
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        out.push((relative_vault_path(root, path)?, file_mtime_secs(path)?));
    }
    Ok(out)
}

fn is_vault_index_path(root: &Path, path: &Path) -> bool {
    path.file_name().and_then(|s| s.to_str()) == Some(VAULT_INDEX_NAME)
        && path.parent().is_some_and(|p| p == root)
}

fn relative_vault_path(root: &Path, path: &Path) -> Result<String> {
    let rel = path.strip_prefix(root).map_err(|_| {
        TesError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "path {} is outside vault root {}",
                path.display(),
                root.display()
            ),
        ))
    })?;
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

fn file_mtime_secs(path: &Path) -> Result<u64> {
    let modified = fs::metadata(path)?.modified()?;
    Ok(modified
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0))
}

fn rfc3339_now() -> String {
    use time::OffsetDateTime;
    let now = OffsetDateTime::now_utc();
    now.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{DocumentCatalog, TesWriterSession, TextHeader};
    use crate::layout::DocKind;
    use tempfile::tempdir;

    fn write_note(dir: &Path, name: &str, title: &str, tags: &[&str]) {
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

    #[test]
    fn rebuild_and_list_uses_index() {
        let dir = tempdir().unwrap();
        write_note(dir.path(), "a.tes", "Alpha", &["x"]);
        write_note(dir.path(), "b.tes", "Beta", &["y"]);

        let path = rebuild_vault_index(dir.path()).unwrap();
        assert!(path.ends_with(VAULT_INDEX_NAME));

        let report = list_vault_documents(dir.path(), None, false).unwrap();
        assert!(report.used_index);
        assert!(!report.index_stale);
        assert_eq!(report.entries.len(), 2);
        assert_eq!(report.entries[0].title, "Alpha");

        let tagged = list_vault_documents(dir.path(), Some("y"), false).unwrap();
        assert_eq!(tagged.entries.len(), 1);
        assert_eq!(tagged.entries[0].title, "Beta");
    }

    #[test]
    fn stale_index_falls_back_to_scan() {
        let dir = tempdir().unwrap();
        write_note(dir.path(), "a.tes", "Alpha", &[]);
        rebuild_vault_index(dir.path()).unwrap();
        write_note(dir.path(), "b.tes", "Beta", &[]);

        let report = list_vault_documents(dir.path(), None, false).unwrap();
        assert!(!report.used_index);
        assert!(report.index_stale);
        assert_eq!(report.entries.len(), 2);
    }
}
