//! Optional `vault.tes` catalog index (`doc_kind = index`).
//!
//! A sealed TOC-style sidecar listing vault documents by id/title/tags without
//! opening every note for list/search. Version ≥ 2 also stores registered
//! external members (THI-217) so rebuild/list/`tes link` share one membership set.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::catalog::{DocumentCatalog, TesFile, TesWriterSession, TextHeader, decode_text_payload};
use crate::error::{Result, TesError};
use crate::layout::DocKind;

use super::members::{
    VaultMember, display_path, load_registered_members, membership_document_paths_with,
};

/// Conventional filename for the vault catalog sidecar.
pub const VAULT_INDEX_NAME: &str = "vault.tes";

const INDEX_FORMAT: &str = "tessera.vault_index";
/// Current on-disk index payload version (members field).
pub const INDEX_VERSION: u32 = 2;
/// Oldest readable index version (no `members`).
const INDEX_VERSION_MIN: u32 = 1;
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
    /// Path relative to the vault root when in-tree; otherwise absolute.
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
    /// Explicit external files / extra roots (version ≥ 2).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<VaultMember>,
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
    fn new(members: Vec<VaultMember>, entries: Vec<VaultIndexEntry>) -> Self {
        Self {
            format: INDEX_FORMAT.into(),
            version: INDEX_VERSION,
            members,
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
        if index.version < INDEX_VERSION_MIN || index.version > INDEX_VERSION {
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

/// Rebuild `vault.tes` under `root`, preserving any registered members.
///
/// # Errors
///
/// Returns IO / open / encode errors while scanning or writing the index.
pub fn rebuild_vault_index(root: impl AsRef<Path>) -> Result<PathBuf> {
    let root = root.as_ref();
    let members = load_registered_members(root)?;
    rebuild_vault_index_with_members(root, members)
}

/// Rebuild `vault.tes` with an explicit member registry.
///
/// # Errors
///
/// Returns IO / open / encode errors while scanning or writing the index.
pub fn rebuild_vault_index_with_members(
    root: impl AsRef<Path>,
    members: Vec<VaultMember>,
) -> Result<PathBuf> {
    let root = root.as_ref();
    let index = VaultIndex::new(members.clone(), scan_catalog_entries(root, &members)?);
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
    let paths = membership_document_paths_with(root, &index.members)?;
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
        (
            scan_catalog_entries(root, &load_registered_members(root)?)?,
            false,
            false,
        )
    } else {
        match load_vault_index(root)? {
            Some(index) if vault_index_is_fresh(root, &index)? => (index.entries, true, false),
            Some(index) => (scan_catalog_entries(root, &index.members)?, false, true),
            None => (scan_catalog_entries(root, &[])?, false, false),
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

fn scan_catalog_entries(root: &Path, members: &[VaultMember]) -> Result<Vec<VaultIndexEntry>> {
    let paths = membership_document_paths_with(root, members)?;
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
            path: display_path(root, &path),
            mtime_secs: file_mtime_secs(&path)?,
        });
    }
    Ok(entries)
}

fn path_signatures(root: &Path, paths: &[PathBuf]) -> Result<Vec<(String, u64)>> {
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        out.push((display_path(root, path), file_mtime_secs(path)?));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
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
    use crate::vault::test_util::write_note;
    use crate::vault::{register_member, unregister_member};
    use tempfile::tempdir;

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

    #[test]
    fn list_includes_registered_external() {
        let vault = tempdir().unwrap();
        let outside = tempdir().unwrap();
        write_note(vault.path(), "a.tes", "Alpha", &[]);
        write_note(outside.path(), "ext.tes", "External", &["x"]);
        register_member(vault.path(), outside.path().join("ext.tes")).unwrap();

        let report = list_vault_documents(vault.path(), None, false).unwrap();
        assert!(report.used_index);
        assert_eq!(report.entries.len(), 2);
        assert!(report.entries.iter().any(|e| e.title == "External"));

        unregister_member(vault.path(), outside.path().join("ext.tes")).unwrap();
        let after = list_vault_documents(vault.path(), None, false).unwrap();
        assert_eq!(after.entries.len(), 1);
    }

    #[test]
    fn v1_index_loads_empty_members_and_rebuild_upgrades_to_v2() {
        let dir = tempdir().unwrap();
        write_note(dir.path(), "a.tes", "Alpha", &[]);
        write_v1_index(dir.path());

        let loaded = load_vault_index(dir.path()).unwrap().expect("v1 index");
        assert_eq!(loaded.version, 1);
        assert!(loaded.members.is_empty());
        assert!(load_registered_members(dir.path()).unwrap().is_empty());
        assert_eq!(
            list_vault_documents(dir.path(), None, false)
                .unwrap()
                .entries
                .len(),
            1
        );

        rebuild_vault_index(dir.path()).unwrap();
        let upgraded = load_vault_index(dir.path()).unwrap().expect("v2 index");
        assert_eq!(upgraded.version, INDEX_VERSION);
        assert_eq!(upgraded.version, 2);
        assert!(upgraded.members.is_empty());
        assert_eq!(upgraded.entries.len(), 1);
    }

    fn write_v1_index(root: &Path) {
        let entries = scan_catalog_entries(root, &[]).unwrap();
        let body = serde_json::json!({
            "format": INDEX_FORMAT,
            "version": 1,
            "entries": entries,
        });
        let path = vault_index_path(root);
        let now = "2026-07-28T00:00:00Z".to_owned();
        let mut session = TesWriterSession::create(&path, DocKind::Index);
        let mut catalog = DocumentCatalog::new(
            INDEX_DOC_ID,
            "Vault index",
            now.clone(),
            now,
            DocKind::Index,
        );
        catalog.tags = vec!["vault-index".into()];
        session.set_catalog(catalog).unwrap();
        session
            .add_text_chunk(
                &TextHeader::code_block(Some("json")),
                &serde_json::to_string_pretty(&body).unwrap(),
            )
            .unwrap();
        session.commit().unwrap();
    }
}
