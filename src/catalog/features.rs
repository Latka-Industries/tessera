//! Catalog feature flags (`docs/structure_v1.md` — forward compatibility).
//!
//! Documents may declare named features as **optional** (skip with warning if
//! unknown) or **required** (fail if unknown). This build keeps
//! `layout_version = 0`; spans / attachments / external URIs are optional.

use serde::{Deserialize, Serialize};

/// Stable feature id strings used in catalog JSON and writer stamps.
pub mod ids {
    /// Layout-v1 text header fields (spans, table, math, lang, align, code_lang).
    pub const TEXT_SPANS: &str = "text_spans";
    /// Inert `ChunkType::Attachment` payloads.
    pub const ATTACHMENTS: &str = "attachments";
    /// TLNK v1 external URI heap.
    pub const EXTERNAL_URIS: &str = "external_uris";
}

/// Well-known feature ids understood by this build (all optional today).
pub const KNOWN_FEATURES: &[&str] = &[ids::TEXT_SPANS, ids::ATTACHMENTS, ids::EXTERNAL_URIS];

/// Optional-vs-required feature declarations in the document catalog.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FeatureSet {
    /// Features an older reader may skip with a warning.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub optional: Vec<String>,
    /// Features an older reader must reject if unknown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,
}

/// One policy outcome when evaluating a [`FeatureSet`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeaturePolicyFinding {
    /// Unknown id listed under `optional`.
    UnknownOptional(String),
    /// Unknown id listed under `required`.
    UnknownRequired(String),
}

impl FeaturePolicyFinding {
    /// Verify check id (`features.optional` / `features.required`).
    #[must_use]
    pub fn check(&self) -> &'static str {
        match self {
            Self::UnknownOptional(_) => "features.optional",
            Self::UnknownRequired(_) => "features.required",
        }
    }

    /// Human-readable finding message.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::UnknownOptional(name) => {
                format!("unknown optional feature '{name}' (skipped)")
            }
            Self::UnknownRequired(name) => {
                format!("unknown must-understand feature '{name}'")
            }
        }
    }

    /// Whether this finding fails `tes verify`.
    #[must_use]
    pub fn is_error(&self) -> bool {
        matches!(self, Self::UnknownRequired(_))
    }
}

impl FeatureSet {
    /// Empty declarations.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.optional.is_empty() && self.required.is_empty()
    }

    /// Whether `name` is already listed as optional or required.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.optional.iter().any(|n| n == name) || self.required.iter().any(|n| n == name)
    }

    /// Record `name` as optional if not already listed.
    pub fn declare_optional(&mut self, name: &str) {
        if !self.contains(name) {
            self.optional.push(name.to_owned());
        }
    }

    /// Record `name` as required (moves it out of optional if present).
    pub fn declare_required(&mut self, name: &str) {
        self.optional.retain(|n| n != name);
        if !self.required.iter().any(|n| n == name) {
            self.required.push(name.to_owned());
        }
    }

    /// Merge another set (union; required wins over optional for the same name).
    pub fn merge(&mut self, other: &FeatureSet) {
        for name in &other.required {
            self.declare_required(name);
        }
        for name in &other.optional {
            self.declare_optional(name);
        }
    }

    /// Classify unknown feature names for verify / open policy.
    #[must_use]
    pub fn evaluate(&self) -> Vec<FeaturePolicyFinding> {
        let mut out = Vec::new();
        for name in &self.required {
            if !is_known_feature(name) {
                out.push(FeaturePolicyFinding::UnknownRequired(name.clone()));
            }
        }
        for name in &self.optional {
            if !is_known_feature(name) {
                out.push(FeaturePolicyFinding::UnknownOptional(name.clone()));
            }
        }
        out
    }
}

/// Whether this build knows `name`.
#[must_use]
pub fn is_known_feature(name: &str) -> bool {
    KNOWN_FEATURES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_optional_is_silent() {
        let mut set = FeatureSet::default();
        set.declare_optional(ids::TEXT_SPANS);
        assert!(set.evaluate().is_empty());
    }

    #[test]
    fn unknown_optional_warns() {
        let mut set = FeatureSet::default();
        set.declare_optional("future_widget");
        assert_eq!(
            set.evaluate(),
            vec![FeaturePolicyFinding::UnknownOptional(
                "future_widget".into()
            )]
        );
    }

    #[test]
    fn unknown_required_errors() {
        let mut set = FeatureSet::default();
        set.declare_required("encrypted_payload");
        assert_eq!(
            set.evaluate(),
            vec![FeaturePolicyFinding::UnknownRequired(
                "encrypted_payload".into()
            )]
        );
    }
}
