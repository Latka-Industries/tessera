//! Per-chunk / per-line blame over THST ancestry.

use std::fmt::Write as _;
use std::path::Path;

use crate::catalog::history::{ChunkManifest, HistoryV1, Revision};
use crate::error::{Result, TesError};

use super::read_history;

/// Options for [`blame_file`].
#[derive(Debug, Clone, Default)]
pub struct BlameOptions {
    /// Blame only this chunk id (all chunks when `None`).
    pub chunk: Option<u64>,
    /// Revision id or draft name (defaults to history `head`).
    pub rev: Option<String>,
}

/// One attributed region in a blame report.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BlameRegion {
    /// Chunk id.
    pub chunk_id: u64,
    /// Chunk type name.
    pub chunk_type: String,
    /// 1-based line within the text body (`None` for non-text / whole-chunk rows).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// Revision that introduced this region.
    pub revision_id: String,
    /// Revision timestamp.
    pub at: String,
    /// Tool / actor (`Revision.source`).
    pub source: String,
    /// Optional save message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Line text or a short non-text label.
    pub text: String,
}

/// Blame report for a tip revision.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BlameReport {
    /// Path inspected.
    pub path: String,
    /// Tip revision id blamed.
    pub revision_id: String,
    /// Ordered regions (reading-order chunks, then lines).
    pub regions: Vec<BlameRegion>,
}

/// Attribute current chunk / line text to the revision that last introduced it.
///
/// Defaults to history `head`. Text chunks emit one row per line; other chunk
/// types emit a single whole-chunk row.
///
/// # Errors
///
/// Returns [`TesError::InvalidHistory`] when no revisions / head exist,
/// [`TesError::RevisionNotFound`] for a bad `--rev`, or payload decode errors.
pub fn blame_file(path: impl AsRef<Path>, options: &BlameOptions) -> Result<BlameReport> {
    let path = path.as_ref();
    let history = read_history(path)?;
    if history.revisions.is_empty() {
        return Err(TesError::InvalidHistory {
            message: "no revisions to blame (run `tes save` first)".into(),
        });
    }
    let tip = if let Some(name) = &options.rev {
        history.resolve(name)?
    } else {
        let head = history
            .head
            .as_deref()
            .ok_or_else(|| TesError::InvalidHistory {
                message: "history has revisions but no head".into(),
            })?;
        history
            .revision(head)
            .ok_or_else(|| TesError::RevisionNotFound {
                id: head.to_owned(),
            })?
    };
    let chain = ancestry(&history, tip)?;
    let mut regions = Vec::new();
    for manifest in &tip.chunks {
        if options.chunk.is_some_and(|id| id != manifest.id) {
            continue;
        }
        let introducer = introducing_revision(&chain, manifest.id, &manifest.hash).unwrap_or(tip);
        if manifest.chunk_type == "text" {
            regions.extend(blame_text_lines(&history, &chain, tip, manifest)?);
        } else {
            regions.push(BlameRegion {
                chunk_id: manifest.id,
                chunk_type: manifest.chunk_type.clone(),
                line: None,
                revision_id: introducer.id.clone(),
                at: introducer.at.clone(),
                source: introducer.source.clone(),
                message: introducer.message.clone(),
                text: format!("[{}]", manifest.chunk_type),
            });
        }
    }
    Ok(BlameReport {
        path: path.display().to_string(),
        revision_id: tip.id.clone(),
        regions,
    })
}

/// Format a [`BlameReport`] for CLI output.
#[must_use]
pub fn format_blame(report: &BlameReport) -> String {
    let mut out = format!("# blame tip={}\n", report.revision_id);
    for region in &report.regions {
        let loc = match region.line {
            Some(line) => format!("{}:{line}", region.chunk_id),
            None => format!("{}", region.chunk_id),
        };
        let msg = region.message.as_deref().unwrap_or("");
        let _ = writeln!(
            out,
            "{loc}\t{}\t{}\t{}\t{msg}\t{}",
            region.revision_id, region.at, region.source, region.text
        );
    }
    out
}

/// Format blame as JSON.
///
/// # Errors
///
/// Returns JSON serialization errors.
pub fn format_blame_json(report: &BlameReport) -> Result<String> {
    Ok(serde_json::to_string_pretty(report)?)
}

fn ancestry<'a>(history: &'a HistoryV1, tip: &'a Revision) -> Result<Vec<&'a Revision>> {
    let mut chain = Vec::new();
    let mut current = tip;
    loop {
        chain.push(current);
        let Some(parent_id) = current.parent.as_deref() else {
            break;
        };
        let parent = history
            .revision(parent_id)
            .ok_or_else(|| TesError::RevisionNotFound {
                id: parent_id.to_owned(),
            })?;
        current = parent;
    }
    chain.reverse(); // oldest → tip
    Ok(chain)
}

