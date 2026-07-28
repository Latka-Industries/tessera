//! Document summaries for `tes info` (`docs/cli.md`).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use serde::Serialize;

use crate::catalog::document::DocumentCatalog;
use crate::catalog::file::TesFile;
use crate::catalog::index::Codec;
use crate::error::Result;
use crate::layout::Region;

/// Machine-readable `tes info --json` report.
#[derive(Debug, Clone, Serialize)]
pub struct TesInfoReport {
    /// Path that was opened.
    pub path: String,
    /// File size in bytes.
    pub file_len: u64,
    /// Superblock fields.
    pub superblock: SuperblockInfo,
    /// Parsed catalog, if present.
    pub catalog: Option<DocumentCatalog>,
    /// Chunk index rows (no payload bodies).
    pub chunks: Vec<ChunkInfo>,
    /// Link table entries.
    pub links: Vec<LinkInfo>,
}

/// Superblock projection for JSON output.
#[derive(Debug, Clone, Serialize)]
pub struct SuperblockInfo {
    /// Layout version (always `0` for v0).
    pub layout_version: u32,
    /// Superblock flags bitfield.
    pub flags: u32,
    /// Document kind string.
    pub doc_kind: &'static str,
    /// Catalog region.
    pub catalog: RegionInfo,
    /// Link table region.
    pub link_table: RegionInfo,
    /// Chunk index region.
    pub chunk_index: RegionInfo,
}

/// Offset/length pair for JSON.
#[derive(Debug, Clone, Serialize)]
pub struct RegionInfo {
    /// Byte offset from file start.
    pub offset: u64,
    /// Byte length (`0` = absent).
    pub length: u64,
}

impl From<Region> for RegionInfo {
    fn from(r: Region) -> Self {
        Self {
            offset: r.offset,
            length: r.length,
        }
    }
}

/// One chunk index row for JSON / tables.
#[derive(Debug, Clone, Serialize)]
pub struct ChunkInfo {
    /// Stable chunk id (1-based in the reference writer).
    pub chunk_id: u64,
    /// Chunk type name.
    pub chunk_type: &'static str,
    /// Raw type discriminant.
    pub chunk_type_id: u32,
    /// Chunk flags bitfield.
    pub chunk_flags: u32,
    /// File offset of stored bytes.
    pub payload_offset: u64,
    /// Uncompressed size.
    pub raw_byte_len: u64,
    /// On-disk size.
    pub stored_byte_len: u64,
    /// Codec name (`raw` / `zstd`).
    pub codec: &'static str,
}

impl ChunkInfo {
    fn from_entry(entry: &crate::catalog::index::ChunkIndexEntry) -> Self {
        Self {
            chunk_id: entry.chunk_id,
            chunk_type: entry.chunk_type.as_str(),
            chunk_type_id: entry.chunk_type.as_u32(),
            chunk_flags: entry.chunk_flags,
            payload_offset: entry.payload_offset,
            raw_byte_len: entry.raw_byte_len,
            stored_byte_len: entry.stored_byte_len,
            codec: match entry.codec {
                Codec::Raw => "raw",
                Codec::Zstd => "zstd",
            },
        }
    }
}

/// One link-table row for JSON output.
#[derive(Debug, Clone, Serialize)]
pub struct LinkInfo {
    /// Source chunk containing the anchor.
    pub source_chunk_id: u64,
    /// Anchor byte range.
    pub source_byte_start: u32,
    /// Exclusive anchor end.
    pub source_byte_end: u32,
    /// Target document UUID.
    pub target_doc_id: String,
    /// Target chunk (`0` = whole document).
    pub target_chunk_id: u64,
    /// Link kind.
    pub link_kind: &'static str,
}

