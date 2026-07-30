//! Batch Markdown / Obsidian vault → `.tes` vault import.
//!
//! Pipeline:
//! 1. Collect `.md` files (skip hidden / `.obsidian` by default)
//! 2. Plan each note: `doc_id`, title, slug, aliases, category, `doc_kind`
//! 3. Resolve slug / `doc_id` collisions within the batch
//! 4. Build a title → slug → aliases resolve map and rewrite wikilinks
//! 5. Seal each `.tes`, rebuild `vault.tes`

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use crate::catalog::TesFile;
use crate::error::Result;
use crate::io::import::{
    MarkdownImportOptions, WikilinkResolver, collect_unresolved_wikilinks, import_markdown_v0,
    parse_front_matter,
};
use crate::layout::DocKind;
use crate::vault::index::rebuild_vault_index;

/// Options for [`import_markdown_vault`].
#[derive(Debug, Clone)]
pub struct VaultMarkdownImportOptions {
    /// Skip `.obsidian` and other hidden directories (default true).
    pub skip_hidden: bool,
    /// Apply `* Index` → `doc_kind = hub` heuristic (default true).
    pub index_as_hub: bool,
    /// Resolve `[[wikilinks]]` after planning doc ids (default true).
    pub resolve_wikilinks: bool,
}

impl Default for VaultMarkdownImportOptions {
    fn default() -> Self {
        Self {
            skip_hidden: true,
            index_as_hub: true,
            resolve_wikilinks: true,
        }
    }
}

/// One imported note in a vault import report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VaultMarkdownImportEntry {
    /// Source Markdown path (relative to md root when possible).
    pub source: String,
    /// Output `.tes` path (relative to vault root when possible).
    pub output: String,
    /// Catalog document id.
    pub doc_id: String,
    /// Catalog title.
    pub title: String,
    /// Catalog slug when assigned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    /// Catalog category when assigned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Document kind written.
    pub doc_kind: String,
    /// Text chunk count.
    pub chunk_count: usize,
}

/// Summary of a vault Markdown import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VaultMarkdownImportReport {
    /// Markdown root.
    pub source_root: PathBuf,
    /// Output vault root.
    pub vault_root: PathBuf,
    /// Successful imports.
    pub imported: Vec<VaultMarkdownImportEntry>,
    /// Wikilink targets that could not be resolved (unique, sorted).
    pub unresolved_wikilinks: Vec<String>,
    /// Soft warnings: duplicate slug cleared on the later note.
    pub slug_collisions: Vec<String>,
    /// Soft warnings: duplicate Obsidian `id:` → path re-seed for losers.
    pub doc_id_collisions: Vec<String>,
    /// Path to rebuilt `vault.tes`.
    pub vault_index: PathBuf,
}

/// Import every `.md` under `source_root` into `vault_root`, preserving relative layout.
///
/// Deterministic `doc_id` comes from Obsidian front matter `id:` (else vault-relative
/// path) via [`crate::catalog::doc_id_from_seed`]. Re-import keeps an existing
/// catalog `doc_id` (D2). Duplicate `id:` values within the batch clear the later
/// slug and re-seed that note from its path. Top-level folder becomes `category`;
/// `* Index.md` becomes [`DocKind::Hub`] when enabled.
///
/// # Errors
///
/// Returns IO / import errors for the first hard failure. Soft issues (slug /
/// `doc_id` collisions, unresolved wikilinks) are reported in the result.
pub fn import_markdown_vault(
    source_root: impl AsRef<Path>,
    vault_root: impl AsRef<Path>,
    options: &VaultMarkdownImportOptions,
) -> Result<VaultMarkdownImportReport> {
    let source_root = source_root.as_ref();
    let vault_root = vault_root.as_ref();
    fs::create_dir_all(vault_root)?;

    let md_files = collect_markdown_files(source_root, options.skip_hidden)?;
    let mut plans = Vec::with_capacity(md_files.len());
    for path in &md_files {
        plans.push(plan_note(source_root, vault_root, path, options)?);
    }

    let slug_collisions = resolve_slug_collisions(&mut plans);
    let doc_id_collisions = resolve_doc_id_collisions(&mut plans);
    let resolve_map = build_wikilink_resolve_map(&plans);
    let (imported, unresolved) = import_planned_notes(&plans, &resolve_map, options)?;

    let mut unresolved_wikilinks: Vec<String> = unresolved.into_iter().collect();
    unresolved_wikilinks.sort();
    let vault_index = rebuild_vault_index(vault_root)?;

    Ok(VaultMarkdownImportReport {
        source_root: source_root.to_path_buf(),
        vault_root: vault_root.to_path_buf(),
        imported,
        unresolved_wikilinks,
        slug_collisions,
        doc_id_collisions,
        vault_index,
    })
}

