//! Layout health checks (`docs/layout_v0.md` — *File health*, `docs/cli.md` — `tes verify`).
//!
//! Unlike [`crate::catalog::file::TesFile`], which fails on the first structural
//! error, verification collects **findings** so a corrupt file yields a full
//! report. `tes verify` exits `1` when any finding is an error.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::catalog::document::DocumentCatalog;
use crate::catalog::index::{
    ChunkIndexEntry, ChunkIndexHeader, Codec, ENTRY_LEN, HEADER_LEN, MAGIC as TIDX_MAGIC,
};
use crate::catalog::link::read_link_table;
use crate::error::Result;
use crate::layout::{self, MAGIC as TESS_MAGIC, Region, SUPERBLOCK_LEN};

/// Severity of a single verification finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// File violates the v0 spec; `tes verify` exits 1.
    Error,
    /// Suspicious but readable.
    Warning,
    /// Informational note.
    Info,
}

/// One check outcome.
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    /// Machine-readable check id (e.g. `superblock.magic`).
    pub check: String,
    /// Severity of the outcome.
    pub severity: Severity,
    /// Human-readable explanation.
    pub message: String,
}

impl Finding {
    fn error(check: &str, message: impl Into<String>) -> Self {
        Self {
            check: check.to_owned(),
            severity: Severity::Error,
            message: message.into(),
        }
    }

    fn warning(check: &str, message: impl Into<String>) -> Self {
        Self {
            check: check.to_owned(),
            severity: Severity::Warning,
            message: message.into(),
        }
    }
}

/// Full verification report for one `.tes` file.
#[derive(Debug, Clone, Serialize)]
pub struct TesVerifyReport {
    /// Path that was checked.
    pub path: String,
    /// File length in bytes.
    pub file_len: u64,
    /// Whether every check passed (no [`Severity::Error`] findings).
    pub ok: bool,
    /// Number of chunk index rows parsed (best effort).
    pub chunk_count: u64,
    /// Whether payload bytes were decoded (`--deep`).
    pub deep: bool,
    /// All findings, in check order.
    pub findings: Vec<Finding>,
}

impl TesVerifyReport {
    /// Findings with [`Severity::Error`].
    #[must_use]
    pub fn errors(&self) -> Vec<&Finding> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .collect()
    }
}

/// Verify the `.tes` file at `path`.
///
/// `deep` additionally decodes every payload (codec + UTF-8 validation for text).
pub fn verify_tes_file(path: impl AsRef<Path>, deep: bool) -> Result<TesVerifyReport> {
    let path = path.as_ref().to_path_buf();
    let mmap = layout::open_mmap(&path)?;
    Ok(verify_bytes(path, &mmap, deep))
}

