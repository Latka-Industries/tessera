//! Optional `THST` history footer (`docs/layout_v0.md` — M10).
//!
//! Envelope (unchanged from v0):
//! ```text
//! history_json | u64 LE len | u32 LE version | "THST"
//! ```
//!
//! `history_version` **1** carries content-addressed revision manifests, an
//! exact-hash payload store, named drafts, and a reserved `pending` ops list.
//! Layout version stays **0**.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Result, TesError};

/// Trailer magic at EOF when the history flag is set.
pub const THST_MAGIC: [u8; 4] = *b"THST";

/// Trailer `history_version` for M10 JSON (`layout_version` remains 0).
pub const HISTORY_VERSION: u32 = 1;

/// Fixed trailer size after JSON: len(u64) + version(u32) + magic(4).
pub const TRAILER_FIXED_LEN: usize = 8 + 4 + 4;

/// Root history document stored in `history_json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryV1 {
    /// Discriminator for the JSON schema.
    pub format: String,
    /// Schema version (must be 1).
    pub version: u32,
    /// Tip revision id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    /// Named draft → revision id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub drafts: BTreeMap<String, String>,
    /// Logical full revisions (oldest first).
    #[serde(default)]
    pub revisions: Vec<Revision>,
    /// Exact-hash content-addressed payload store (`sha256hex` → base64).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub store: BTreeMap<String, String>,
    /// Pending authored [`crate::edit::TesOp`] suggestions (redline later).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending: Vec<serde_json::Value>,
    /// Free-form metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

/// One logical full revision (manifest of chunk id → payload hash).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    /// Stable revision id (`rev_` + short hex).
    pub id: String,
    /// Parent revision id, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// ISO-8601 timestamp.
    pub at: String,
    /// Tool / actor that created the revision.
    pub source: String,
    /// Operation label (`save`, `import`, …).
    pub op: String,
    /// Optional human message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Optional draft name this save updated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
    /// SHA-256 of catalog JSON bytes (empty catalog → empty string hash of `[]`).
    pub catalog_hash: String,
    /// Reading-order and media chunks in this revision.
    pub chunks: Vec<ChunkManifest>,
}

/// Chunk id + type + content hash for structural diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkManifest {
    /// Stable chunk id.
    pub id: u64,
    /// Chunk type name (`text`, `figure`, …).
    #[serde(rename = "type")]
    pub chunk_type: String,
    /// SHA-256 hex of decoded payload bytes.
    pub hash: String,
}

impl HistoryV1 {
    /// Empty history document.
    #[must_use]
    pub fn new() -> Self {
        Self {
            format: "tessera-history".into(),
            version: 1,
            head: None,
            drafts: BTreeMap::new(),
            revisions: Vec::new(),
            store: BTreeMap::new(),
            pending: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }

    /// Validate schema invariants.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::InvalidHistory`] when the document is malformed.
    pub fn validate(&self) -> Result<()> {
        if self.format != "tessera-history" {
            return Err(TesError::InvalidHistory {
                message: format!("unknown history format '{}'", self.format),
            });
        }
        if self.version != 1 {
            return Err(TesError::InvalidHistory {
                message: format!("unsupported history schema version {}", self.version),
            });
        }
        let mut seen = std::collections::BTreeSet::new();
        for rev in &self.revisions {
            if rev.id.is_empty() {
                return Err(TesError::InvalidHistory {
                    message: "revision id must be non-empty".into(),
                });
            }
            if !seen.insert(rev.id.clone()) {
                return Err(TesError::InvalidHistory {
                    message: format!("duplicate revision id '{}'", rev.id),
                });
            }
        }
        let ids: std::collections::BTreeSet<_> =
            self.revisions.iter().map(|r| r.id.as_str()).collect();
        for rev in &self.revisions {
            if let Some(parent) = &rev.parent
                && !ids.contains(parent.as_str())
            {
                return Err(TesError::InvalidHistory {
                    message: format!("revision '{}' parent '{parent}' not found", rev.id),
                });
            }
        }
        if let Some(head) = &self.head
            && !ids.contains(head.as_str())
        {
            return Err(TesError::InvalidHistory {
                message: format!("head '{head}' not found in revisions"),
            });
        }
        for (name, rev_id) in &self.drafts {
            if !ids.contains(rev_id.as_str()) {
                return Err(TesError::InvalidHistory {
                    message: format!("draft '{name}' points at missing revision '{rev_id}'"),
                });
            }
        }
        Ok(())
    }

    /// Look up a revision by id.
    #[must_use]
    pub fn revision(&self, id: &str) -> Option<&Revision> {
        self.revisions.iter().find(|r| r.id == id)
    }

    /// Resolve a draft name or revision id to a revision.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::RevisionNotFound`] if neither matches.
    pub fn resolve(&self, draft_or_rev: &str) -> Result<&Revision> {
        if let Some(rev_id) = self.drafts.get(draft_or_rev) {
            return self
                .revision(rev_id)
                .ok_or_else(|| TesError::RevisionNotFound { id: rev_id.clone() });
        }
        self.revision(draft_or_rev)
            .ok_or_else(|| TesError::RevisionNotFound {
                id: draft_or_rev.to_owned(),
            })
    }

    /// Insert payload bytes into the exact-hash store (no-op if present).
    pub fn put_payload(&mut self, hash: &str, bytes: &[u8]) {
        self.store
            .entry(hash.to_owned())
            .or_insert_with(|| crate::catalog::base64_encode(bytes));
    }

    /// Fetch payload bytes from the store.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::InvalidHistory`] if the hash is missing or base64 is bad.
    pub fn get_payload(&self, hash: &str) -> Result<Vec<u8>> {
        let encoded = self
            .store
            .get(hash)
            .ok_or_else(|| TesError::InvalidHistory {
                message: format!("payload hash '{hash}' missing from store"),
            })?;
        crate::catalog::media::base64_decode(encoded).map_err(|e| TesError::InvalidHistory {
            message: format!("bad store payload for '{hash}': {e}"),
        })
    }
}

impl Default for HistoryV1 {
    fn default() -> Self {
        Self::new()
    }
}

/// SHA-256 hex digest of `bytes`.
#[must_use]
pub fn content_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex_encode(&digest)
}

