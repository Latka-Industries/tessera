//! List of figures / list of tables collection (THI-395).
//!
//! Sealed [`TextRole::Lof`](crate::catalog::TextRole::Lof) /
//! [`TextRole::Lot`](crate::catalog::TextRole::Lot) are live markers;
//! print/HTML expand them from captioned (or titled) figures and tables.

use crate::catalog::chunk::{TextHeader, TextRole};
use crate::catalog::file::TesFile;
use crate::catalog::index::{ChunkIndexEntry, ChunkType};
use crate::error::Result;
use crate::io::export::{decode_figure_entry, decode_text_entry, escape_html};

/// One float considered for LOF / LOT inclusion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloatEntry {
    /// 1-based figure or table number in document order.
    pub number: u32,
    /// Caption text, else title (whichever is non-empty).
    pub text: String,
    /// Source chunk id (HTML `#chunk-N` / print `f-N` / `t-N` dest).
    pub chunk_id: u64,
}

/// Collect figures that have a non-empty title and/or caption.
///
/// # Errors
///
/// Returns decode errors from figure chunk payloads.
pub fn collect_figures(file: &TesFile, entries: &[&ChunkIndexEntry]) -> Result<Vec<FloatEntry>> {
    let mut out = Vec::new();
    let mut number = 0_u32;
    for entry in entries {
        if entry.chunk_type != ChunkType::Figure {
            continue;
        }
        let figure = decode_figure_entry(file, entry)?;
        let text = float_label(figure.caption.as_deref(), figure.title.as_deref());
        let Some(text) = text else {
            continue;
        };
        number = number.saturating_add(1);
        out.push(FloatEntry {
            number,
            text,
            chunk_id: entry.chunk_id,
        });
    }
    Ok(out)
}

/// Collect tables that have a non-empty title and/or caption.
///
/// # Errors
///
/// Returns decode errors from text chunk payloads.
pub fn collect_tables(file: &TesFile, entries: &[&ChunkIndexEntry]) -> Result<Vec<FloatEntry>> {
    let mut out = Vec::new();
    let mut number = 0_u32;
    for entry in entries {
        if entry.chunk_type != ChunkType::Text {
            continue;
        }
        let (header, _) = decode_text_entry(file, entry)?;
        if header.role != TextRole::Table {
            continue;
        }
        let text = float_label(header.caption.as_deref(), header.title.as_deref());
        let Some(text) = text else {
            continue;
        };
        number = number.saturating_add(1);
        out.push(FloatEntry {
            number,
            text,
            chunk_id: entry.chunk_id,
        });
    }
    Ok(out)
}

fn float_label(caption: Option<&str>, title: Option<&str>) -> Option<String> {
    caption
        .or(title)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Expand a sealed LOF / LOT marker to HTML.
#[must_use]
pub fn expand_float_list_html(
    chunk_id: u64,
    header: &TextHeader,
    entries: &[FloatEntry],
    kind: FloatListKind,
) -> String {
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
    use super::{FloatEntry, FloatListKind, expand_float_list_html, float_dest_id};
    use crate::catalog::chunk::TextHeader;

    #[test]
    fn dest_ids_match_prefixes() {
        assert_eq!(float_dest_id(FloatListKind::Figures, 7), "f-7");
        assert_eq!(float_dest_id(FloatListKind::Tables, 3), "t-3");
    }

    #[test]
    fn html_lists_entries() {
        let header = TextHeader::lof_titled("Figures");
        let html = expand_float_list_html(
            9,
            &header,
            &[FloatEntry {
                number: 1,
                text: "A still".into(),
                chunk_id: 4,
            }],
            FloatListKind::Figures,
        );
        assert!(html.contains("class=\"lof\""));
        assert!(html.contains("href=\"#chunk-4\""));
        assert!(html.contains("Figure 1. A still"));
        assert!(html.contains("<h2>Figures</h2>"));
    }
}
