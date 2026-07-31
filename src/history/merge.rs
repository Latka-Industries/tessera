//! Verified 3-way structural merge for git (`tes merge-file`).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::catalog::document::DocumentCatalog;
use crate::catalog::file::TesFile;
use crate::catalog::history::{attach_footer, content_hash, split_body_and_history};
use crate::catalog::index::ChunkType;
use crate::catalog::session::TesWriterSession;
use crate::error::{Result, TesError};
use crate::layout::SuperblockV0;
use crate::verify::verify_bytes;

use super::atomic_replace;

/// One side of a 3-way merge snapshot.
#[derive(Debug, Clone)]
struct SideSnapshot {
    catalog_hash: String,
    catalog_bytes: Vec<u8>,
    doc_kind: crate::layout::DocKind,
    /// `chunk_id` → (type, flags, hash, payload)
    chunks: BTreeMap<u64, ChunkSnap>,
}

#[derive(Debug, Clone)]
struct ChunkSnap {
    chunk_type: ChunkType,
    chunk_flags: u32,
    hash: String,
    payload: Vec<u8>,
}

/// Report from a successful auto-merge.
#[derive(Debug, Clone)]
pub struct MergeReport {
    /// Path written (`ours` / `%A`).
    pub path: PathBuf,
    /// Chunk ids taken from ours.
    pub from_ours: Vec<u64>,
    /// Chunk ids taken from theirs.
    pub from_theirs: Vec<u64>,
    /// Chunk ids unchanged from base.
    pub unchanged: Vec<u64>,
}

/// Three-way structural merge of `.tes` files for git merge drivers.
///
/// Convention matches git: `base` = `%O`, `ours` = `%A` (also the output path),
/// `theirs` = `%B`. On success, overwrites `ours` with a deep-verified sealed
/// body (re-attaches `ours`' `THST` footer when present). On conflict, leaves
/// `ours` untouched and returns [`TesError::MergeConflict`].
///
/// # Errors
///
/// Returns open/decode errors, [`TesError::MergeConflict`] when chunk or
/// catalog edits overlap unsafely, or verify failure after rebuild.
pub fn merge_files(
    base: impl AsRef<Path>,
    ours: impl AsRef<Path>,
    theirs: impl AsRef<Path>,
) -> Result<MergeReport> {
    let base = base.as_ref();
    let ours = ours.as_ref();
    let theirs = theirs.as_ref();

    let snap_o = snapshot_side(base)?;
    let snap_a = snapshot_side(ours)?;
    let snap_b = snapshot_side(theirs)?;

    let mut conflicts = Vec::new();
    let catalog_bytes = merge_catalog(&snap_o, &snap_a, &snap_b, &mut conflicts);
    let (merged, from_ours, from_theirs, unchanged) =
        merge_chunks(&snap_o, &snap_a, &snap_b, &mut conflicts);

    if !conflicts.is_empty() {
        return Err(TesError::MergeConflict {
            message: conflicts.join("; "),
        });
    }

    let mut session = TesWriterSession::create(ours, snap_a.doc_kind);
    if !catalog_bytes.is_empty() {
        session.set_catalog(DocumentCatalog::from_bytes(&catalog_bytes)?)?;
    }
    for chunk in merged.values() {
        session.add_payload_chunk(chunk.chunk_type, chunk.chunk_flags, chunk.payload.clone())?;
    }
    let body = session.encode_file()?;
    let out = seal_with_ours_history(ours, body)?;

    let report = verify_bytes(ours, &out, true);
    if !report.ok {
        let message = report
            .findings
            .iter()
            .find(|f| matches!(f.severity, crate::verify::Severity::Error))
            .map_or_else(|| "merge verify failed".into(), |f| f.message.clone());
        return Err(TesError::EditVerifyFailed { message });
    }

    atomic_replace(ours, &out)?;
    Ok(MergeReport {
        path: ours.to_path_buf(),
        from_ours,
        from_theirs,
        unchanged,
    })
}

fn seal_with_ours_history(ours: &Path, body: Vec<u8>) -> Result<Vec<u8>> {
    let ours_bytes = fs::read(ours)?;
    let sb = SuperblockV0::from_bytes(&ours_bytes)?;
    if !sb.has_history_footer() {
        return Ok(body);
    }
    let (_old_body, history) = split_body_and_history(&ours_bytes, true)?;
    match history {
        Some(h) => attach_footer(body, &h),
        None => Ok(body),
    }
}