/// Build a revision id from parent + catalog + chunk hashes.
#[must_use]
pub fn revision_id(parent: Option<&str>, catalog_hash: &str, chunks: &[ChunkManifest]) -> String {
    let mut material = String::new();
    material.push_str(parent.unwrap_or(""));
    material.push('\n');
    material.push_str(catalog_hash);
    material.push('\n');
    for c in chunks {
        let _ = writeln!(material, "{}:{}:{}", c.id, c.chunk_type, c.hash);
    }
    let digest = content_hash(material.as_bytes());
    format!("rev_{}", &digest[..16])
}

/// Encode a history document as a THST footer suffix (JSON + trailer).
///
/// # Errors
///
/// Returns validation or JSON errors.
pub fn encode_footer(history: &HistoryV1) -> Result<Vec<u8>> {
    history.validate()?;
    let json = serde_json::to_vec(history)?;
    let mut out = Vec::with_capacity(json.len() + TRAILER_FIXED_LEN);
    out.extend_from_slice(&json);
    out.extend_from_slice(&(json.len() as u64).to_le_bytes());
    out.extend_from_slice(&HISTORY_VERSION.to_le_bytes());
    out.extend_from_slice(&THST_MAGIC);
    Ok(out)
}

/// Decode a THST footer from the end of `bytes`.
///
/// # Errors
///
/// Returns [`TesError::InvalidHistory`] / [`TesError::BadMagic`] /
/// [`TesError::UnsupportedVersion`] when the trailer is malformed.
pub fn decode_footer(bytes: &[u8]) -> Result<HistoryV1> {
    if bytes.len() < TRAILER_FIXED_LEN {
        return Err(TesError::InvalidHistory {
            message: format!(
                "file too short for THST trailer (need ≥{TRAILER_FIXED_LEN}, got {})",
                bytes.len()
            ),
        });
    }
    let magic_start = bytes.len() - 4;
    let found = [
        bytes[magic_start],
        bytes[magic_start + 1],
        bytes[magic_start + 2],
        bytes[magic_start + 3],
    ];
    if found != THST_MAGIC {
        return Err(TesError::BadMagic {
            structure: "THST",
            expected: THST_MAGIC,
            found,
        });
    }
    let version_start = magic_start - 4;
    let version = u32::from_le_bytes([
        bytes[version_start],
        bytes[version_start + 1],
        bytes[version_start + 2],
        bytes[version_start + 3],
    ]);
    if version != HISTORY_VERSION {
        return Err(TesError::UnsupportedVersion {
            structure: "THST",
            found: version,
            supported: HISTORY_VERSION,
        });
    }
    let len_start = version_start - 8;
    let json_len = u64::from_le_bytes([
        bytes[len_start],
        bytes[len_start + 1],
        bytes[len_start + 2],
        bytes[len_start + 3],
        bytes[len_start + 4],
        bytes[len_start + 5],
        bytes[len_start + 6],
        bytes[len_start + 7],
    ]) as usize;
    if json_len > len_start {
        return Err(TesError::InvalidHistory {
            message: format!("history_json_len {json_len} exceeds available prefix {len_start}"),
        });
    }
    let json_start = len_start - json_len;
    let history: HistoryV1 = serde_json::from_slice(&bytes[json_start..len_start])?;
    history.validate()?;
    Ok(history)
}