/// Verify an in-memory image of a `.tes` file.
#[must_use]
pub fn verify_bytes(path: PathBuf, bytes: &[u8], deep: bool) -> TesVerifyReport {
    let file_len = bytes.len() as u64;
    let mut findings = Vec::new();
    let mut chunk_count = 0u64;

    // 1. Superblock: size, magic, version, region bounds.
    if bytes.len() < SUPERBLOCK_LEN {
        findings.push(Finding::error(
            "superblock.size",
            format!(
                "file is {} bytes; superblock needs {SUPERBLOCK_LEN}",
                bytes.len()
            ),
        ));
        return finish(path, file_len, chunk_count, deep, findings);
    }
    if bytes[0..4] != TESS_MAGIC {
        findings.push(Finding::error(
            "superblock.magic",
            format!("expected {TESS_MAGIC:?}, found {:?}", &bytes[0..4]),
        ));
        return finish(path, file_len, chunk_count, deep, findings);
    }

    let superblock = match crate::layout::SuperblockV0::from_bytes(bytes) {
        Ok(sb) => sb,
        Err(err) => {
            findings.push(Finding::error("superblock.parse", err.to_string()));
            return finish(path, file_len, chunk_count, deep, findings);
        }
    };

    check_region_bounds(&mut findings, "catalog", superblock.catalog, file_len);
    check_region_bounds(&mut findings, "link_table", superblock.link_table, file_len);
    check_region_bounds(
        &mut findings,
        "chunk_index",
        superblock.chunk_index,
        file_len,
    );

    if superblock.catalog.length == 0 && superblock.catalog.offset != 0 {
        findings.push(Finding::warning(
            "catalog.offset",
            "catalog_length is 0 but catalog_offset is non-zero",
        ));
    }

    // 2. Catalog JSON parse + required keys.
    if superblock.catalog.is_present() {
        match superblock.catalog.slice(bytes, "catalog") {
            Ok(slice) => match DocumentCatalog::from_bytes(slice) {
                Ok(cat) => {
                    if cat.doc_id.is_empty() {
                        findings.push(Finding::error("catalog.doc_id", "doc_id is empty"));
                    }
                    if cat.doc_kind != superblock.doc_kind.as_str() {
                        findings.push(Finding::warning(
                            "catalog.doc_kind",
                            format!(
                                "catalog doc_kind '{}' differs from superblock '{}'",
                                cat.doc_kind,
                                superblock.doc_kind.as_str()
                            ),
                        ));
                    }
                }
                Err(err) => {
                    findings.push(Finding::error("catalog.json", err.to_string()));
                }
            },
            Err(err) => findings.push(Finding::error("catalog.bounds", err.to_string())),
        }
    }

    // 3. Link table magic, version, and fixed-row bounds.
    if superblock.link_table.is_present() {
        match superblock.link_table.slice(bytes, "link_table") {
            Ok(region) => {
                if let Err(err) = read_link_table(region) {
                    findings.push(Finding::error("link_table.parse", err.to_string()));
                }
            }
            Err(err) => findings.push(Finding::error("link_table.bounds", err.to_string())),
        }
    }

    // 4. Chunk index magic, version, and length arithmetic.
    let mut entries: Vec<ChunkIndexEntry> = Vec::new();
    if superblock.chunk_index.is_present() {
        match superblock.chunk_index.slice(bytes, "chunk_index") {
            Ok(region) => {
                if region.len() < HEADER_LEN {
                    findings.push(Finding::error(
                        "chunk_index.size",
                        format!(
                            "region is {} bytes; header needs {HEADER_LEN}",
                            region.len()
                        ),
                    ));
                } else if region[0..4] != TIDX_MAGIC {
                    findings.push(Finding::error(
                        "chunk_index.magic",
                        format!("expected {TIDX_MAGIC:?}, found {:?}", &region[0..4]),
                    ));
                } else {
                    match ChunkIndexHeader::from_bytes(region) {
                        Ok(header) => {
                            chunk_count = header.entry_count;
                            let expected = header.region_len();
                            if expected != region.len() as u64 {
                                findings.push(Finding::error(
                                    "chunk_index.length",
                                    format!(
                                        "header implies {expected} bytes (32 + {}×48), region is {}",
                                        header.entry_count,
                                        region.len()
                                    ),
                                ));
                            } else {
                                for i in 0..header.entry_count as usize {
                                    let start = HEADER_LEN + i * ENTRY_LEN;
                                    match ChunkIndexEntry::from_bytes(&region[start..]) {
                                        Ok(entry) => entries.push(entry),
                                        Err(err) => findings.push(Finding::error(
                                            "chunk_index.entry",
                                            format!("row {i}: {err}"),
                                        )),
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            findings.push(Finding::error("chunk_index.header", err.to_string()));
                        }
                    }
                }
            }
            Err(err) => findings.push(Finding::error("chunk_index.bounds", err.to_string())),
        }
    } else if superblock.chunk_index.length == 0 {
        // Valid empty skeleton; nothing to check.
    }

    // 5. Payload bounds (+ optional decode).
    let usable_len = file_len; // THST footer handling lands with history support.
    for entry in &entries {
        let check_id = "chunk.payload_bounds";
        match entry.payload_offset.checked_add(entry.stored_byte_len) {
            None => findings.push(Finding::error(
                check_id,
                format!(
                    "chunk {} offset {} + length {} overflows u64",
                    entry.chunk_id, entry.payload_offset, entry.stored_byte_len
                ),
            )),
            Some(end) if end > usable_len => findings.push(Finding::error(
                check_id,
                format!(
                    "chunk {} spans {}..{} beyond file_len {usable_len}",
                    entry.chunk_id, entry.payload_offset, end
                ),
            )),
            Some(end) => {
                if entry.codec == Codec::Raw && entry.stored_byte_len != entry.raw_byte_len {
                    findings.push(Finding::warning(
                        "chunk.codec",
                        format!(
                            "chunk {} is raw but stored_byte_len {} != raw_byte_len {}",
                            entry.chunk_id, entry.stored_byte_len, entry.raw_byte_len
                        ),
                    ));
                }
                if deep {
                    verify_payload_decode(
                        &mut findings,
                        entry,
                        &bytes[entry.payload_offset as usize..end as usize],
                    );
                }
            }
        }
    }

    verify_figure_targets(&mut findings, &entries, bytes);

    // 6. History footer flag.
    if superblock.has_history_footer()
        && (file_len < 16 || &bytes[file_len as usize - 4..] != b"THST")
    {
        findings.push(Finding::error(
            "history.footer",
            "flags set HISTORY_FOOTER but THST magic missing at EOF",
        ));
    }

    finish(path, file_len, chunk_count, deep, findings)
}

fn verify_payload_decode(findings: &mut Vec<Finding>, entry: &ChunkIndexEntry, payload: &[u8]) {
    use crate::catalog::chunk::decode_text_payload;
    use crate::catalog::index::ChunkType;
    use crate::catalog::media::{FigureRef, ImagePayload};

    match entry.codec {
        Codec::Raw => {}
        Codec::Zstd => {
            match zstd::decode_all(payload) {
                Ok(raw) if raw.len() as u64 != entry.raw_byte_len => findings.push(Finding::error(
                    "chunk.decode",
                    format!(
                        "chunk {} zstd decoded to {} bytes, index says {}",
                        entry.chunk_id,
                        raw.len(),
                        entry.raw_byte_len
                    ),
                )),
                Ok(_) => {}
                Err(err) => findings.push(Finding::error(
                    "chunk.decode",
                    format!("chunk {} zstd decode failed: {err}", entry.chunk_id),
                )),
            }
            return;
        }
    }

    match entry.chunk_type {
        ChunkType::Text => {
            if let Err(err) = decode_text_payload(payload) {
                findings.push(Finding::error(
                    "chunk.text_payload",
                    format!("chunk {}: {err}", entry.chunk_id),
                ));
            }
        }
        ChunkType::Image => {
            if let Err(err) = ImagePayload::from_bytes(payload) {
                findings.push(Finding::error(
                    "chunk.image_payload",
                    format!("chunk {}: {err}", entry.chunk_id),
                ));
            }
        }
        ChunkType::Figure => {
            if let Err(err) = FigureRef::from_bytes(payload) {
                findings.push(Finding::error(
                    "chunk.figure_payload",
                    format!("chunk {}: {err}", entry.chunk_id),
                ));
            }
        }
        _ => {}
    }
}

fn verify_figure_targets(findings: &mut Vec<Finding>, entries: &[ChunkIndexEntry], bytes: &[u8]) {
    use crate::catalog::index::ChunkType;
    use crate::catalog::media::FigureRef;
    use std::collections::HashMap;

    let by_id: HashMap<u64, &ChunkIndexEntry> = entries.iter().map(|e| (e.chunk_id, e)).collect();

    for entry in entries {
        if entry.chunk_type != ChunkType::Figure {
            continue;
        }
        let start = entry.payload_offset as usize;
        let end = start + entry.stored_byte_len as usize;
        if end > bytes.len() {
            continue;
        }
        let Ok(figure) = FigureRef::from_bytes(&bytes[start..end]) else {
            continue;
        };
        match by_id.get(&figure.image_chunk_id) {
            None => findings.push(Finding::error(
                "figure.target",
                format!(
                    "figure {} references missing image chunk {}",
                    entry.chunk_id, figure.image_chunk_id
                ),
            )),
            Some(target) if target.chunk_type != ChunkType::Image => findings.push(Finding::error(
                "figure.target",
                format!(
                    "figure {} references chunk {} of type '{}', expected image",
                    entry.chunk_id,
                    figure.image_chunk_id,
                    target.chunk_type.as_str()
                ),
            )),
            Some(_) => {}
        }
        if entry.chunk_flags & crate::catalog::index::chunk_flags::READING_ORDER == 0 {
            findings.push(Finding::warning(
                "figure.reading_order",
                format!(
                    "figure {} is not marked reading-order; exports may skip it",
                    entry.chunk_id
                ),
            ));
        }
    }
}

fn check_region_bounds(
    findings: &mut Vec<Finding>,
    name: &'static str,
    region: Region,
    file_len: u64,
) {
    if !region.is_present() {
        return;
    }
    match region.offset.checked_add(region.length) {
        None => findings.push(Finding::error(
            "region.bounds",
            format!("{name} offset + length overflows u64"),
        )),
        Some(end) if end > file_len => findings.push(Finding::error(
            "region.bounds",
            format!(
                "{name} spans {}..{end} beyond file_len {file_len}",
                region.offset
            ),
        )),
        Some(_) => {
            if !region.offset.is_multiple_of(8) {
                findings.push(Finding::warning(
                    "region.align",
                    format!("{name} offset {} is not 8-byte aligned", region.offset),
                ));
            }
        }
    }
}

fn finish(
    path: PathBuf,
    file_len: u64,
    chunk_count: u64,
    deep: bool,
    findings: Vec<Finding>,
) -> TesVerifyReport {
    let ok = !findings.iter().any(|f| f.severity == Severity::Error);
    TesVerifyReport {
        path: path.display().to_string(),
        file_len,
        ok,
        chunk_count,
        deep,
        findings,
    }
}

/// Human-readable checklist for `tes verify`.
#[must_use]
pub fn format_verify_human(report: &TesVerifyReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "path:    {}", report.path);
    let _ = writeln!(out, "bytes:   {}", report.file_len);
    let _ = writeln!(out, "chunks:  {}", report.chunk_count);
    let _ = writeln!(
        out,
        "mode:    {}",
        if report.deep { "deep" } else { "basic" }
    );
    if report.findings.is_empty() {
        let _ = writeln!(out, "findings: none");
    } else {
        let _ = writeln!(out, "findings:");
        for f in &report.findings {
            let tag = match f.severity {
                Severity::Error => "ERROR",
                Severity::Warning => "WARN ",
                Severity::Info => "INFO ",
            };
            let _ = writeln!(out, "  [{tag}] {}: {}", f.check, f.message);
        }
    }
    let _ = writeln!(out, "status:  {}", if report.ok { "ok" } else { "failed" });
    out
}

/// One-line quiet form: `status=ok` or `status=failed`.
#[must_use]
pub fn format_verify_quiet(report: &TesVerifyReport) -> String {
    format!("status={}", if report.ok { "ok" } else { "failed" })
}

/// JSON report for `tes verify --json`.
pub fn format_verify_json(report: &TesVerifyReport) -> Result<String> {
    Ok(serde_json::to_string_pretty(report)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{DocumentCatalog, TesWriterSession, TextHeader};
    use crate::layout::DocKind;

    fn note_bytes() -> Vec<u8> {
        let mut s = TesWriterSession::create("note.tes", DocKind::Note);
        s.set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440000",
            "Meeting notes",
            "2026-06-05T12:00:00Z",
            "2026-06-05T12:30:00Z",
            DocKind::Note,
        ))
        .unwrap();
        s.add_text_chunk(&TextHeader::paragraph(), "Hello from Tessera.")
            .unwrap();
        s.encode_file().unwrap()
    }

    fn p() -> PathBuf {
        PathBuf::from("mem.tes")
    }

    #[test]
    fn valid_note_passes_basic_and_deep() {
        let bytes = note_bytes();
        let basic = verify_bytes(p(), &bytes, false);
        assert!(basic.ok, "{:?}", basic.findings);
        assert_eq!(basic.chunk_count, 1);

        let deep = verify_bytes(p(), &bytes, true);
        assert!(deep.ok, "{:?}", deep.findings);
        assert!(deep.deep);
    }

    #[test]
    fn empty_skeleton_passes() {
        let bytes = TesWriterSession::create("empty.tes", DocKind::Note)
            .encode_file()
            .unwrap();
        let report = verify_bytes(p(), &bytes, true);
        assert!(report.ok, "{:?}", report.findings);
        assert_eq!(report.chunk_count, 0);
    }

    #[test]
    fn truncated_file_fails() {
        let bytes = note_bytes();
        let report = verify_bytes(p(), &bytes[..bytes.len() - 10], false);
        assert!(!report.ok);
        assert!(report.errors().iter().any(|f| f.check.starts_with("chunk")));
    }

    #[test]
    fn too_short_for_superblock_fails() {
        let report = verify_bytes(p(), &[0u8; 10], false);
        assert!(!report.ok);
        assert_eq!(report.errors()[0].check, "superblock.size");
    }

    #[test]
    fn bad_magic_fails() {
        let mut bytes = note_bytes();
        bytes[0] = b'X';
        let report = verify_bytes(p(), &bytes, false);
        assert!(!report.ok);
        assert_eq!(report.errors()[0].check, "superblock.magic");
    }

    #[test]
    fn corrupt_index_magic_fails() {
        let mut bytes = note_bytes();
        let sb = crate::layout::SuperblockV0::from_bytes(&bytes).unwrap();
        let off = sb.chunk_index.offset as usize;
        bytes[off] = b'Z';
        let report = verify_bytes(p(), &bytes, false);
        assert!(!report.ok);
        assert_eq!(report.errors()[0].check, "chunk_index.magic");
    }

    #[test]
    fn corrupt_catalog_json_fails() {
        let mut bytes = note_bytes();
        let sb = crate::layout::SuperblockV0::from_bytes(&bytes).unwrap();
        bytes[sb.catalog.offset as usize] = b'?';
        let report = verify_bytes(p(), &bytes, false);
        assert!(!report.ok);
        assert_eq!(report.errors()[0].check, "catalog.json");
    }

    #[test]
    fn history_flag_without_footer_fails() {
        let mut bytes = note_bytes();
        // Set flags bit 1 (HISTORY_FOOTER) at offset 8.
        bytes[8..12].copy_from_slice(&1u32.to_le_bytes());
        let report = verify_bytes(p(), &bytes, false);
        assert!(!report.ok);
        assert!(report.errors().iter().any(|f| f.check == "history.footer"));
    }

    #[test]
    fn formatters_render() {
        let bytes = note_bytes();
        let report = verify_bytes(p(), &bytes, false);
        assert!(format_verify_human(&report).contains("status:  ok"));
        assert_eq!(format_verify_quiet(&report), "status=ok");
        assert!(
            format_verify_json(&report)
                .unwrap()
                .contains("\"ok\": true")
        );
    }
}
