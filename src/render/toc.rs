//! In-document TOC expansion from heading chunks (THI-390).
//!
//! Sealed [`TextRole::Toc`](crate::catalog::TextRole::Toc) is a live marker;
//! print/HTML expand it from the document's heading structure.

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

/// Expand TOC to a semantic HTML `<nav class="toc">` fragment.
#[must_use]
pub fn expand_toc_html(chunk_id: u64, header: &TextHeader, headings: &[TocHeading]) -> String {
    let included = filter_headings(header, headings);
    let mut html = format!("  <nav class=\"toc\" data-chunk-id=\"{chunk_id}\">\n");
    if let Some(title) = header.title.as_deref().filter(|s| !s.is_empty()) {
        html.push_str(&format!("    <h2>{}</h2>\n", escape_html(title)));
    }
    if included.is_empty() {
        html.push_str("  </nav>\n");
        return html;
    }
    let min_level = included.iter().map(|h| h.level).min().unwrap_or(1);
    html.push_str(&toc_html_list(&included, min_level));
    html.push_str("  </nav>\n");
    html
}

fn toc_html_list(headings: &[&TocHeading], depth: u32) -> String {
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
            html.push_str(&toc_html_list(&headings[start..i], sub));
        }
        if i >= headings.len() || headings[i].level < depth {
            break;
        }
        html.push_str(&format!(
            "      <li>{}",
            escape_html(headings[i].text.as_str())
        ));
        i += 1;
        let child_start = i;
        while i < headings.len() && headings[i].level > depth {
            i += 1;
        }
        if child_start < i {
            html.push('\n');
            html.push_str(&toc_html_list(&headings[child_start..i], depth + 1));
            html.push_str("      ");
        }
        html.push_str("</li>\n");
    }
    html.push_str("    </ul>\n");
    html
}
