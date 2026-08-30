//! Body text → styled [`TextRun`]s (inline spans, cite markers, fonts).

use std::collections::BTreeSet;

use ariadnes_weave::{InlineStyle, TextRun};

use crate::catalog::chunk::{InlineKind, InlineSpan};
use crate::catalog::link::LinkEntry;
use crate::edit::tessprek::extract_inline_fonts_mapped;
use crate::io::cite::CiteProj;

/// Split body into styled runs using half-open UTF-8 span ranges.
///
/// Inline [`InlineKind::Citation`] spans are rewritten to `[n]` / `[@key]`
/// (same projection as HTML/Markdown export) and keep `style.cite`.
/// [`InlineKind::Link`] resolves `link_id` against the document link table into
/// [`TextRun::link_uri`] for native PDF annotations.
pub(crate) fn body_to_runs(
    body: &str,
    spans: &[InlineSpan],
    cite: Option<CiteProj<'_>>,
    links: &[LinkEntry],
) -> Vec<TextRun> {
    let (body, spans) = project_inline_citations(body, spans, cite);
    body_to_runs_projected(&body, &spans, links)
}

/// Table / row cells often still carry Tessprek `\font{id}{…}` / `\icon{…}`
/// scaffolding (not sealed as cell spans). Strip macros and apply
/// [`InlineKind::Font`] before run split, remapping any existing cell spans
/// (links, emphasis) onto the stripped text.
pub(crate) fn cell_to_runs(
    text: &str,
    spans: &[InlineSpan],
    cite: Option<CiteProj<'_>>,
    links: &[LinkEntry],
) -> Vec<TextRun> {
    if !text.contains("\\font{") && !text.contains("\\icon{") {
        return body_to_runs(text, spans, cite, links);
    }
    let Ok(extracted) = extract_inline_fonts_mapped(text) else {
        return body_to_runs(text, spans, cite, links);
    };
    if extracted.pending.is_empty() {
        return body_to_runs(text, spans, cite, links);
    }
    let mut merged: Vec<InlineSpan> = spans
        .iter()
        .filter_map(|span| {
            let (start, end) = extracted.remap_range(span.start, span.end)?;
            Some(InlineSpan {
                start,
                end,
                kind: span.kind.clone(),
            })
        })
        .collect();
    merged.extend(extracted.pending.into_iter().map(|f| InlineSpan {
        start: f.start,
        end: f.end,
        kind: InlineKind::Font { font_id: f.font_id },
    }));
    body_to_runs(&extracted.body, &merged, cite, links)
}

fn project_inline_citations(
    body: &str,
    spans: &[InlineSpan],
    cite: Option<CiteProj<'_>>,
) -> (String, Vec<InlineSpan>) {
    let Some(cite) = cite else {
        return (body.to_owned(), spans.to_vec());
    };

    let mut body = body.to_owned();
    let mut spans = spans.to_vec();
    let mut cite_spans: Vec<_> = spans
        .iter()
        .filter(|s| matches!(s.kind, InlineKind::Citation { .. }))
        .cloned()
        .collect();
    cite_spans.sort_by_key(|s| std::cmp::Reverse(s.start));

    let mut marker_spans = Vec::new();
    for span in cite_spans {
        let InlineKind::Citation { cite_chunk_id } = span.kind else {
            continue;
        };
        let Some((start, end)) = utf8_range(&body, span.start, span.end) else {
            continue;
        };
        let marker = cite.marker(cite_chunk_id);
        let old_len = end - start;
        let new_len = marker.len();
        let delta = new_len as i64 - old_len as i64;
        body.replace_range(start..end, &marker);

        for other in &mut spans {
            if matches!(other.kind, InlineKind::Citation { .. }) {
                continue;
            }
            if (other.start as usize) >= end {
                other.start = u32::try_from(i64::from(other.start) + delta).unwrap_or(other.start);
                other.end = u32::try_from(i64::from(other.end) + delta).unwrap_or(other.end);
            }
        }
        marker_spans.push(InlineSpan {
            start: span.start,
            end: span
                .start
                .saturating_add(u32::try_from(new_len).unwrap_or(u32::MAX)),
            kind: InlineKind::Citation { cite_chunk_id },
        });
    }

    spans.retain(|s| !matches!(s.kind, InlineKind::Citation { .. }));
    spans.extend(marker_spans);
    (body, spans)
}

fn body_to_runs_projected(body: &str, spans: &[InlineSpan], links: &[LinkEntry]) -> Vec<TextRun> {
    if body.is_empty() {
        return Vec::new();
    }
    if spans.is_empty() {
        return vec![TextRun::plain(body)];
    }

    let mut bounds = BTreeSet::new();
    bounds.insert(0usize);
    bounds.insert(body.len());
    for span in spans {
        if let Some((start, end)) = utf8_range(body, span.start, span.end)
            && start < end
        {
            bounds.insert(start);
            bounds.insert(end);
        }
    }
    let bounds: Vec<usize> = bounds.into_iter().collect();
    let mut runs = Vec::new();
    for window in bounds.windows(2) {
        let (a, b) = (window[0], window[1]);
        if a >= b {
            continue;
        }
        let mut style = InlineStyle::default();
        let mut face: Option<String> = None;
        let mut link_uri: Option<String> = None;
        for span in spans {
            let start = span.start as usize;
            let end = span.end as usize;
            if start <= a && end >= b {
                apply_inline_kind(&mut style, &span.kind);
                match &span.kind {
                    InlineKind::Font { font_id } => {
                        // Innermost / last covering Font wins → weave TextRun.face.
                        face = Some(font_id.clone());
                    }
                    InlineKind::Link { link_id } => {
                        if let Some(uri) =
                            links.get(*link_id as usize).and_then(|e| e.external_uri())
                        {
                            link_uri = Some(uri.to_owned());
                            style.link = true;
                        }
                    }
                    _ => {}
                }
            }
        }
        runs.push(TextRun {
            text: body[a..b].to_owned(),
            style,
            face,
            link_uri,
        });
    }
    runs
}

/// Valid half-open UTF-8 byte range inside `body`, or `None` if out of bounds.
fn utf8_range(body: &str, start: u32, end: u32) -> Option<(usize, usize)> {
    let start = start as usize;
    let end = end as usize;
    if end > body.len() || start > end {
        return None;
    }
    if !body.is_char_boundary(start) || !body.is_char_boundary(end) {
        return None;
    }
    Some((start, end))
}

fn apply_inline_kind(style: &mut InlineStyle, kind: &InlineKind) {
    match kind {
        InlineKind::Emphasis | InlineKind::Term | InlineKind::Quote => style.emphasis = true,
        InlineKind::Strong => style.strong = true,
        // Inline math has no weave TextRun channel yet (display Math is a block).
        // Show the latex source in monospace until weave grows inline math runs.
        InlineKind::Code | InlineKind::Math { .. } => style.code = true,
        InlineKind::Link { .. } => style.link = true,
        InlineKind::Citation { .. } => style.cite = true,
        InlineKind::Underline => style.underline = true,
        InlineKind::Font { .. } => {}
    }
}
