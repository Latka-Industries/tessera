//! Layout chunk payloads (type `9`) — D24 sealed layout ops.
//!
//! A layout chunk is a reading-order unit of closed `place` / `vspace` / `rule`
//! ops. Paint lives in ariadnes-weave; this module is the Tessera wire +
//! Tessprek-facing shape.

use serde::{Deserialize, Serialize};

use crate::catalog::chunk::{InlineKind, InlineSpan};
use crate::error::{Result, TesError};

/// Soft upper bound on ops per layout chunk.
pub const LAYOUT_OPS_MAX: usize = 64;

/// Soft upper bound on place content UTF-8 bytes.
pub const LAYOUT_CONTENT_MAX: usize = 4096;

/// Layout payload JSON (chunk type `9`, reading-order).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutPayload {
    /// Ordered closed layout ops.
    pub ops: Vec<LayoutOp>,
}

/// One closed layout op (D24).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LayoutOp {
    /// Horizontal skip, then inline content.
    Place {
        /// Skip before content (`frac` of measure or `em`).
        skip: PlaceSkip,
        /// Plain content after Tessprek `\font` / phrase expansion.
        #[serde(default)]
        content: String,
        /// Inline spans over [`Self::Place::content`] (e.g. `\font`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        spans: Vec<InlineSpan>,
    },
    /// Extra vertical air (no measure-`frac`).
    Vspace {
        /// Named step or em distance.
        amount: VspaceAmount,
    },
    /// Horizontal rule across part of the measure.
    Rule {
        /// Rule width (`frac` and/or `em`, summed).
        width: RuleWidth,
    },
}

/// Horizontal skip for [`LayoutOp::Place`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlaceSkip {
    /// Fraction of line measure in reading direction.
    Frac {
        /// Measure fraction (`0..=10_000` bps).
        frac: MeasureFrac,
    },
    /// Skip in body ems.
    Em {
        /// Em distance.
        em: EmAmount,
    },
}

/// Vertical gap for [`LayoutOp::Vspace`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VspaceAmount {
    /// Small step (~0.5 em).
    Small,
    /// Medium step (~1 em).
    Med,
    /// Large step (~2 em).
    Big,
    /// Explicit em distance.
    Em {
        /// Em distance.
        em: EmAmount,
    },
}

/// Rule width for [`LayoutOp::Rule`] — `frac` and/or `em` (widths add).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleWidth {
    /// Fraction of line measure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frac: Option<MeasureFrac>,
    /// Additional width in body ems.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub em: Option<EmAmount>,
}

impl RuleWidth {
    /// Width from measure fraction only.
    #[must_use]
    pub fn frac(frac: MeasureFrac) -> Self {
        Self {
            frac: Some(frac),
            em: None,
        }
    }

    /// Width from em amount only.
    #[must_use]
    pub fn em(em: EmAmount) -> Self {
        Self {
            frac: None,
            em: Some(em),
        }
    }
}

/// Fraction of line measure stored as basis points (`10_000` = 1.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeasureFrac {
    /// Ten-thousandths of full measure (`10_000` = flush / full width).
    pub bps: u16,
}

impl MeasureFrac {
    /// Full measure (`frac = 1`).
    pub const FULL: Self = Self { bps: 10_000 };

    /// Construct from basis points; [`LayoutPayload::validate`] rejects `> 10_000`.
    #[must_use]
    pub const fn from_bps(bps: u16) -> Self {
        Self { bps }
    }

    /// Convert `0.0..=1.0` to nearest bps.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::InvalidLayout`] when outside `0..=1`.
    pub fn try_from_f32(frac: f32) -> Result<Self> {
        if !(0.0..=1.0).contains(&frac) || !frac.is_finite() {
            return Err(TesError::InvalidLayout {
                message: format!("invalid frac {frac} (expected 0..=1)"),
            });
        }
        Ok(Self {
            bps: (frac * 10_000.0).round() as u16,
        })
    }

