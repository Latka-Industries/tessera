//! Vault membership: in-tree scan ∪ registered external files / extra roots.
//!
//! Registered members live in `vault.tes` (`members` on index version ≥ 2) so
//! rebuild/list and `tes link` share one path set (THI-217).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, TesError};

use super::graph::collect_tes_paths;
use super::index::{VAULT_INDEX_NAME, load_vault_index};

/// How a registered membership path is interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultMemberKind {
    /// A single `.tes` file (may be outside the vault root).
    File,
    /// An extra directory to recurse for `.tes` files.
    Root,
}

impl VaultMemberKind {
    /// Wire / CLI label (`file` or `root`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Root => "root",
        }
    }
}

/// One explicitly registered vault member (not the automatic in-tree scan).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultMember {
    /// File vs extra root.
    pub kind: VaultMemberKind,
    /// Portable path: vault-relative when under the root, else absolute.
    pub path: String,
}

/// Resolve registered members + automatic scan under `root` to absolute `.tes` paths.
///
/// Excludes the root `vault.tes` sidecar. Deduplicates and sorts.
///
/// # Errors
///
/// Returns IO errors while scanning or reading the index.
pub fn membership_document_paths(root: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    let root = root.as_ref();
    let members = load_registered_members(root)?;
    membership_document_paths_with(root, &members)
}

/// Like [`membership_document_paths`], but uses an explicit member registry
/// (for rebuild-before-write so the new set is visible).
///
/// # Errors
///
/// Returns IO errors while scanning.
pub fn membership_document_paths_with(
    root: impl AsRef<Path>,
    members: &[VaultMember],
) -> Result<Vec<PathBuf>> {
    let root = root.as_ref();
    let mut set = BTreeSet::new();
    insert_tes_tree(root, root, &mut set)?;

    for member in members {
        match member.kind {
            VaultMemberKind::File => {
                let path = resolve_stored_path(root, &member.path)?;
                if path.is_file() {
                    set.insert(canon_or(path));
                }
            }
            VaultMemberKind::Root => {
                let dir = resolve_stored_path(root, &member.path)?;
                if dir.is_dir() {
                    insert_tes_tree(root, &dir, &mut set)?;
                }
            }
        }
    }

    Ok(set.into_iter().collect())
}

/// Registered members from `vault.tes` (empty when missing or v1 index).
///
/// # Errors
///
/// Returns open/decode errors when `vault.tes` exists but is invalid.
pub fn load_registered_members(root: impl AsRef<Path>) -> Result<Vec<VaultMember>> {
    Ok(load_vault_index(root)?
        .map(|index| index.members)
        .unwrap_or_default())
}

/// Register a `.tes` file or extra root directory and rebuild `vault.tes`.
///
/// # Errors
///
/// Returns IO / encode errors, or invalid path kinds.
pub fn register_member(root: impl AsRef<Path>, path: impl AsRef<Path>) -> Result<VaultMember> {
    let root = root.as_ref();
    let member = classify_member(root, &canon_or(path.as_ref().to_path_buf()))?;
    mutate_members(root, |members| {
        if members
            .iter()
            .any(|m| m.path == member.path && m.kind == member.kind)
        {
            return false;
        }
        members.push(member.clone());
        members.sort_by(|a, b| a.path.cmp(&b.path).then(a.kind.cmp(&b.kind)));
        true
    })?;
    Ok(member)
}

/// Unregister a previously registered path (file or root) and rebuild.
///
/// # Errors
///
/// Returns IO / encode errors, or when the path was not registered.
pub fn unregister_member(root: impl AsRef<Path>, path: impl AsRef<Path>) -> Result<()> {
    let root = root.as_ref();
    let abs = canon_or(path.as_ref().to_path_buf());
    let stored = store_path(root, &abs);
    let changed = mutate_members(root, |members| {
        let before = members.len();
        members.retain(|m| {
            let resolved =
                resolve_stored_path(root, &m.path).unwrap_or_else(|_| PathBuf::from(&m.path));
            canon_or(resolved) != abs && m.path != stored
        });
        members.len() != before
    })?;
    if !changed {
        return Err(TesError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("not a registered vault member: {}", path.as_ref().display()),
        )));
    }
    Ok(())
}

fn mutate_members(root: &Path, f: impl FnOnce(&mut Vec<VaultMember>) -> bool) -> Result<bool> {
    let mut members = load_registered_members(root)?;
    let changed = f(&mut members);
    if changed {
        super::index::rebuild_vault_index_with_members(root, members)?;
    }
    Ok(changed)
}

