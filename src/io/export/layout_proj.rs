//! Shared layout-chunk projections for HTML / Markdown / AI (D24 / THI-364).

use std::fmt::Write as _;

use crate::catalog::layout::{LayoutOp, LayoutPayload, PlaceSkip, RuleWidth, VspaceAmount};
use crate::catalog::{InlineKind, InlineSpan};

use super::common::escape_html;

/// Semantic HTML for a sealed layout chunk (`tes serve` / `--html`).
#[must_use]
pub(super) fn layout_html(chunk_id: u64, layout: &LayoutPayload) -> String {
    let mut out = format!("  <div class=\"tes-layout\" data-chunk-id=\"{chunk_id}\">\n");
    for op in &layout.ops {
        match op {
            LayoutOp::Place {
                skip,
                content,
                spans,
            } => out.push_str(&place_html(skip, content, spans)),
            LayoutOp::Vspace { amount } => out.push_str(&vspace_html(*amount)),
            LayoutOp::Rule { width } => out.push_str(&rule_html(*width)),
        }
    }
    out.push_str("  </div>\n");
    out
}

fn place_html(skip: &PlaceSkip, content: &str, spans: &[InlineSpan]) -> String {
    let inner = apply_place_spans_html(content, spans);
    let (extra_class, data, style) = match skip {
        PlaceSkip::Frac { frac } if frac.bps >= 10_000 => (
            " tes-layout-place-flush",
            format!("data-frac=\"{}\"", escape_html(&frac.tessprek_token())),
            None,
        ),
        PlaceSkip::Frac { frac } => {
            let pct = frac.as_f32() * 100.0;
            (
                "",
                format!("data-frac=\"{}\"", escape_html(&frac.tessprek_token())),
                Some(format!("padding-inline-start: {pct}%;")),
            )
        }
        PlaceSkip::Em { em } => {
            let tok = em.tessprek_token();
            (
                "",
                format!("data-em=\"{}\"", escape_html(&tok)),
                Some(format!("padding-inline-start: {tok}em;")),
            )
        }
    };
    let style_attr = style
        .map(|s| format!(" style=\"{s}\""))
        .unwrap_or_default();
    format!(
        "    <div class=\"tes-layout-place{extra_class}\" {data}{style_attr}>\
         <span class=\"tes-layout-place-label\">{inner}</span></div>\n"
    )
}

fn vspace_html(amount: VspaceAmount) -> String {
    match amount {
        VspaceAmount::Small => vspace_named("small", "0.5em"),
        VspaceAmount::Med => vspace_named("med", "1em"),
        VspaceAmount::Big => vspace_named("big", "2em"),
        VspaceAmount::Em { em } => {
            let tok = em.tessprek_token();
            format!(
                "    <div class=\"tes-layout-vspace\" data-em=\"{}\" style=\"height: {tok}em;\" \
                 aria-hidden=\"true\"></div>\n",
                escape_html(&tok)
            )
        }
    }
}

fn vspace_named(amount: &str, height: &str) -> String {
    format!(
        "    <div class=\"tes-layout-vspace\" data-amount=\"{amount}\" style=\"height: {height};\" \
         aria-hidden=\"true\"></div>\n"
    )
}

fn rule_html(width: RuleWidth) -> String {
    let mut style = String::new();
    let mut data = String::new();
    if let Some(frac) = width.frac {
        let pct = frac.as_f32() * 100.0;
        let _ = write!(style, "width: {pct}%;");
        let _ = write!(
            data,
            " data-frac=\"{}\"",
            escape_html(&frac.tessprek_token())
        );
    }
    if let Some(em) = width.em {
        let tok = em.tessprek_token();
        if style.is_empty() {
            let _ = write!(style, "width: {tok}em;");
        }
        let _ = write!(data, " data-em=\"{}\"", escape_html(&tok));
    }
    if style.is_empty() {
        style.push_str("width: 100%;");
    }
    format!("    <hr class=\"tes-layout-rule\"{data} style=\"{style}\">\n")
}

/// Escape place content; wrap `\font` spans in `<span class="tes-font" data-font="…">`.
fn apply_place_spans_html(body: &str, spans: &[InlineSpan]) -> String {
    let mut fonts: Vec<_> = spans
        .iter()
        .filter(|s| matches!(s.kind, InlineKind::Font { .. }))
        .collect();
    if fonts.is_empty() {
        return escape_html(body);
    }
    fonts.sort_by_key(|s| s.start);
    let mut out = String::new();
    let mut cursor = 0usize;
    for span in fonts {
        let start = span.start as usize;
        let end = span.end as usize;
        if start < cursor || end > body.len() || start > end {
            continue;
        }
        out.push_str(&escape_html(&body[cursor..start]));
        let InlineKind::Font { font_id } = &span.kind else {
            continue;
        };
        let _ = write!(
            out,
            "<span class=\"tes-font\" data-font=\"{}\">{}</span>",
            escape_html(font_id),
            escape_html(&body[start..end])
        );
        cursor = end;
    }
    out.push_str(&escape_html(&body[cursor..]));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::layout::{MeasureFrac, RuleWidth, VspaceAmount};

    fn flush_fixture() -> LayoutPayload {
        LayoutPayload {
            ops: vec![
                LayoutOp::Place {
                    skip: PlaceSkip::Frac {
                        frac: MeasureFrac::FULL,
                    },
                    content: "▸".into(),
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
    fn html_flush_place_and_rule() {
        let html = layout_html(2, &flush_fixture());
        assert!(html.contains("tes-layout-place-flush"), "{html}");
        assert!(html.contains('▸'), "{html}");
        assert!(html.contains("tes-layout-rule"), "{html}");
        assert!(html.contains("data-amount=\"med\""), "{html}");
    }
}
