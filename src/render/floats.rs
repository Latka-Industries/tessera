//! List of figures / list of tables collection (THI-395).
//!
//! Sealed [`TextRole::Lof`](crate::catalog::TextRole::Lof) /
//! [`TextRole::Lot`](crate::catalog::TextRole::Lot) are live markers;
//! print/HTML expand them from figure/table labels (default: **title** only).

use crate::catalog::chunk::{FloatListSource, TextHeader, TextRole};
use crate::catalog::file::TesFile;
use crate::catalog::index::{ChunkIndexEntry, ChunkType};
use crate::error::Result;
use crate::io::export::{decode_figure_entry, decode_text_entry, escape_html};

/// Raw float label fields before LOF/LOT `source=` filtering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloatCandidate {
    /// Source chunk id (HTML `#chunk-N` / print `f-N` / `t-N` dest).
    pub chunk_id: u64,
    /// Figure/table title (above the float).
    pub title: Option<String>,
    /// Figure/table caption (below the float).
    pub caption: Option<String>,
}

/// One float included in a LOF / LOT after `source=` filtering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloatEntry {
    /// 1-based number among included entries for this list.
    pub number: u32,
    /// Label text from title or caption per [`FloatListSource`].
    pub text: String,
    /// Source chunk id (HTML `#chunk-N` / print `f-N` / `t-N` dest).
    pub chunk_id: u64,
}

/// Collect figure title/caption fields (no filtering yet).
///
/// # Errors
///
/// Returns decode errors from figure chunk payloads.
pub fn collect_figures(file: &TesFile, entries: &[&ChunkIndexEntry]) -> Result<Vec<FloatCandidate>> {
    let mut out = Vec::new();
    for entry in entries {
        if entry.chunk_type != ChunkType::Figure {
            continue;
        }
        let figure = decode_figure_entry(file, entry)?;
        out.push(FloatCandidate {
            chunk_id: entry.chunk_id,
            title: nonempty_owned(figure.title.as_deref()),
            caption: nonempty_owned(figure.caption.as_deref()),
        });
    }
    Ok(out)
}

/// Collect table title/caption fields (no filtering yet).
///
/// # Errors
///
/// Returns decode errors from text chunk payloads.
pub fn collect_tables(file: &TesFile, entries: &[&ChunkIndexEntry]) -> Result<Vec<FloatCandidate>> {
    let mut out = Vec::new();
    for entry in entries {
        if entry.chunk_type != ChunkType::Text {
            continue;
        }
        let (header, _) = decode_text_entry(file, entry)?;
        if header.role != TextRole::Table {
            continue;
        }
        out.push(FloatCandidate {
            chunk_id: entry.chunk_id,
            title: nonempty_owned(header.title.as_deref()),
            caption: nonempty_owned(header.caption.as_deref()),
        });
    }
    Ok(out)
}

/// Filter candidates by a LOF/LOT marker's `source=` knob and number them.
#[must_use]
pub fn select_float_entries(
    candidates: &[FloatCandidate],
    source: FloatListSource,
) -> Vec<FloatEntry> {
    let mut out = Vec::new();
    let mut number = 0_u32;
    for c in candidates {
        let Some(text) = float_label(c, source) else {
            continue;
        };
        number = number.saturating_add(1);
        out.push(FloatEntry {
            number,
            text,
            chunk_id: c.chunk_id,
        });
    }
    out
}

fn nonempty_owned(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned)
}

fn float_label(c: &FloatCandidate, source: FloatListSource) -> Option<String> {
    match source {
        FloatListSource::Title => c.title.clone(),
        FloatListSource::Caption => c.caption.clone(),
    }
}

/// Expand a sealed LOF / LOT marker to HTML.
#[must_use]
pub fn expand_float_list_html(
    chunk_id: u64,
    header: &TextHeader,
    candidates: &[FloatCandidate],
    kind: FloatListKind,
) -> String {
    let entries = select_float_entries(candidates, header.float_list_source_or_default());
    let class = kind.html_class();
    let mut html = format!("  <nav class=\"{class}\" data-chunk-id=\"{chunk_id}\">\n");
    if let Some(title) = header.title.as_deref().filter(|s| !s.is_empty()) {
        html.push_str(&format!("    <h2>{}</h2>\n", escape_html(title)));
    }
    if entries.is_empty() {
        html.push_str("  </nav>\n");
        return html;
    }
    html.push_str("    <ol>\n");
    for entry in entries {
        let label = format!("{} {}. {}", kind.noun(), entry.number, entry.text);
        html.push_str(&format!(
            "      <li><a href=\"#chunk-{}\">{}</a></li>\n",
            entry.chunk_id,
            escape_html(&label)
        ));
    }
    html.push_str("    </ol>\n");
    html.push_str("  </nav>\n");
    html
}

/// LOF vs LOT for shared HTML / print helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatListKind {
    /// List of figures.
    Figures,
    /// List of tables.
    Tables,
}

impl FloatListKind {
    #[must_use]
    pub const fn noun(self) -> &'static str {
        match self {
            Self::Figures => "Figure",
            Self::Tables => "Table",
        }
    }

    #[must_use]
    pub const fn html_class(self) -> &'static str {
        match self {
            Self::Figures => "lof",
            Self::Tables => "lot",
        }
    }

    #[must_use]
    pub const fn dest_prefix(self) -> &'static str {
        match self {
            Self::Figures => "f",
            Self::Tables => "t",
        }
    }
}

/// Print / HTML destination id for a figure or table chunk.
#[must_use]
pub fn float_dest_id(kind: FloatListKind, chunk_id: u64) -> String {
    format!("{}-{chunk_id}", kind.dest_prefix())
}

#[cfg(test)]
mod tests {
    use super::{
        FloatCandidate, FloatListKind, expand_float_list_html, float_dest_id, select_float_entries,
    };
    use crate::catalog::chunk::{FloatListSource, TextHeader};

    #[test]
    fn dest_ids_match_prefixes() {
        assert_eq!(float_dest_id(FloatListKind::Figures, 7), "f-7");
        assert_eq!(float_dest_id(FloatListKind::Tables, 3), "t-3");
    }

    #[test]
    fn select_title_skips_caption_only() {
        let candidates = [
            FloatCandidate {
                chunk_id: 1,
                title: Some("Harbor".into()),
                caption: Some("long caption".into()),
            },
            FloatCandidate {
                chunk_id: 2,
                title: None,
                caption: Some("caption only".into()),
            },
        ];
        let entries = select_float_entries(&candidates, FloatListSource::Title);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].text, "Harbor");
        assert_eq!(entries[0].number, 1);
    }

    #[test]
    fn select_caption_skips_title_only() {
        let candidates = [FloatCandidate {
            chunk_id: 1,
            title: Some("Harbor".into()),
            caption: None,
        }];
        assert!(select_float_entries(&candidates, FloatListSource::Caption).is_empty());
    }

    #[test]
    fn html_lists_entries() {
        let header = TextHeader::lof_titled("Figures");
        let html = expand_float_list_html(
            9,
            &header,
            &[FloatCandidate {
                chunk_id: 4,
                title: Some("A still".into()),
                caption: None,
            }],
            FloatListKind::Figures,
        );
        assert!(html.contains("class=\"lof\""));
        assert!(html.contains("href=\"#chunk-4\""));
        assert!(html.contains("Figure 1. A still"));
        assert!(html.contains("<h2>Figures</h2>"));
    }
}