    /// `bps` as a `0.0..=1.0` factor.
    #[must_use]
    pub fn as_f32(self) -> f32 {
        f32::from(self.bps) / 10_000.0
    }

    /// Compact Tessprek / CSS token (`1`, `0.875`, …).
    #[must_use]
    pub fn tessprek_token(self) -> String {
        format_compact_decimal(self.as_f32(), 4)
    }
}

/// Distance in thousandths of an em (`1000` = 1em).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmAmount {
    /// Thousandths of an em.
    pub milli: i32,
}

impl EmAmount {
    /// One em.
    pub const ONE: Self = Self { milli: 1000 };

    /// Construct from thousandths of an em.
    #[must_use]
    pub const fn from_milli(milli: i32) -> Self {
        Self { milli }
    }

    /// Construct from an em multiple (rounded to milli-ems).
    #[must_use]
    pub fn from_em(em: f32) -> Self {
        Self {
            milli: (em * 1000.0).round() as i32,
        }
    }

    /// Em multiple as `f32`.
    #[must_use]
    pub fn as_em(self) -> f32 {
        self.milli as f32 / 1000.0
    }

    /// Compact Tessprek / CSS token (`1`, `0.5`, …).
    #[must_use]
    pub fn tessprek_token(self) -> String {
        format_compact_decimal(self.as_em(), 3)
    }
}

impl PlaceSkip {
    /// Rough leading-space stand-in for Markdown / AI (`frac` → up to 40 spaces).
    #[must_use]
    pub fn lossy_leading_spaces(self) -> usize {
        match self {
            Self::Frac { frac } => ((frac.as_f32() * 40.0).round() as usize).min(40),
            Self::Em { em } => (em.as_em().max(0.0).round() as usize).min(40),
        }
    }
}

impl VspaceAmount {
    /// Named token for Tessprek / HTML data attrs (`small` / `med` / `big`), or `None` for em.
    #[must_use]
    pub fn named_token(self) -> Option<&'static str> {
        match self {
            Self::Small => Some("small"),
            Self::Med => Some("med"),
            Self::Big => Some("big"),
            Self::Em { .. } => None,
        }
    }
}

impl LayoutPayload {
    /// Lossy plain text for Markdown / AI exports.
    ///
    /// `place` → optional leading spaces + content; `vspace` → blank line;
    /// `rule` → `---`.
    #[must_use]
    pub fn lossy_prose(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        for op in &self.ops {
            match op {
                LayoutOp::Place { skip, content, .. } => {
                    let mut line = " ".repeat(skip.lossy_leading_spaces());
                    line.push_str(content);
                    lines.push(line);
                }
                LayoutOp::Vspace { .. } => {
                    if lines.last().is_some_and(|l| !l.is_empty()) {
                        lines.push(String::new());
                    }
                }
                LayoutOp::Rule { .. } => lines.push("---".into()),
            }
        }
        lines.join("\n")
    }