/// Clear duplicate slugs: first note keeps the slug; later notes lose it.
fn resolve_slug_collisions(plans: &mut [PlannedNote]) -> Vec<String> {
    let mut used_slugs: HashMap<String, PathBuf> = HashMap::new();
    let mut collisions = Vec::new();
    for plan in plans {
        let Some(slug) = plan.slug.clone() else {
            continue;
        };
        if let Some(first) = used_slugs.get(&slug) {
            collisions.push(format!(
                "slug '{slug}' already used by {}; clearing on {}",
                first.display(),
                plan.rel_md.display()
            ));
            plan.slug = None;
        } else {
            used_slugs.insert(slug, plan.rel_md.clone());
        }
    }
    collisions
}

/// Re-seed duplicate `doc_id`s from path; prefer the note that still owns its slug.
fn resolve_doc_id_collisions(plans: &mut [PlannedNote]) -> Vec<String> {
    let mut id_groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, plan) in plans.iter().enumerate() {
        id_groups.entry(plan.doc_id.clone()).or_default().push(i);
    }
    let mut collisions = Vec::new();
    for (doc_id, indices) in &id_groups {
        if indices.len() < 2 {
            continue;
        }
        let winner = indices
            .iter()
            .copied()
            .find(|&i| plans[i].slug.is_some())
            .unwrap_or(indices[0]);
        let winner_path = plans[winner].rel_md.display().to_string();
        for &i in indices {
            if i == winner {
                continue;
            }
            let new_id = crate::catalog::doc_id_from_seed(&path_seed(&plans[i].rel_md)).to_string();
            collisions.push(format!(
                "doc_id {doc_id} already used by {winner_path}; re-seeding {} from path",
                plans[i].rel_md.display()
            ));
            plans[i].doc_id = new_id;
        }
    }
    collisions
}

/// Map title / slug / aliases / filename stem → `doc_id` (first registration wins).
fn build_wikilink_resolve_map(plans: &[PlannedNote]) -> HashMap<String, String> {
    let mut resolve_map = HashMap::new();
    for plan in plans {
        register_name(&mut resolve_map, &plan.title, &plan.doc_id);
        if let Some(slug) = &plan.slug {
            register_name(&mut resolve_map, slug, &plan.doc_id);
        }
        for alias in &plan.aliases {
            register_name(&mut resolve_map, alias, &plan.doc_id);
        }
        if let Some(stem) = plan.rel_md.file_stem().and_then(|s| s.to_str()) {
            register_name(&mut resolve_map, stem, &plan.doc_id);
        }
    }
    resolve_map
}

/// Seal each planned note and accumulate unresolved wikilink targets.
fn import_planned_notes(
    plans: &[PlannedNote],
    resolve_map: &HashMap<String, String>,
    options: &VaultMarkdownImportOptions,
) -> Result<(Vec<VaultMarkdownImportEntry>, HashSet<String>)> {
    let resolver = options.resolve_wikilinks.then(|| -> WikilinkResolver {
        let map = resolve_map.clone();
        Arc::new(move |name: &str| map.get(name).cloned())
    });

    let mut unresolved = HashSet::new();
    let mut imported = Vec::with_capacity(plans.len());
    for plan in plans {
        let import_opts = MarkdownImportOptions {
            doc_kind: plan.doc_kind,
            title: None,
            doc_id: Some(plan.doc_id.clone()),
            doc_id_seed: None,
            tags: Vec::new(),
            category: plan.category.clone(),
            aliases: Vec::new(),
            slug: plan.slug.clone(),
            slug_override: true,
            wikilink_resolver: resolver.clone(),
        };

        if let Some(parent) = plan.abs_tes.parent() {
            fs::create_dir_all(parent)?;
        }
        let report = import_markdown_v0(&plan.abs_md, &plan.abs_tes, &import_opts)?;
        if options.resolve_wikilinks {
            let source = fs::read_to_string(&plan.abs_md)?;
            let (_, body) = parse_front_matter(&source);
            collect_unresolved_wikilinks(
                body,
                |name| resolve_map.contains_key(name),
                &mut unresolved,
            );
        }
        imported.push(VaultMarkdownImportEntry {
            source: plan.rel_md.display().to_string(),
            output: plan.rel_tes.display().to_string(),
            doc_id: report.doc_id,
            title: report.title,
            slug: report.slug,
            category: plan.category.clone(),
            doc_kind: plan.doc_kind.as_str().to_owned(),
            chunk_count: report.chunk_count,
        });
    }
    Ok((imported, unresolved))
}