fn insert_tes_tree(vault_root: &Path, dir: &Path, set: &mut BTreeSet<PathBuf>) -> Result<()> {
    let mut paths = Vec::new();
    collect_tes_paths(dir, &mut paths)?;
    for path in paths {
        if !is_root_vault_index(vault_root, &path) {
            set.insert(canon_or(path));
        }
    }
    Ok(())
}

fn classify_member(root: &Path, abs: &Path) -> Result<VaultMember> {
    let meta = fs::metadata(abs).map_err(|e| {
        TesError::Io(std::io::Error::new(
            e.kind(),
            format!("{}: {e}", abs.display()),
        ))
    })?;
    let kind = if meta.is_dir() {
        VaultMemberKind::Root
    } else if abs.extension().and_then(|s| s.to_str()) == Some("tes") {
        VaultMemberKind::File
    } else {
        return Err(TesError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "vault member must be a .tes file or a directory: {}",
                abs.display()
            ),
        )));
    };
    // In-tree paths under the vault root are already auto-scanned; registering
    // them is allowed but redundant — still store so remove works symmetrically.
    Ok(VaultMember {
        kind,
        path: store_path(root, abs),
    })
}

fn store_path(root: &Path, abs: &Path) -> String {
    if let Ok(rel) = abs.strip_prefix(root) {
        return rel.to_string_lossy().replace('\\', "/");
    }
    abs.to_string_lossy().replace('\\', "/")
}

fn resolve_stored_path(root: &Path, stored: &str) -> Result<PathBuf> {
    let p = PathBuf::from(stored);
    if p.is_absolute() {
        Ok(p)
    } else {
        Ok(root.join(p))
    }
}

fn canon_or(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

fn is_root_vault_index(root: &Path, path: &Path) -> bool {
    path.file_name().and_then(|s| s.to_str()) == Some(VAULT_INDEX_NAME)
        && path.parent().is_some_and(|p| p == root)
}

/// Display path for TOC rows: vault-relative when possible, else absolute.
pub(super) fn display_path(root: &Path, path: &Path) -> String {
    store_path(root, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::rebuild_vault_index;
    use crate::vault::test_util::write_note;
    use tempfile::tempdir;

    #[test]
    fn register_external_file_appears_in_membership() {
        let vault = tempdir().unwrap();
        let outside = tempdir().unwrap();
        write_note(vault.path(), "in.tes", "Inside", &[]);
        write_note(outside.path(), "out.tes", "Outside", &[]);

        rebuild_vault_index(vault.path()).unwrap();
        let before = membership_document_paths(vault.path()).unwrap();
        assert_eq!(before.len(), 1);

        let external = outside.path().join("out.tes");
        register_member(vault.path(), &external).unwrap();
        let after = membership_document_paths(vault.path()).unwrap();
        assert_eq!(after.len(), 2);
        assert!(after.iter().any(|p| p.file_name().unwrap() == "out.tes"));
    }

    #[test]
    fn unregister_removes_external() {
        let vault = tempdir().unwrap();
        let outside = tempdir().unwrap();
        write_note(vault.path(), "in.tes", "Inside", &[]);
        write_note(outside.path(), "out.tes", "Outside", &[]);
        let external = outside.path().join("out.tes");
        register_member(vault.path(), &external).unwrap();
        unregister_member(vault.path(), &external).unwrap();
        assert_eq!(membership_document_paths(vault.path()).unwrap().len(), 1);
    }

    #[test]
    fn register_extra_root_includes_nested_tes() {
        let vault = tempdir().unwrap();
        let extra = tempdir().unwrap();
        write_note(vault.path(), "in.tes", "Inside", &[]);
        write_note(extra.path(), "nested.tes", "Nested", &[]);
        std::fs::create_dir(extra.path().join("sub")).unwrap();
        write_note(&extra.path().join("sub"), "deep.tes", "Deep", &[]);

        let member = register_member(vault.path(), extra.path()).unwrap();
        assert_eq!(member.kind, VaultMemberKind::Root);

        let paths = membership_document_paths(vault.path()).unwrap();
        assert_eq!(paths.len(), 3);
        assert!(paths.iter().any(|p| p.file_name().unwrap() == "nested.tes"));
        assert!(paths.iter().any(|p| p.file_name().unwrap() == "deep.tes"));

        let report = crate::vault::list_vault_documents(vault.path(), None, false).unwrap();
        assert_eq!(report.entries.len(), 3);
    }
}
