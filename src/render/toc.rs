//! In-document TOC expansion from heading chunks (THI-390).
//!
//! Sealed [`TextRole::Toc`](crate::catalog::TextRole::Toc) is a live marker;
//! print/HTML expand it from the document's heading structure.

use std::fmt::Write as _;

use crate::catalog::chunk::{TextHeader, TextRole};
use crate::catalog::file::TesFile;
use crate::catalog::index::{ChunkIndexEntry, ChunkType};
use crate::error::Result;
use crate::io::export::decode_text_entry;
use crate::io::export::escape_html;

/// One heading considered for TOC inclusion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocHeading {
    /// Heading level 1–6.
    pub level: u32,
    /// Plain heading text.
    pub text: String,
    /// Source text chunk id (HTML `#chunk-N` / print `h-N` dest).
    pub chunk_id: u64,
}

/// Collect headings from reading-order entries (skips non-text).
///
/// # Errors
///
/// Returns decode errors from text chunk payloads.
pub fn collect_headings(file: &TesFile, entries: &[&ChunkIndexEntry]) -> Result<Vec<TocHeading>> {
    let mut out = Vec::new();
    for entry in entries {
        if entry.chunk_type != ChunkType::Text {
            continue;
        }
        let (header, body) = decode_text_entry(file, entry)?;
        if header.role != TextRole::Heading {
            continue;
        }
        let text = body.trim();
        if text.is_empty() {
            continue;
        }
        out.push(TocHeading {
            level: header.level.unwrap_or(1).clamp(1, 6),
            text: text.to_owned(),
            chunk_id: entry.chunk_id,
        });
    }
    Ok(out)
}

/// Headings included by a TOC chunk's depth knob.
#[must_use]
pub fn filter_headings<'a>(header: &TextHeader, headings: &'a [TocHeading]) -> Vec<&'a TocHeading> {
    let depth = header.toc_depth_or_default();
    headings.iter().filter(|h| h.level <= depth).collect()
}

/// Hierarchical section labels for `headings` in order (`"1"`, `"1.1"`, `"2"`, …).
///
/// Counters walk by level relative to the shallowest heading in the slice so a
/// depth-filtered TOC of only H2s still numbers `1`, `2`, … rather than `0.1`.
#[must_use]
pub fn section_number_labels(headings: &[TocHeading]) -> Vec<String> {
    section_labels_from_levels(headings.iter().map(|h| h.level))
}

fn section_labels_from_levels(levels: impl IntoIterator<Item = u32>) -> Vec<String> {
    let levels: Vec<u32> = levels.into_iter().map(|l| l.clamp(1, 6)).collect();
    if levels.is_empty() {
        return Vec::new();
    }
    let min_level = levels.iter().copied().min().unwrap_or(1) as usize;
    let mut counters = [0u32; 7];
    let mut labels = Vec::with_capacity(levels.len());
    for level in levels {
        let idx = level as usize;
        counters[idx] += 1;
        for c in counters.iter_mut().skip(idx + 1) {
            *c = 0;
        }
        let parts: Vec<String> = (min_level..=idx).map(|i| counters[i].to_string()).collect();
        labels.push(parts.join("."));
    }
    labels
}

/// Expand TOC to a semantic HTML `<nav class="toc">` fragment.
#[must_use]
pub fn expand_toc_html(chunk_id: u64, header: &TextHeader, headings: &[TocHeading]) -> String {
    let included = filter_headings(header, headings);
    let mut html = format!("  <nav class=\"toc\" data-chunk-id=\"{chunk_id}\">\n");
    if let Some(title) = header.title.as_deref().filter(|s| !s.is_empty()) {
        let _ = writeln!(html, "    <h2>{}</h2>", escape_html(title));
    }
    if included.is_empty() {
        html.push_str("  </nav>\n");
        return html;
    }
    let owned: Vec<TocHeading> = included.iter().map(|h| (*h).clone()).collect();
    let labels = section_number_labels(&owned);
    let min_level = owned.iter().map(|h| h.level).min().unwrap_or(1);
    html.push_str(&toc_html_list(&owned, &labels, min_level));
    html.push_str("  </nav>\n");
    html
}

fn toc_html_list(headings: &[TocHeading], labels: &[String], depth: u32) -> String {
    let mut html = String::from("    <ul>\n");
    let mut i = 0;
    while i < headings.len() {
        if headings[i].level < depth {
            break;
        }
        while i < headings.len() && headings[i].level > depth {
            // Orphan deeper than expected — emit at its own level then continue.
            let sub = headings[i].level;
            let start = i;
            i += 1;
            while i < headings.len() && headings[i].level > sub {
                i += 1;
            }
            html.push_str(&toc_html_list(&headings[start..i], &labels[start..i], sub));
        }
        if i >= headings.len() || headings[i].level < depth {
            break;
        }
        let label = labels.get(i).map_or("", String::as_str);
        let title = if label.is_empty() {
            headings[i].text.clone()
        } else {
            format!("{label} {}", headings[i].text)
        };
        let _ = write!(
            html,
            "      <li><a href=\"#chunk-{}\">{}</a>",
            headings[i].chunk_id,
            escape_html(&title)
        );
        i += 1;
        let child_start = i;
        while i < headings.len() && headings[i].level > depth {
            i += 1;
        }
        if child_start < i {
            html.push('\n');
            html.push_str(&toc_html_list(
                &headings[child_start..i],
                &labels[child_start..i],
                depth + 1,
            ));
            html.push_str("      ");
        }
        html.push_str("</li>\n");
    }
    html.push_str("    </ul>\n");
    html
}

#[cfg(test)]
mod tests {
    use super::{TocHeading, section_number_labels};

    #[test]
    fn section_numbers_nested_and_reset() {
        let headings = vec![
            TocHeading {
                level: 1,
                text: "A".into(),
                chunk_id: 1,
            },
            TocHeading {
                level: 2,
                text: "A1".into(),
                chunk_id: 2,
            },
            TocHeading {
                level: 2,
                text: "A2".into(),
                chunk_id: 3,
            },
            TocHeading {
                level: 1,
                text: "B".into(),
                chunk_id: 4,
            },
        ];
        assert_eq!(
            section_number_labels(&headings),
            vec!["1", "1.1", "1.2", "2"]
        );
    }

    #[test]
    fn section_numbers_relative_to_min_level() {
        let headings = vec![
            TocHeading {
                level: 2,
                text: "X".into(),
                chunk_id: 1,
            },
            TocHeading {
                level: 3,
                text: "Y".into(),
                chunk_id: 2,
            },
        ];
        assert_eq!(section_number_labels(&headings), vec!["1", "1.1"]);
    }
}