/// Byte length of the THST suffix at EOF, if present and well-formed.
///
/// Returns `None` when magic is absent (caller decides based on the flag).
#[must_use]
pub fn footer_suffix_len(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < TRAILER_FIXED_LEN || bytes[bytes.len() - 4..] != THST_MAGIC {
        return None;
    }
    let version_start = bytes.len() - 8;
    let len_start = bytes.len() - 16;
    let json_len = u64::from_le_bytes([
        bytes[len_start],
        bytes[len_start + 1],
        bytes[len_start + 2],
        bytes[len_start + 3],
        bytes[len_start + 4],
        bytes[len_start + 5],
        bytes[len_start + 6],
        bytes[len_start + 7],
    ]) as usize;
    let total = json_len.checked_add(TRAILER_FIXED_LEN)?;
    if total > bytes.len() {
        return None;
    }
    // version check is soft here — verify does the hard fail.
    let _ = version_start;
    Some(total)
}

/// Usable file length excluding a well-formed THST footer (or full length).
#[must_use]
pub fn usable_file_len(bytes: &[u8], has_history_flag: bool) -> u64 {
    if !has_history_flag {
        return bytes.len() as u64;
    }
    match footer_suffix_len(bytes) {
        Some(suffix) => (bytes.len() - suffix) as u64,
        None => bytes.len() as u64,
    }
}

/// Split sealed bytes into body (no footer) + optional history.
///
/// # Errors
///
/// Returns decode errors when the history flag is set but the footer is bad.
pub fn split_body_and_history(
    bytes: &[u8],
    has_history_flag: bool,
) -> Result<(Vec<u8>, Option<HistoryV1>)> {
    if !has_history_flag {
        return Ok((bytes.to_vec(), None));
    }
    let Some(suffix) = footer_suffix_len(bytes) else {
        return Err(TesError::InvalidHistory {
            message: "HISTORY_FOOTER flag set but THST trailer is missing or truncated".into(),
        });
    };
    let body_len = bytes.len() - suffix;
    let history = decode_footer(&bytes[body_len..])?;
    let mut body = bytes[..body_len].to_vec();
    clear_history_flag(&mut body)?;
    Ok((body, Some(history)))
}

/// Append a THST footer to `body`, setting the superblock history flag.
///
/// # Errors
///
/// Returns encode errors from [`encode_footer`] or a too-short body.
pub fn attach_footer(mut body: Vec<u8>, history: &HistoryV1) -> Result<Vec<u8>> {
    set_history_flag(&mut body)?;
    let footer = encode_footer(history)?;
    body.extend_from_slice(&footer);
    Ok(body)
}

fn set_history_flag(body: &mut [u8]) -> Result<()> {
    if body.len() < crate::layout::SUPERBLOCK_LEN {
        return Err(TesError::BufferTooSmall {
            structure: "SuperblockV0",
            need: crate::layout::SUPERBLOCK_LEN,
            got: body.len(),
        });
    }
    let flags = u32::from_le_bytes([body[8], body[9], body[10], body[11]]);
    let flags = flags | crate::layout::flags::HISTORY_FOOTER;
    let enc = flags.to_le_bytes();
    body[8..12].copy_from_slice(&enc);
    Ok(())
}

fn clear_history_flag(body: &mut [u8]) -> Result<()> {
    if body.len() < crate::layout::SUPERBLOCK_LEN {
        return Err(TesError::BufferTooSmall {
            structure: "SuperblockV0",
            need: crate::layout::SUPERBLOCK_LEN,
            got: body.len(),
        });
    }
    let flags = u32::from_le_bytes([body[8], body[9], body[10], body[11]]);
    let flags = flags & !crate::layout::flags::HISTORY_FOOTER;
    let enc = flags.to_le_bytes();
    body[8..12].copy_from_slice(&enc);
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_round_trip() {
        let mut hist = HistoryV1::new();
        let chunks = vec![ChunkManifest {
            id: 1,
            chunk_type: "text".into(),
            hash: content_hash(b"hello"),
        }];
        hist.put_payload(&chunks[0].hash, b"hello");
        let id = revision_id(None, "abc", &chunks);
        hist.revisions.push(Revision {
            id: id.clone(),
            parent: None,
            at: "2026-07-28T00:00:00Z".into(),
            source: "test".into(),
            op: "save".into(),
            message: Some("first".into()),
            draft: Some("main".into()),
            catalog_hash: "abc".into(),
            chunks,
        });
        hist.head = Some(id.clone());
        hist.drafts.insert("main".into(), id);
        let footer = encode_footer(&hist).unwrap();
        assert!(footer.ends_with(b"THST"));
        let decoded = decode_footer(&footer).unwrap();
        assert_eq!(decoded, hist);
        assert_eq!(footer_suffix_len(&footer), Some(footer.len()));
    }

    #[test]
    fn rejects_bad_magic() {
        let err = decode_footer(b"not-a-footer!!!!!!").unwrap_err();
        assert!(matches!(err, TesError::BadMagic { .. }));
    }
}
