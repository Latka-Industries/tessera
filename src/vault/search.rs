//! Vault search: parallel scan for small vaults, Tantivy index under `.tessera/fts`.
//!
//! Default mode is an in-process scan of membership (`--ai-text` + catalog fields).
//! When membership reaches [`AUTO_INDEX_DOC_THRESHOLD`], or the caller opts in with
//! `--index`, search uses a Tantivy BM25 index (THI-223).

use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{STORED, STRING, Schema, TEXT, TantivyDocument, Value};
use tantivy::snippet::SnippetGenerator;
use tantivy::{Index, IndexWriter, ReloadPolicy, doc};

use crate::catalog::TesFile;
use crate::error::{Result, TesError};
use crate::io::export::{ExportOptions, ExportView, export_file};
use crate::layout::DocKind;

use super::index::file_mtime_secs;
use super::members::{display_path, load_registered_members, membership_document_paths_with};

/// Vault-local cache directory (FTS index lives under this).
pub const VAULT_DOT_DIR: &str = ".tessera";

/// Tantivy index directory name under [`.tessera`](VAULT_DOT_DIR).
pub const VAULT_FTS_DIRNAME: &str = "fts";

/// Membership size at/above which search auto-uses the Tantivy index.
pub const AUTO_INDEX_DOC_THRESHOLD: usize = 64;

const SIGNATURES_NAME: &str = "tessera_signatures.json";
const FTS_FORMAT: &str = "tessera.vault_fts";
const FTS_VERSION: u32 = 1;
const WRITER_HEAP: usize = 50_000_000;

/// How a search was executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultSearchMode {
    /// Parallel membership scan (default for small vaults).
    Scan,
    /// Tantivy BM25 index under `.tessera/fts`.
    Index,
}

/// Options for [`search_vault`].
#[derive(Debug, Clone, Copy, Default)]
pub struct VaultSearchOptions {
    /// Max hits (clamped to at least 1).
    pub limit: usize,
    /// Always rebuild the Tantivy index before an indexed search.
    pub force_rebuild: bool,
    /// Force Tantivy even when below the auto threshold.
    pub force_index: bool,
    /// Force scan even when above the auto threshold / an index exists.
    pub force_scan: bool,
}

/// One search hit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VaultSearchHit {
    /// Stable document UUID.
    pub doc_id: String,
    /// Display title.
    pub title: String,
    /// Document kind string.
    pub doc_kind: String,
    /// Path relative to vault root when in-tree; otherwise absolute.
    pub path: String,
    /// Optional category.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Optional slug.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    /// Short body/title snippet.
    pub snippet: String,
}