/// Planned conversion of one Markdown note before sealing.
#[derive(Debug)]
struct PlannedNote {
    abs_md: PathBuf,
    abs_tes: PathBuf,
    rel_md: PathBuf,
    rel_tes: PathBuf,
    doc_id: String,
    title: String,
    slug: Option<String>,
    aliases: Vec<String>,
    category: Option<String>,
    doc_kind: DocKind,
}

/// Build a [`PlannedNote`] from one source Markdown path.
fn plan_note(
    source_root: &Path,
    vault_root: &Path,
    abs_md: &Path,
    options: &VaultMarkdownImportOptions,
) -> Result<PlannedNote> {
    let rel_md = abs_md
        .strip_prefix(source_root)
        .unwrap_or(abs_md)
        .to_path_buf();
    let rel_tes = rel_md.with_extension("tes");
    let abs_tes = vault_root.join(&rel_tes);

    let source = fs::read_to_string(abs_md)?;
    let (front, body) = parse_front_matter(&source);
    let title = front
        .title
        .clone()
        .or_else(|| first_markdown_heading(body))
        .or_else(|| {
            abs_md
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "Untitled".to_owned());

    let category = top_level_category(&rel_md);
    let seed = front.id.clone().unwrap_or_else(|| path_seed(&rel_md));
    let doc_id = existing_or_seeded_doc_id(&abs_tes, &seed);

    let doc_kind = if options.index_as_hub && is_index_note(&rel_md, &title) {
        DocKind::Hub
    } else {
        DocKind::Note
    };

    Ok(PlannedNote {
        abs_md: abs_md.to_path_buf(),
        abs_tes,
        rel_md,
        rel_tes,
        doc_id,
        title,
        slug: front.id,
        aliases: front.aliases,
        category,
        doc_kind,
    })
}

/// Top-level folder name, or `None` for vault-root Markdown files.
fn top_level_category(rel_md: &Path) -> Option<String> {
    rel_md
        .components()
        .next()
        .and_then(|c| c.as_os_str().to_str())
        .filter(|s| {
            !Path::new(s)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        })
        .map(str::to_owned)
}

/// Vault-relative path seed for `UUIDv5` (forward slashes).
fn path_seed(rel: &Path) -> String {
    rel.to_string_lossy().replace('\\', "/")
}

/// Keep catalog `doc_id` when the output `.tes` already exists; else seed.
fn existing_or_seeded_doc_id(abs_tes: &Path, seed: &str) -> String {
    let seeded = || crate::catalog::doc_id_from_seed(seed).to_string();
    if !abs_tes.is_file() {
        return seeded();
    }
    TesFile::open(abs_tes)
        .ok()
        .and_then(|file| file.catalog().map(|c| c.doc_id.clone()))
        .unwrap_or_else(seeded)
}

/// True when the stem/title looks like an Obsidian folder index note.
fn is_index_note(rel: &Path, title: &str) -> bool {
    let stem = rel.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    stem.ends_with(" Index") || stem == "Index" || title.ends_with(" Index") || title == "Index"
}

/// First ATX heading text in the Markdown body, if any.
fn first_markdown_heading(body: &str) -> Option<String> {
    for line in body.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix('#') {
            let heading = rest.trim_start_matches('#').trim();
            if !heading.is_empty() {
                return Some(heading.to_owned());
            }
        }
    }
    None
}

fn register_name(map: &mut HashMap<String, String>, name: &str, doc_id: &str) {
    let key = name.trim();
    if key.is_empty() {
        return;
    }
    map.entry(key.to_owned())
        .or_insert_with(|| doc_id.to_owned());
}

