//! Document catalog JSON blob (`docs/layout_v0.md` — Document catalog).

use serde::{Deserialize, Serialize};

use crate::error::{Result, TesError};
use crate::layout::DocKind;

/// Maximum catalog size accepted by the v0 reference writer (16 KiB).
pub const CATALOG_MAX_BYTES: usize = 16 * 1024;

/// UTF-8 JSON document catalog stored at `catalog_offset`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentCatalog {
    /// Stable UUID string used in cross-doc links.
    pub doc_id: String,
    /// Display title.
    pub title: String,
    /// RFC 3339 UTC creation time.
    pub created: String,
    /// RFC 3339 UTC modification time.
    pub modified: String,
    /// String mirror of the superblock [`DocKind`].
    pub doc_kind: String,
    /// Optional tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Optional export / GUI template hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_id: Option<String>,
    /// Optional theme hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_id: Option<String>,
}

impl DocumentCatalog {
    /// Build a catalog for `kind` with the required fields filled in.
    #[must_use]
    pub fn new(
        doc_id: impl Into<String>,
        title: impl Into<String>,
        created: impl Into<String>,
        modified: impl Into<String>,
        kind: DocKind,
    ) -> Self {
        Self {
            doc_id: doc_id.into(),
            title: title.into(),
            created: created.into(),
            modified: modified.into(),
            doc_kind: kind.as_str().to_owned(),
            tags: Vec::new(),
            template_id: None,
            theme_id: None,
        }
    }

    /// Serialize to UTF-8 JSON bytes (no BOM), enforcing the 16 KiB limit.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let bytes = serde_json::to_vec(self)?;
        if bytes.len() > CATALOG_MAX_BYTES {
            return Err(TesError::CatalogTooLarge {
                len: bytes.len(),
                limit: CATALOG_MAX_BYTES,
            });
        }
        Ok(bytes)
    }

    /// Parse a catalog JSON blob.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(serde_json::from_slice(bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_minimal() {
        let cat = DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440000",
            "Meeting notes",
            "2026-06-05T12:00:00Z",
            "2026-06-05T12:30:00Z",
            DocKind::Note,
        );
        let bytes = cat.to_bytes().unwrap();
        assert!(bytes.len() < CATALOG_MAX_BYTES);
        let decoded = DocumentCatalog::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, cat);
        assert_eq!(decoded.doc_kind, "note");
    }

    #[test]
    fn round_trip_with_optional_fields() {
        let mut cat = DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440000",
            "Paper",
            "2026-06-05T12:00:00Z",
            "2026-06-05T12:00:00Z",
            DocKind::Research,
        );
        cat.tags = vec!["ml".into(), "notes".into()];
        cat.template_id = Some("academic".into());
        cat.theme_id = Some("print".into());
        let decoded = DocumentCatalog::from_bytes(&cat.to_bytes().unwrap()).unwrap();
        assert_eq!(decoded, cat);
    }
}