/// Build an info report from an open file.
#[must_use]
pub fn info_report(file: &TesFile) -> TesInfoReport {
    let sb = file.superblock();
    TesInfoReport {
        path: file.path().display().to_string(),
        file_len: file.file_len(),
        superblock: SuperblockInfo {
            layout_version: crate::layout::LAYOUT_VERSION,
            flags: sb.flags,
            doc_kind: sb.doc_kind.as_str(),
            catalog: sb.catalog.into(),
            link_table: sb.link_table.into(),
            chunk_index: sb.chunk_index.into(),
        },
        catalog: file.catalog().cloned(),
        chunks: file.chunks().iter().map(ChunkInfo::from_entry).collect(),
        links: file
            .links()
            .iter()
            .map(|link| LinkInfo {
                source_chunk_id: link.source_chunk_id,
                source_byte_start: link.source_byte_start,
                source_byte_end: link.source_byte_end,
                target_doc_id: link.target_uuid().to_string(),
                target_chunk_id: link.target_chunk_id,
                link_kind: link.link_kind.as_str(),
            })
            .collect(),
    }
}

/// Open `path` and return an info report.
///
/// # Errors
///
/// Returns errors from [`TesFile::open`].
pub fn read_summary_v0(path: impl AsRef<Path>) -> Result<TesInfoReport> {
    let file = TesFile::open(path.as_ref())?;
    Ok(info_report(&file))
}

/// Human-readable default `tes info` table.
#[must_use]
pub fn format_info_human(report: &TesInfoReport) -> String {
    let title = report
        .catalog
        .as_ref()
        .map_or("(no catalog)", |c| c.title.as_str());
    let modified = report.catalog.as_ref().map_or("-", |c| c.modified.as_str());
    let counts = chunk_type_counts(&report.chunks);
    let counts_str = format_counts(&counts);

    let mut out = String::new();
    let _ = writeln!(out, "path:      {}", report.path);
    let _ = writeln!(out, "title:     {title}");
    let _ = writeln!(out, "doc_kind:  {}", report.superblock.doc_kind);
    let _ = writeln!(out, "modified:  {modified}");
    let _ = writeln!(out, "chunks:    {} ({counts_str})", report.chunks.len());
    let _ = writeln!(out, "bytes:     {}", report.file_len);
    out
}

/// One-line quiet form: `title\tchunks=N\tbytes=M`.
#[must_use]
pub fn format_info_quiet(report: &TesInfoReport) -> String {
    let title = report
        .catalog
        .as_ref()
        .map_or("(no catalog)", |c| c.title.as_str());
    format!(
        "{title}\tchunks={}\tbytes={}",
        report.chunks.len(),
        report.file_len
    )
}

fn chunk_type_counts(chunks: &[ChunkInfo]) -> BTreeMap<&'static str, usize> {
    let mut map = BTreeMap::new();
    for c in chunks {
        *map.entry(c.chunk_type).or_insert(0) += 1;
    }
    map
}

fn format_counts(counts: &BTreeMap<&'static str, usize>) -> String {
    if counts.is_empty() {
        return "none".to_owned();
    }
    counts
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// JSON string for `tes info --json`.
///
/// # Errors
///
/// Returns [`crate::error::TesError::Json`] if serialization fails.
pub fn format_info_json(report: &TesInfoReport) -> Result<String> {
    Ok(serde_json::to_string_pretty(report)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/v0")
            .join(name)
    }

    #[test]
    fn summary_empty() {
        let report = read_summary_v0(fixture("empty.tes")).unwrap();
        assert_eq!(report.file_len, 64);
        assert!(report.catalog.is_none());
        assert!(report.chunks.is_empty());
        assert_eq!(report.superblock.doc_kind, "note");
        let human = format_info_human(&report);
        assert!(human.contains("chunks:    0"));
    }

    #[test]
    fn summary_note_json_round_fields() {
        let report = read_summary_v0(fixture("note_one_chunk.tes")).unwrap();
        assert_eq!(report.catalog.as_ref().unwrap().title, "Meeting notes");
        assert_eq!(report.chunks.len(), 1);
        assert_eq!(report.chunks[0].chunk_type, "text");
        let json = format_info_json(&report).unwrap();
        assert!(json.contains("\"doc_id\""));
        assert!(json.contains("Meeting notes"));
    }
}