fn collect_markdown_files(root: &Path, skip_hidden: bool) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    collect_markdown_files_rec(root, skip_hidden, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect_markdown_files_rec(dir: &Path, skip_hidden: bool, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if skip_hidden && name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_markdown_files_rec(&path, skip_hidden, out)?;
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn imports_vault_with_wikilinks_and_category() {
        let src = tempdir().unwrap();
        let dst = tempdir().unwrap();
        let lit = src.path().join("Literature");
        fs::create_dir_all(&lit).unwrap();
        fs::write(
            lit.join("Erasure.md"),
            "---\nid: Erasure\ntags:\n  - Fiction\naliases:\n  - American Fiction\n---\n# Erasure\n\nBy someone.\n",
        )
        .unwrap();
        fs::write(
            lit.join("Literature Index.md"),
            "# Literature Index\n\nSee [[Erasure]] and [[American Fiction]].\n",
        )
        .unwrap();

        let report = import_markdown_vault(
            src.path(),
            dst.path(),
            &VaultMarkdownImportOptions::default(),
        )
        .unwrap();
        assert_eq!(report.imported.len(), 2);
        assert!(report.unresolved_wikilinks.is_empty());
        let index = report
            .imported
            .iter()
            .find(|e| e.title == "Literature Index")
            .unwrap();
        assert_eq!(index.doc_kind, "hub");
        assert_eq!(index.category.as_deref(), Some("Literature"));

        let erasure = report
            .imported
            .iter()
            .find(|e| e.slug.as_deref() == Some("Erasure"))
            .unwrap();
        let file = TesFile::open(dst.path().join(&erasure.output)).unwrap();
        let cat = file.catalog().unwrap();
        assert_eq!(cat.tags, vec!["Fiction"]);
        assert_eq!(cat.aliases, vec!["American Fiction"]);
        assert_eq!(cat.category.as_deref(), Some("Literature"));
        assert!(dst.path().join("vault.tes").is_file());
    }

    #[test]
    fn reimport_keeps_doc_id() {
        let src = tempdir().unwrap();
        let dst = tempdir().unwrap();
        fs::create_dir_all(src.path().join("Concepts")).unwrap();
        let md = src.path().join("Concepts/Chunking.md");
        fs::write(&md, "# Chunking\n\nOne.\n").unwrap();
        let first = import_markdown_vault(
            src.path(),
            dst.path(),
            &VaultMarkdownImportOptions::default(),
        )
        .unwrap();
        fs::write(&md, "# Chunking\n\nTwo.\n").unwrap();
        let second = import_markdown_vault(
            src.path(),
            dst.path(),
            &VaultMarkdownImportOptions::default(),
        )
        .unwrap();
        assert_eq!(first.imported[0].doc_id, second.imported[0].doc_id);
    }

    #[test]
    fn duplicate_obsidian_id_reseeds_doc_id() {
        let src = tempdir().unwrap();
        let dst = tempdir().unwrap();
        let lit = src.path().join("Literature");
        fs::create_dir_all(lit.join("Authors")).unwrap();
        fs::create_dir_all(lit.join("Books")).unwrap();
        fs::write(
            lit.join("Authors/David Foster Wallace.md"),
            "---\nid: David Foster Wallace\naliases:\n  - DFW\n---\n# David Foster Wallace\n",
        )
        .unwrap();
        fs::write(
            lit.join("Books/The Broom of the System.md"),
            "---\nid: David Foster Wallace\n---\n# The Broom of the System\n\nBy [[David Foster Wallace]].\n",
        )
        .unwrap();

        let report = import_markdown_vault(
            src.path(),
            dst.path(),
            &VaultMarkdownImportOptions::default(),
        )
        .unwrap();
        assert_eq!(report.imported.len(), 2);
        assert_eq!(report.doc_id_collisions.len(), 1);
        assert_ne!(report.imported[0].doc_id, report.imported[1].doc_id);
        let author = report
            .imported
            .iter()
            .find(|e| e.source.contains("Authors"))
            .unwrap();
        let book = report
            .imported
            .iter()
            .find(|e| e.source.contains("Books"))
            .unwrap();
        assert_eq!(author.slug.as_deref(), Some("David Foster Wallace"));
        assert!(book.slug.is_none());
        assert!(report.unresolved_wikilinks.is_empty());
    }
}