/// Result of a vault search.
#[derive(Debug, Clone, Serialize)]
pub struct VaultSearchReport {
    /// Hits in rank order.
    pub hits: Vec<VaultSearchHit>,
    /// Query string.
    pub query: String,
    /// Backend used for this call.
    pub mode: VaultSearchMode,
    /// Whether the Tantivy index was rebuilt for this call.
    pub rebuilt: bool,
    /// True when an existing index was missing/stale before rebuild.
    pub was_stale: bool,
    /// Membership documents considered (excludes `vault.tes` index docs).
    pub documents: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FtsSignatures {
    format: String,
    version: u32,
    signatures: Vec<(String, u64)>,
}

#[derive(Debug, Clone)]
struct SearchableDoc {
    doc_id: String,
    title: String,
    doc_kind: String,
    path: String,
    category: Option<String>,
    slug: Option<String>,
    aliases: String,
    tags: String,
    body: String,
    mtime_secs: u64,
}

/// Absolute path to `root/.tessera/fts`.
#[must_use]
pub fn vault_fts_path(root: impl AsRef<Path>) -> PathBuf {
    root.as_ref().join(VAULT_DOT_DIR).join(VAULT_FTS_DIRNAME)
}

/// Rebuild the Tantivy index from current membership.
///
/// # Errors
///
/// Returns open/export/Tantivy errors while scanning or writing the index.
pub fn rebuild_vault_fts(root: impl AsRef<Path>) -> Result<PathBuf> {
    let root = root.as_ref();
    let docs = load_searchable_docs(root)?;
    rebuild_index_from_docs(root, &docs)
}

/// Whether the Tantivy index exists and matches current membership mtimes.
///
/// # Errors
///
/// Returns IO errors while inspecting membership or the sidecar.
pub fn vault_fts_is_fresh(root: impl AsRef<Path>) -> Result<bool> {
    let root = root.as_ref();
    let docs = load_searchable_docs(root)?;
    index_matches_signatures(&vault_fts_path(root), &docs_to_signatures(&docs))
}

/// Search the vault (scan or Tantivy per [`VaultSearchOptions`] / size threshold).
///
/// # Errors
///
/// Returns open/export/Tantivy / invalid-query errors.
pub fn search_vault(
    root: impl AsRef<Path>,
    query: &str,
    options: VaultSearchOptions,
) -> Result<VaultSearchReport> {
    let root = root.as_ref();
    let query = query.trim();
    if query.is_empty() {
        return Err(TesError::VaultFts {
            message: "empty search query".into(),
        });
    }
    if options.force_index && options.force_scan {
        return Err(TesError::VaultFts {
            message: "cannot combine force_index and force_scan".into(),
        });
    }

    let docs = load_searchable_docs(root)?;
    let documents = docs.len();
    let limit = options.limit.max(1);

    let mode = resolve_mode(documents, options);
    match mode {
        VaultSearchMode::Scan => {
            let hits = scan_search(&docs, query, limit);
            Ok(VaultSearchReport {
                hits,
                query: query.to_owned(),
                mode,
                rebuilt: false,
                was_stale: false,
                documents,
            })
        }
        VaultSearchMode::Index => {
            let expected = docs_to_signatures(&docs);
            let fresh = index_matches_signatures(&vault_fts_path(root), &expected)?;
            let was_stale = !fresh;
            let mut rebuilt = false;
            if options.force_rebuild || !fresh {
                rebuild_index_from_docs(root, &docs)?;
                rebuilt = true;
            }
            let hits = tantivy_search(root, query, limit)?;
            Ok(VaultSearchReport {
                hits,
                query: query.to_owned(),
                mode,
                rebuilt,
                was_stale,
                documents,
            })
        }
    }
}

fn index_matches_signatures(dir: &Path, expected: &[(String, u64)]) -> Result<bool> {
    if !dir.is_dir() {
        return Ok(false);
    }
    let Some(stored) = read_signatures(dir)? else {
        return Ok(false);
    };
    if stored.format != FTS_FORMAT || stored.version != FTS_VERSION {
        return Ok(false);
    }
    Ok(stored.signatures == expected)
}

fn resolve_mode(documents: usize, options: VaultSearchOptions) -> VaultSearchMode {
    if options.force_scan {
        VaultSearchMode::Scan
    } else if options.force_index || options.force_rebuild || documents >= AUTO_INDEX_DOC_THRESHOLD
    {
        VaultSearchMode::Index
    } else {
        VaultSearchMode::Scan
    }
}

fn load_searchable_docs(root: &Path) -> Result<Vec<SearchableDoc>> {
    let members = load_registered_members(root)?;
    let paths = membership_document_paths_with(root, &members)?;
    let mut docs = Vec::new();
    for abs in paths {
        let file = TesFile::open(&abs)?;
        let Some(catalog) = file.catalog() else {
            continue;
        };
        if catalog.doc_kind == DocKind::Index.as_str() {
            continue;
        }
        let body = export_file(&file, ExportView::AiText, &ExportOptions::default())?;
        docs.push(SearchableDoc {
            doc_id: catalog.doc_id.clone(),
            title: catalog.title.clone(),
            doc_kind: catalog.doc_kind.clone(),
            path: display_path(root, &abs),
            category: catalog.category.clone(),
            slug: catalog.slug.clone(),
            aliases: catalog.aliases.join(" "),
            tags: catalog.tags.join(" "),
            body,
            mtime_secs: file_mtime_secs(&abs)?,
        });
    }
    Ok(docs)
}

fn docs_to_signatures(docs: &[SearchableDoc]) -> Vec<(String, u64)> {
    let mut out: Vec<(String, u64)> = docs
        .iter()
        .map(|d| (d.path.clone(), d.mtime_secs))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn rebuild_index_from_docs(root: &Path, docs: &[SearchableDoc]) -> Result<PathBuf> {
    let dir = vault_fts_path(root);
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;

    let (schema, fields) = build_schema();
    let index = Index::create_in_dir(&dir, schema).map_err(fts_err)?;
    let mut writer: IndexWriter = index.writer(WRITER_HEAP).map_err(fts_err)?;

    for d in docs {
        writer
            .add_document(doc!(
                fields.title => d.title.as_str(),
                fields.body => d.body.as_str(),
                fields.aliases => d.aliases.as_str(),
                fields.tags => d.tags.as_str(),
                fields.category => d.category.as_deref().unwrap_or(""),
                fields.slug => d.slug.as_deref().unwrap_or(""),
                fields.path => d.path.as_str(),
                fields.doc_id => d.doc_id.as_str(),
                fields.doc_kind => d.doc_kind.as_str(),
            ))
            .map_err(fts_err)?;
    }
    writer.commit().map_err(fts_err)?;

    write_signatures(
        &dir,
        &FtsSignatures {
            format: FTS_FORMAT.into(),
            version: FTS_VERSION,
            signatures: docs_to_signatures(docs),
        },
    )?;
    Ok(dir)
}

struct SchemaFields {
    title: tantivy::schema::Field,
    body: tantivy::schema::Field,
    aliases: tantivy::schema::Field,
    tags: tantivy::schema::Field,
    category: tantivy::schema::Field,
    slug: tantivy::schema::Field,
    path: tantivy::schema::Field,
    doc_id: tantivy::schema::Field,
    doc_kind: tantivy::schema::Field,
}

fn build_schema() -> (Schema, SchemaFields) {
    let mut builder = Schema::builder();
    let title = builder.add_text_field("title", TEXT | STORED);
    let body = builder.add_text_field("body", TEXT | STORED);
    let aliases = builder.add_text_field("aliases", TEXT | STORED);
    let tags = builder.add_text_field("tags", TEXT | STORED);
    let category = builder.add_text_field("category", TEXT | STORED);
    let slug = builder.add_text_field("slug", TEXT | STORED);
    let path = builder.add_text_field("path", STRING | STORED);
    let doc_id = builder.add_text_field("doc_id", STRING | STORED);
    let doc_kind = builder.add_text_field("doc_kind", STRING | STORED);
    let schema = builder.build();
    (
        schema,
        SchemaFields {
            title,
            body,
            aliases,
            tags,
            category,
            slug,
            path,
            doc_id,
            doc_kind,
        },
    )
}

fn tantivy_search(root: &Path, query: &str, limit: usize) -> Result<Vec<VaultSearchHit>> {
    let dir = vault_fts_path(root);
    let index = Index::open_in_dir(&dir).map_err(fts_err)?;
    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::Manual)
        .try_into()
        .map_err(fts_err)?;
    let searcher = reader.searcher();
    let schema = index.schema();
    let title = schema.get_field("title").map_err(fts_err)?;
    let body = schema.get_field("body").map_err(fts_err)?;
    let aliases = schema.get_field("aliases").map_err(fts_err)?;
    let tags = schema.get_field("tags").map_err(fts_err)?;
    let category = schema.get_field("category").map_err(fts_err)?;
    let slug = schema.get_field("slug").map_err(fts_err)?;
    let path = schema.get_field("path").map_err(fts_err)?;
    let doc_id = schema.get_field("doc_id").map_err(fts_err)?;
    let doc_kind = schema.get_field("doc_kind").map_err(fts_err)?;

    let mut parser =
        QueryParser::for_index(&index, vec![title, body, aliases, tags, category, slug]);
    parser.set_field_boost(title, 2.5);
    parser.set_field_boost(aliases, 1.5);
    let parsed = parser.parse_query(query).map_err(|e| TesError::VaultFts {
        message: format!("invalid search query '{query}': {e}"),
    })?;

    let top = searcher
        .search(&parsed, &TopDocs::with_limit(limit).order_by_score())
        .map_err(fts_err)?;
    let snippet_gen = SnippetGenerator::create(&searcher, &*parsed, body).map_err(fts_err)?;

    let mut hits = Vec::with_capacity(top.len());
    for (_score, addr) in top {
        let doc = searcher.doc::<TantivyDocument>(addr).map_err(fts_err)?;
        let snippet = snippet_gen.snippet_from_doc(&doc);
        let snippet_text = {
            let html = snippet.to_html();
            if html.trim().is_empty() {
                stored_str(&doc, title).unwrap_or_default()
            } else {
                html.replace("<b>", "[").replace("</b>", "]")
            }
        };
        hits.push(VaultSearchHit {
            doc_id: stored_str(&doc, doc_id).unwrap_or_default(),
            title: stored_str(&doc, title).unwrap_or_default(),
            doc_kind: stored_str(&doc, doc_kind).unwrap_or_default(),
            path: stored_str(&doc, path).unwrap_or_default(),
            category: stored_str(&doc, category).filter(|s| !s.is_empty()),
            slug: stored_str(&doc, slug).filter(|s| !s.is_empty()),
            snippet: snippet_text,
        });
    }
    Ok(hits)
}

fn stored_str(doc: &TantivyDocument, field: tantivy::schema::Field) -> Option<String> {
    doc.get_first(field)
        .and_then(|v| v.as_str().map(str::to_owned))
}

fn scan_search(docs: &[SearchableDoc], query: &str, limit: usize) -> Vec<VaultSearchHit> {
    let q = query.to_lowercase();
    let mut scored: Vec<(i32, VaultSearchHit)> = docs
        .par_iter()
        .filter_map(|d| {
            let title_l = d.title.to_lowercase();
            let body_l = d.body.to_lowercase();
            let aliases_l = d.aliases.to_lowercase();
            let tags_l = d.tags.to_lowercase();
            let slug_l = d.slug.as_deref().unwrap_or("").to_lowercase();
            let cat_l = d.category.as_deref().unwrap_or("").to_lowercase();

            let mut score = 0i32;
            if title_l.contains(&q) {
                score += 100;
            }
            if aliases_l.contains(&q) || slug_l.contains(&q) {
                score += 60;
            }
            if tags_l.contains(&q) || cat_l.contains(&q) {
                score += 40;
            }
            if body_l.contains(&q) {
                score += 20;
            }
            if score == 0 {
                return None;
            }
            Some((
                score,
                VaultSearchHit {
                    doc_id: d.doc_id.clone(),
                    title: d.title.clone(),
                    doc_kind: d.doc_kind.clone(),
                    path: d.path.clone(),
                    category: d.category.clone(),
                    slug: d.slug.clone(),
                    snippet: scan_snippet(&d.body, &d.title, &q),
                },
            ))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.title.cmp(&b.1.title)));
    scored.into_iter().take(limit).map(|(_, h)| h).collect()
}

fn scan_snippet(body: &str, title: &str, query_lower: &str) -> String {
    let body_l = body.to_lowercase();
    if let Some(idx) = body_l.find(query_lower) {
        let start = idx.saturating_sub(40);
        let end = (idx + query_lower.len() + 40).min(body.len());
        let mut snip = body[start..end].replace(['\t', '\n'], " ");
        if start > 0 {
            snip.insert(0, '…');
        }
        if end < body.len() {
            snip.push('…');
        }
        return snip;
    }
    title.to_owned()
}

fn signatures_path(dir: &Path) -> PathBuf {
    dir.join(SIGNATURES_NAME)
}

fn write_signatures(dir: &Path, sigs: &FtsSignatures) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(sigs)?;
    fs::write(signatures_path(dir), bytes)?;
    Ok(())
}