fn chunk_hash(rev: &Revision, chunk_id: u64) -> Option<&str> {
    rev.chunks
        .iter()
        .find(|c| c.id == chunk_id)
        .map(|c| c.hash.as_str())
}

fn introducing_revision<'a>(
    chain: &[&'a Revision],
    chunk_id: u64,
    tip_hash: &str,
) -> Option<&'a Revision> {
    // Oldest revision whose hash for this chunk equals tip_hash.
    chain
        .iter()
        .copied()
        .find(|rev| chunk_hash(rev, chunk_id) == Some(tip_hash))
}

fn blame_text_lines(
    history: &HistoryV1,
    chain: &[&Revision],
    tip: &Revision,
    manifest: &ChunkManifest,
) -> Result<Vec<BlameRegion>> {
    use crate::catalog::chunk::decode_text_payload;

    let tip_bytes = history.get_payload(&manifest.hash)?;
    let tip_body = decode_text_payload(&tip_bytes).map_or_else(
        |_| String::from_utf8_lossy(&tip_bytes).into_owned(),
        |(_, body)| body,
    );
    let tip_lines: Vec<String> = tip_body.lines().map(str::to_owned).collect();
    if tip_lines.is_empty() {
        let introducer = introducing_revision(chain, manifest.id, &manifest.hash).unwrap_or(tip);
        return Ok(vec![BlameRegion {
            chunk_id: manifest.id,
            chunk_type: manifest.chunk_type.clone(),
            line: Some(1),
            revision_id: introducer.id.clone(),
            at: introducer.at.clone(),
            source: introducer.source.clone(),
            message: introducer.message.clone(),
            text: String::new(),
        }]);
    }

    // Collect bodies for revisions that contain this chunk (oldest → tip).
    let mut versions: Vec<(&Revision, Vec<String>)> = Vec::new();
    for rev in chain {
        let Some(hash) = chunk_hash(rev, manifest.id) else {
            continue;
        };
        let bytes = history.get_payload(hash)?;
        let body = decode_text_payload(&bytes).map_or_else(
            |_| String::from_utf8_lossy(&bytes).into_owned(),
            |(_, body)| body,
        );
        versions.push((rev, body.lines().map(str::to_owned).collect()));
    }
    if versions.is_empty() {
        return Ok(Vec::new());
    }

    let n = tip_lines.len();
    let mut owners: Vec<Option<&Revision>> = vec![None; n];
    // Map tip line index → index in the current (child) body while walking back.
    let mut map: Vec<Option<usize>> = (0..n).map(Some).collect();

    for window in versions.windows(2).rev() {
        let (_parent_rev, parent_lines) = &window[0];
        let (child_rev, child_lines) = &window[1];
        let matching = lcs_child_to_parent(parent_lines, child_lines);
        for (tip_i, slot) in map.iter_mut().enumerate() {
            let Some(child_i) = *slot else {
                continue;
            };
            if let Some(parent_i) = matching.get(child_i).copied().flatten() {
                *slot = Some(parent_i);
            } else {
                owners[tip_i] = Some(*child_rev);
                *slot = None;
            }
        }
    }
    let oldest = versions[0].0;
    for (tip_i, slot) in map.iter().enumerate() {
        if slot.is_some() {
            owners[tip_i] = Some(oldest);
        }
    }

    let mut regions = Vec::with_capacity(n);
    for (i, line) in tip_lines.iter().enumerate() {
        let rev = owners[i].unwrap_or(tip);
        regions.push(BlameRegion {
            chunk_id: manifest.id,
            chunk_type: manifest.chunk_type.clone(),
            line: Some((i + 1) as u32),
            revision_id: rev.id.clone(),
            at: rev.at.clone(),
            source: rev.source.clone(),
            message: rev.message.clone(),
            text: line.clone(),
        });
    }
    Ok(regions)
}

/// For each child line index, the matched parent line index (LCS), or `None` if new.
fn lcs_child_to_parent(parent: &[String], child: &[String]) -> Vec<Option<usize>> {
    let n = parent.len();
    let m = child.len();
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in 0..n {
        for j in 0..m {
            if parent[i] == child[j] {
                dp[i + 1][j + 1] = dp[i][j] + 1;
            } else {
                dp[i + 1][j + 1] = dp[i + 1][j].max(dp[i][j + 1]);
            }
        }
    }
    let mut matching = vec![None; m];
    let mut i = n;
    let mut j = m;
    while i > 0 && j > 0 {
        if parent[i - 1] == child[j - 1] {
            matching[j - 1] = Some(i - 1);
            i -= 1;
            j -= 1;
        } else if dp[i][j - 1] >= dp[i - 1][j] {
            j -= 1;
        } else {
            i -= 1;
        }
    }
    matching
}