    /// Validate op count, frac range, and place content/spans.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::InvalidLayout`] when fields violate soft limits or
    /// closed-op invariants.
    pub fn validate(&self) -> Result<()> {
        if self.ops.is_empty() {
            return Err(TesError::InvalidLayout {
                message: "layout must declare at least one op".into(),
            });
        }
        if self.ops.len() > LAYOUT_OPS_MAX {
            return Err(TesError::InvalidLayout {
                message: format!("layout has {} ops (max {LAYOUT_OPS_MAX})", self.ops.len()),
            });
        }
        for (i, op) in self.ops.iter().enumerate() {
            validate_op(op).map_err(|message| TesError::InvalidLayout {
                message: format!("op {i}: {message}"),
            })?;
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

    /// Parse a layout payload from UTF-8 JSON.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::Json`] or validation errors from [`Self::validate`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let layout: Self = serde_json::from_slice(bytes)?;
        layout.validate()?;
        Ok(layout)
    }
}

fn format_compact_decimal(v: f32, max_frac: usize) -> String {
    if (v - v.round()).abs() < 1e-6 {
        format!("{}", v as i32)
    } else {
        let s = format!("{v:.max_frac$}");
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

fn validate_op(op: &LayoutOp) -> std::result::Result<(), String> {
    match op {
        LayoutOp::Place {
            skip,
            content,
            spans,
        } => {
            match skip {
                PlaceSkip::Frac { frac } if frac.bps > 10_000 => {
                    return Err(format!("place frac bps {} exceeds 10_000", frac.bps));
                }
                PlaceSkip::Frac { .. } | PlaceSkip::Em { .. } => {}
            }
            if content.len() > LAYOUT_CONTENT_MAX {
                return Err(format!(
                    "place content length {} exceeds {LAYOUT_CONTENT_MAX}",
                    content.len()
                ));
            }
            if content.contains('\0') {
                return Err("place content must not contain NUL".into());
            }
            for span in spans {
                if span.end > content.len() as u32 || span.start > span.end {
                    return Err(format!(
                        "place span [{}, {}) out of range for content len {}",
                        span.start,
                        span.end,
                        content.len()
                    ));
                }
                if let InlineKind::Font { font_id } = &span.kind
                    && font_id.trim().is_empty()
                {
                    return Err("place font span requires non-empty font_id".into());
                }
            }
            Ok(())
        }
        LayoutOp::Vspace { .. } => Ok(()),
        LayoutOp::Rule { width } => {
            if width.frac.is_none() && width.em.is_none() {
                return Err("rule requires frac and/or em".into());
            }
            if let Some(frac) = width.frac
                && frac.bps > 10_000
            {
                return Err(format!("rule frac bps {} exceeds 10_000", frac.bps));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place_flush(content: &str) -> LayoutPayload {
        LayoutPayload {
            ops: vec![
                LayoutOp::Place {
                    skip: PlaceSkip::Frac {
                        frac: MeasureFrac::FULL,
                    },
                    content: content.into(),
                    spans: vec![],
                },
                LayoutOp::Vspace {
                    amount: VspaceAmount::Med,
                },
                LayoutOp::Rule {
                    width: RuleWidth::frac(MeasureFrac::FULL),
                },
            ],
        }
    }

    #[test]
    fn round_trip_place_flush_fixture() {
        let layout = place_flush("▸");
        let bytes = layout.to_bytes().unwrap();
        assert_eq!(LayoutPayload::from_bytes(&bytes).unwrap(), layout);
    }

    #[test]
    fn rejects_empty_ops() {
        let layout = LayoutPayload { ops: vec![] };
        assert!(matches!(
            layout.to_bytes(),
            Err(TesError::InvalidLayout { .. })
        ));
    }

    #[test]
    fn rejects_frac_over_one() {
        let layout = LayoutPayload {
            ops: vec![LayoutOp::Place {
                skip: PlaceSkip::Frac {
                    frac: MeasureFrac::from_bps(10_001),
                },
                content: String::new(),
                spans: vec![],
            }],
        };
        assert!(matches!(
            layout.to_bytes(),
            Err(TesError::InvalidLayout { .. })
        ));
    }

    #[test]
    fn rejects_rule_without_width() {
        let layout = LayoutPayload {
            ops: vec![LayoutOp::Rule {
                width: RuleWidth {
                    frac: None,
                    em: None,
                },
            }],
        };
        assert!(matches!(
            layout.to_bytes(),
            Err(TesError::InvalidLayout { .. })
        ));
    }

    #[test]
    fn measure_frac_try_from_f32() {
        assert_eq!(MeasureFrac::try_from_f32(0.875).unwrap().bps, 8750);
        assert!(MeasureFrac::try_from_f32(1.5).is_err());
        assert!(MeasureFrac::try_from_f32(-0.1).is_err());
    }

    #[test]
    fn lossy_prose_place_and_rule() {
        let prose = place_flush("▸").lossy_prose();
        assert!(prose.contains('▸'), "{prose}");
        assert!(prose.contains("---"), "{prose}");
    }
}