fn snapshot_side(path: &Path) -> Result<SideSnapshot> {
    let file = TesFile::open(path)?;
    let catalog_bytes = match file.catalog() {
        Some(cat) => cat.to_bytes()?,
        None => Vec::new(),
    };
    let catalog_hash = content_hash(&catalog_bytes);
    let mut chunks = BTreeMap::new();
    for entry in file.chunks() {
        let payload = file.decode_payload(entry)?;
        let hash = content_hash(payload.as_ref());
        chunks.insert(
            entry.chunk_id,
            ChunkSnap {
                chunk_type: entry.chunk_type,
                chunk_flags: entry.chunk_flags,
                hash,
                payload: payload.into_owned(),
            },
        );
    }
    Ok(SideSnapshot {
        catalog_hash,
        catalog_bytes,
        doc_kind: file.superblock().doc_kind,
        chunks,
    })
}

fn merge_catalog(
    base: &SideSnapshot,
    ours: &SideSnapshot,
    theirs: &SideSnapshot,
    conflicts: &mut Vec<String>,
) -> Vec<u8> {
    if let Some(bytes) = three_way_pick(
        &base.catalog_hash,
        &ours.catalog_hash,
        &theirs.catalog_hash,
        &ours.catalog_bytes,
        &theirs.catalog_bytes,
    ) {
        return bytes.clone();
    }

    // Both sides touched catalog (common: `modified` bump from apply_ops).
    let Some((cat_o, cat_a, cat_b)) = decode_catalogs(base, ours, theirs, conflicts) else {
        return ours.catalog_bytes.clone();
    };

    let mut out = cat_a.clone();
    out.doc_id = three_way_eq(
        &cat_o.doc_id,
        &cat_a.doc_id,
        &cat_b.doc_id,
        "doc_id",
        conflicts,
    );
    out.title = three_way_eq(&cat_o.title, &cat_a.title, &cat_b.title, "title", conflicts);
    out.created = three_way_eq(
        &cat_o.created,
        &cat_a.created,
        &cat_b.created,
        "created",
        conflicts,
    );
    out.modified = three_way_prefer_newer(&cat_o.modified, &cat_a.modified, &cat_b.modified);
    out.doc_kind = three_way_eq(
        &cat_o.doc_kind,
        &cat_a.doc_kind,
        &cat_b.doc_kind,
        "doc_kind",
        conflicts,
    );
    out.tags = three_way_eq(&cat_o.tags, &cat_a.tags, &cat_b.tags, "tags", conflicts);
    out.category = three_way_eq(
        &cat_o.category,
        &cat_a.category,
        &cat_b.category,
        "category",
        conflicts,
    );
    out.section = three_way_eq(
        &cat_o.section,
        &cat_a.section,
        &cat_b.section,
        "section",
        conflicts,
    );
    out.aliases = three_way_eq(
        &cat_o.aliases,
        &cat_a.aliases,
        &cat_b.aliases,
        "aliases",
        conflicts,
    );
    out.slug = three_way_eq(&cat_o.slug, &cat_a.slug, &cat_b.slug, "slug", conflicts);
    out.template_id = three_way_eq(
        &cat_o.template_id,
        &cat_a.template_id,
        &cat_b.template_id,
        "template_id",
        conflicts,
    );
    out.theme_id = three_way_eq(
        &cat_o.theme_id,
        &cat_a.theme_id,
        &cat_b.theme_id,
        "theme_id",
        conflicts,
    );
    out.cite_style_id = three_way_eq(
        &cat_o.cite_style_id,
        &cat_a.cite_style_id,
        &cat_b.cite_style_id,
        "cite_style_id",
        conflicts,
    );
    out.language = three_way_eq(
        &cat_o.language,
        &cat_a.language,
        &cat_b.language,
        "language",
        conflicts,
    );

    out.to_bytes()
        .unwrap_or_else(|_| ours.catalog_bytes.clone())
}

fn decode_catalogs(
    base: &SideSnapshot,
    ours: &SideSnapshot,
    theirs: &SideSnapshot,
    conflicts: &mut Vec<String>,
) -> Option<(DocumentCatalog, DocumentCatalog, DocumentCatalog)> {
    if let (Ok(o), Ok(a), Ok(b)) = (
        DocumentCatalog::from_bytes(&base.catalog_bytes),
        DocumentCatalog::from_bytes(&ours.catalog_bytes),
        DocumentCatalog::from_bytes(&theirs.catalog_bytes),
    ) {
        Some((o, a, b))
    } else {
        conflicts.push("catalog metadata changed on both sides".into());
        None
    }
}