fn read_signatures(dir: &Path) -> Result<Option<FtsSignatures>> {
    let path = signatures_path(dir);
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(path)?;
    Ok(Some(serde_json::from_slice(&bytes)?))
}

fn fts_err(err: impl std::fmt::Display) -> TesError {
    TesError::VaultFts {
        message: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{DocumentCatalog, TesWriterSession, TextHeader};
    use crate::layout::DocKind;
    use crate::vault::rebuild_vault_index;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn write_body(dir: &Path, name: &str, title: &str, body: &str) {
        let path = dir.join(name);
        let mut session = TesWriterSession::create(&path, DocKind::Note);
        let mut catalog = DocumentCatalog::new(
            Uuid::new_v4().to_string(),
            title,
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Note,
        );
        catalog.tags = vec!["searchable".into()];
        session.set_catalog(catalog).unwrap();
        session
            .add_text_chunk(&TextHeader::paragraph(), body)
            .unwrap();
        session.commit().unwrap();
    }

    #[test]
    fn small_vault_defaults_to_scan() {
        let dir = tempdir().unwrap();
        write_body(
            dir.path(),
            "a.tes",
            "Alpha note",
            "unique xylophone phrase in alpha",
        );
        write_body(
            dir.path(),
            "b.tes",
            "Beta note",
            "completely different content",
        );

        let report = search_vault(
            dir.path(),
            "xylophone",
            VaultSearchOptions {
                limit: 10,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(report.mode, VaultSearchMode::Scan);
        assert!(!report.rebuilt);
        assert_eq!(report.hits.len(), 1);
        assert_eq!(report.hits[0].title, "Alpha note");
        assert!(!vault_fts_path(dir.path()).exists());
    }

    #[test]
    fn force_index_builds_under_dot_tessera() {
        let dir = tempdir().unwrap();
        write_body(dir.path(), "a.tes", "Alpha", "needle in a haystack");
        let report = search_vault(
            dir.path(),
            "needle",
            VaultSearchOptions {
                limit: 10,
                force_index: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(report.mode, VaultSearchMode::Index);
        assert!(report.rebuilt);
        assert!(vault_fts_path(dir.path()).starts_with(dir.path().join(VAULT_DOT_DIR)));
        assert!(vault_fts_path(dir.path()).is_dir());
        assert_eq!(report.hits.len(), 1);

        let again = search_vault(
            dir.path(),
            "needle",
            VaultSearchOptions {
                limit: 10,
                force_index: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!again.rebuilt);
        assert!(!again.was_stale);
    }

    #[test]
    fn stale_index_rebuilds_on_indexed_search() {
        let dir = tempdir().unwrap();
        write_body(dir.path(), "a.tes", "Alpha", "first body");
        rebuild_vault_fts(dir.path()).unwrap();
        assert!(vault_fts_is_fresh(dir.path()).unwrap());

        write_body(dir.path(), "b.tes", "Beta", "second body with zebra");
        assert!(!vault_fts_is_fresh(dir.path()).unwrap());

        let report = search_vault(
            dir.path(),
            "zebra",
            VaultSearchOptions {
                limit: 10,
                force_index: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(report.was_stale);
        assert!(report.rebuilt);
        assert_eq!(report.hits.len(), 1);
        assert_eq!(report.hits[0].title, "Beta");
    }

    #[test]
    fn skips_vault_index_doc() {
        let dir = tempdir().unwrap();
        write_body(dir.path(), "a.tes", "Note", "needle unique");
        rebuild_vault_index(dir.path()).unwrap();
        let report = search_vault(
            dir.path(),
            "needle",
            VaultSearchOptions {
                limit: 10,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(report.hits.len(), 1);
        assert_ne!(report.hits[0].doc_kind, "index");
    }
}
