//! Cite chunk JSON payload (type `4`).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Result, TesError};
use crate::io::bib::BibEntry;

/// Cite chunk JSON payload (type `4`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CitePayload {
    /// Quoted text.
    pub quote: String,
    /// Target document UUID string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_doc_id: Option<String>,
    /// Target chunk id (`0` / absent = whole document).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_chunk_id: Option<u64>,
    /// Inclusive/exclusive byte range on the target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_byte_start: Option<u32>,
    /// Exclusive end of the target byte range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_byte_end: Option<u32>,
    /// Citation label (e.g. `Smith2024`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Optional page number from an imported PDF.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Optional bibliographic source (interchange metadata; not a display style).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<BibEntry>,
}

impl CitePayload {
    /// Parse a cite payload from UTF-8 JSON bytes.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::Json`] on malformed JSON, or validation errors from
    /// [`Self::validate`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let cite: Self = serde_json::from_slice(bytes)?;
        cite.validate()?;
        Ok(cite)
    }

    /// Serialize to UTF-8 JSON bytes after validation.
    ///
    /// # Errors
    ///
    /// Returns validation errors from [`Self::validate`], or [`TesError::Json`]
    /// if serialization fails.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        Ok(serde_json::to_vec(self)?)
    }

    /// Reject inconsistent ranges and malformed target UUIDs.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::InvalidCite`] for inverted byte ranges or an empty
    /// `source.cite_key`, or [`TesError::InvalidDocId`] if `target_doc_id` is
    /// not a UUID.
    pub fn validate(&self) -> Result<()> {
        if let (Some(start), Some(end)) = (self.target_byte_start, self.target_byte_end)
            && start >= end
        {
            return Err(TesError::InvalidCite {
                message: format!("target_byte_start ({start}) must be < target_byte_end ({end})"),
            });
        }
        if let Some(doc_id) = self.target_doc_id.as_deref()
            && Uuid::parse_str(doc_id).is_err()
        {
            return Err(TesError::InvalidDocId {
                value: doc_id.to_owned(),
            });
        }
        if let Some(source) = &self.source
            && source.cite_key.trim().is_empty()
        {
            return Err(TesError::InvalidCite {
                message: "source.cite_key must be non-empty".into(),
            });
        }
        Ok(())
    }
}