fn three_way_pick<'a, K, V>(
    base: &K,
    ours: &K,
    theirs: &K,
    ours_val: &'a V,
    theirs_val: &'a V,
) -> Option<&'a V>
where
    K: Eq + ?Sized,
    V: ?Sized,
{
    if ours == theirs {
        Some(ours_val)
    } else if ours == base {
        Some(theirs_val)
    } else if theirs == base {
        Some(ours_val)
    } else {
        None
    }
}

fn three_way_eq<T: Clone + Eq>(
    base: &T,
    ours: &T,
    theirs: &T,
    field: &str,
    conflicts: &mut Vec<String>,
) -> T {
    if let Some(v) = three_way_pick(base, ours, theirs, ours, theirs) {
        return v.clone();
    }
    conflicts.push(format!("catalog.{field} changed on both sides"));
    ours.clone()
}

fn three_way_prefer_newer(base: &str, ours: &str, theirs: &str) -> String {
    if let Some(v) = three_way_pick(base, ours, theirs, ours, theirs) {
        return v.to_owned();
    }
    // Both bumped independently (typical apply_ops) — keep the later stamp.
    if theirs > ours {
        theirs.to_owned()
    } else {
        ours.to_owned()
    }
}

fn merge_chunks(
    base: &SideSnapshot,
    ours: &SideSnapshot,
    theirs: &SideSnapshot,
    conflicts: &mut Vec<String>,
) -> (BTreeMap<u64, ChunkSnap>, Vec<u64>, Vec<u64>, Vec<u64>) {
    let ids: BTreeSet<u64> = base
        .chunks
        .keys()
        .chain(ours.chunks.keys())
        .chain(theirs.chunks.keys())
        .copied()
        .collect();

    let mut merged = BTreeMap::new();
    let mut from_ours = Vec::new();
    let mut from_theirs = Vec::new();
    let mut unchanged = Vec::new();

    for id in ids {
        let o = base.chunks.get(&id);
        let a = ours.chunks.get(&id);
        let b = theirs.chunks.get(&id);
        match (o, a, b) {
            (Some(o), Some(a), Some(b)) if a.hash == o.hash && b.hash == o.hash => {
                unchanged.push(id);
                merged.insert(id, a.clone());
            }
            (Some(o), Some(a), Some(b)) if a.hash == b.hash => {
                if a.hash == o.hash {
                    unchanged.push(id);
                } else {
                    from_ours.push(id);
                }
                merged.insert(id, a.clone());
            }
            (Some(o), Some(a), Some(b)) if a.hash != o.hash && b.hash == o.hash => {
                from_ours.push(id);
                merged.insert(id, a.clone());
            }
            (Some(o), Some(a), Some(b)) if b.hash != o.hash && a.hash == o.hash => {
                from_theirs.push(id);
                merged.insert(id, b.clone());
            }
            (Some(_), Some(a), Some(b)) if a.hash != b.hash => {
                conflicts.push(format!(
                    "chunk {id}: overlapping edits (ours={}, theirs={})",
                    short_hash(&a.hash),
                    short_hash(&b.hash)
                ));
            }
            (Some(_), Some(a), Some(_)) | (None, Some(a), None) => {
                from_ours.push(id);
                merged.insert(id, a.clone());
            }
            (Some(_) | None, None, None) => {}
            (Some(o), None, Some(b)) if b.hash == o.hash => {}
            (Some(o), Some(a), None) if a.hash == o.hash => {}
            (Some(_), None, Some(b)) => {
                conflicts.push(format!(
                    "chunk {id}: deleted on ours, edited on theirs ({})",
                    short_hash(&b.hash)
                ));
            }
            (Some(_), Some(a), None) => {
                conflicts.push(format!(
                    "chunk {id}: deleted on theirs, edited on ours ({})",
                    short_hash(&a.hash)
                ));
            }
            (None, None, Some(b)) => {
                from_theirs.push(id);
                merged.insert(id, b.clone());
            }
            (None, Some(a), Some(b)) if a.hash == b.hash => {
                from_ours.push(id);
                merged.insert(id, a.clone());
            }
            (None, Some(a), Some(b)) => {
                conflicts.push(format!(
                    "chunk {id}: both sides added different content (ours={}, theirs={})",
                    short_hash(&a.hash),
                    short_hash(&b.hash)
                ));
            }
        }
    }

    (merged, from_ours, from_theirs, unchanged)
}

fn short_hash(hash: &str) -> &str {
    hash.get(..12).unwrap_or(hash)
}
