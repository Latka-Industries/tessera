use std::fs;
use std::path::{Path, PathBuf};

use crate::catalog::file::TesFile;
use crate::catalog::history::{
    ChunkManifest, HistoryV1, Revision, attach_footer, content_hash, revision_id,
    split_body_and_history,
};
use crate::error::Result;
use crate::layout::SuperblockV0;

use super::{atomic_replace, chrono_like_now};

/// Options for [`save_revision`].
#[derive(Debug, Clone, Default)]
pub struct SaveOptions {
    /// Named draft to update (also recorded on the revision).
    pub draft: Option<String>,
    /// Human message.
    pub message: Option<String>,
    /// Tool / actor label (default `tes save`).
    pub source: Option<String>,
}

/// Result of appending a revision.
#[derive(Debug, Clone)]
pub struct SaveReport {
    /// Path written.
    pub path: PathBuf,
    /// New revision id.
    pub revision_id: String,
    /// Draft name, if any.
    pub draft: Option<String>,
    /// Total revisions now stored.
    pub revision_count: usize,
}

/// Append a content-addressed revision of the current sealed body to `path`.
///
/// # Errors
///
/// Returns I/O, decode, or history validation errors.
pub fn save_revision(path: impl AsRef<Path>, options: &SaveOptions) -> Result<SaveReport> {
    let path = path.as_ref();
    let bytes = fs::read(path)?;
    let sb = SuperblockV0::from_bytes(&bytes)?;
    let (body, existing) = split_body_and_history(&bytes, sb.has_history_footer())?;
    let mut history = existing.unwrap_or_else(HistoryV1::new);

    // Snapshot from a temporary open of the body (no footer).
    let tmp = path.with_extension("tes.save-body");
    fs::write(&tmp, &body)?;
    let snapshot = match snapshot_live(&tmp, &mut history) {
        Ok(s) => {
            let _ = fs::remove_file(&tmp);
            s
        }
        Err(err) => {
            let _ = fs::remove_file(&tmp);
            return Err(err);
        }
    };

    let parent = history.head.clone();
    // Content-identical to tip → refresh draft pointer only (no new revision).
    if let Some(head_id) = history.head.as_deref()
        && let Some(head) = history.revision(head_id)
        && head.catalog_hash == snapshot.catalog_hash
        && head.chunks == snapshot.chunks
    {
        let id = head_id.to_owned();
        if let Some(draft) = &options.draft {
            history.drafts.insert(draft.clone(), id.clone());
        }
        let out = attach_footer(body, &history)?;
        atomic_replace(path, &out)?;
        return Ok(SaveReport {
            path: path.to_path_buf(),
            revision_id: id,
            draft: options.draft.clone(),
            revision_count: history.revisions.len(),
        });
    }

    let id = revision_id(parent.as_deref(), &snapshot.catalog_hash, &snapshot.chunks);
    let at = chrono_like_now();
    let rev = Revision {
        id: id.clone(),
        parent,
        at,
        source: options.source.clone().unwrap_or_else(|| "tes save".into()),
        op: "save".into(),
        message: options.message.clone(),
        draft: options.draft.clone(),
        catalog_hash: snapshot.catalog_hash,
        chunks: snapshot.chunks,
    };
    history.revisions.push(rev);
    history.head = Some(id.clone());
    if let Some(draft) = &options.draft {
        history.drafts.insert(draft.clone(), id.clone());
    }
    history.validate()?;

    let out = attach_footer(body, &history)?;
    atomic_replace(path, &out)?;
    Ok(SaveReport {
        path: path.to_path_buf(),
        revision_id: id,
        draft: options.draft.clone(),
        revision_count: history.revisions.len(),
    })
}

struct LiveSnapshot {
    catalog_hash: String,
    chunks: Vec<ChunkManifest>,
}

fn snapshot_live(path: &Path, history: &mut HistoryV1) -> Result<LiveSnapshot> {
    let file = TesFile::open(path)?;
    let catalog_bytes = match file.catalog() {
        Some(cat) => cat.to_bytes()?,
        None => Vec::new(),
    };
    let catalog_hash = content_hash(&catalog_bytes);
    history.put_payload(&catalog_hash, &catalog_bytes);

    let mut chunks = Vec::new();
    for entry in file.chunks() {
        let payload = file.decode_payload(entry)?;
        let hash = content_hash(payload.as_ref());
        history.put_payload(&hash, payload.as_ref());
        chunks.push(ChunkManifest {
            id: entry.chunk_id,
            chunk_type: entry.chunk_type.as_str().to_owned(),
            hash,
        });
    }
    Ok(LiveSnapshot {
        catalog_hash,
        chunks,
    })
}
