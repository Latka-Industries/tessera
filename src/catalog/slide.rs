//! Slide chunk payloads (type `5`) — region-based decks.
//!
//! A slide stores a `layout_id` plus named region slots that reference other
//! chunks (text / figure / cite / image). Geometry lives in theme CSS
//! (grid/flex), never as freeform `x/y/w/h` in the wire.

use serde::{Deserialize, Serialize};

use crate::error::{Result, TesError};

/// Soft upper bound on layout / region name lengths.
pub const SLIDE_STRING_MAX: usize = 128;

/// Soft upper bound on regions per slide.
pub const SLIDE_REGIONS_MAX: usize = 32;

/// One named region slot on a slide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlideRegion {
    /// Template region name (`title`, `body`, `media`, …).
    pub name: String,
    /// Target chunk id in this file (text, figure, cite, or image).
    pub chunk_id: u64,
}

/// Slide payload JSON (chunk type `5`, reading-order).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlidePayload {
    /// Layout id understood by the template pack (e.g. `title_body`).
    pub layout_id: String,
    /// Ordered named region → chunk refs.
    pub regions: Vec<SlideRegion>,
}

impl SlidePayload {
    /// Convenience builder for the common `title_body` layout.
    #[must_use]
    pub fn title_body(title_chunk_id: u64, body_chunk_id: u64) -> Self {
        Self {
            layout_id: "title_body".into(),
            regions: vec![
                SlideRegion {
                    name: "title".into(),
                    chunk_id: title_chunk_id,
                },
                SlideRegion {
                    name: "body".into(),
                    chunk_id: body_chunk_id,
                },
            ],
        }
    }

    /// Validate layout id, region names, and bounds.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::InvalidSlide`] when fields violate soft limits or
    /// required invariants.
    pub fn validate(&self) -> Result<()> {
        let layout = self.layout_id.trim();
        if layout.is_empty() || self.layout_id.len() > SLIDE_STRING_MAX {
            return Err(TesError::InvalidSlide {
                message: format!(
                    "layout_id length {} out of range 1..={SLIDE_STRING_MAX}",
                    self.layout_id.len()
                ),
            });
        }
        if self.layout_id.contains(['\0', '\n', '\r']) {
            return Err(TesError::InvalidSlide {
                message: "layout_id must be a single-line token".into(),
            });
        }
        if self.regions.is_empty() {
            return Err(TesError::InvalidSlide {
                message: "slide must declare at least one region".into(),
            });
        }
        if self.regions.len() > SLIDE_REGIONS_MAX {
            return Err(TesError::InvalidSlide {
                message: format!(
                    "slide has {} regions (max {SLIDE_REGIONS_MAX})",
                    self.regions.len()
                ),
            });
        }
        let mut seen = std::collections::BTreeSet::new();
        for region in &self.regions {
            let name = region.name.trim();
            if name.is_empty() || region.name.len() > SLIDE_STRING_MAX {
                return Err(TesError::InvalidSlide {
                    message: format!(
                        "region name length {} out of range 1..={SLIDE_STRING_MAX}",
                        region.name.len()
                    ),
                });
            }
            if region.chunk_id == 0 {
                return Err(TesError::InvalidSlide {
                    message: format!("region '{name}' chunk_id must be non-zero"),
                });
            }
            if !seen.insert(region.name.clone()) {
                return Err(TesError::InvalidSlide {
                    message: format!("duplicate region name '{name}'"),
                });
            }
        }
        Ok(())
    }

    /// Serialize as UTF-8 JSON.
    ///
    /// # Errors
    ///
    /// Returns validation errors from [`Self::validate`], or [`TesError::Json`].
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        Ok(serde_json::to_vec(self)?)
    }

    /// Parse a slide payload from UTF-8 JSON.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::Json`] or validation errors from [`Self::validate`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let slide: Self = serde_json::from_slice(bytes)?;
        slide.validate()?;
        Ok(slide)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_title_body() {
        let slide = SlidePayload::title_body(1, 2);
        let bytes = slide.to_bytes().unwrap();
        assert_eq!(SlidePayload::from_bytes(&bytes).unwrap(), slide);
    }

    #[test]
    fn rejects_empty_regions() {
        let slide = SlidePayload {
            layout_id: "title_body".into(),
            regions: vec![],
        };
        assert!(matches!(
            slide.to_bytes(),
            Err(TesError::InvalidSlide { .. })
        ));
    }

    #[test]
    fn rejects_duplicate_region_names() {
        let slide = SlidePayload {
            layout_id: "title_body".into(),
            regions: vec![
                SlideRegion {
                    name: "title".into(),
                    chunk_id: 1,
                },
                SlideRegion {
                    name: "title".into(),
                    chunk_id: 2,
                },
            ],
        };
        assert!(matches!(
            slide.to_bytes(),
            Err(TesError::InvalidSlide { .. })
        ));
    }
}
