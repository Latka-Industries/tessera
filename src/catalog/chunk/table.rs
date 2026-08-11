//! Structured table payload types and Markdown projection.

use serde::{Deserialize, Serialize};

use super::inline::{InlineSpan, TextAlign};

/// One cell in a structured table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableCell {
    /// Plain cell text.
    #[serde(default)]
    pub text: String,
    /// Optional inline spans over [`Self::text`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<InlineSpan>,
    /// Cell alignment override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<TextAlign>,
    /// Whether this cell is a column/row header.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_header: bool,
    /// Row span (HTML-like); omit or 1 for default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rowspan: Option<u32>,
    /// Column span; omit or 1 for default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colspan: Option<u32>,
}

/// One table row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableRow {
    /// Ordered cells.
    pub cells: Vec<TableCell>,
}

/// Structured table payload stored on the text header when `role = table`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableData {
    /// Ordered rows.
    pub rows: Vec<TableRow>,
}

/// GFM-ish pipe table from structured rows (first row treated as header).
pub(super) fn render_table_markdown_with_links(
    table: &TableData,
    links: &[crate::catalog::LinkEntry],
) -> String {
    if table.rows.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for (i, row) in table.rows.iter().enumerate() {
        out.push('|');
        for cell in &row.cells {
            out.push(' ');
            let rendered = if cell.spans.is_empty() {
                cell.text.clone()
            } else {
                super::inline::apply_spans_markdown(&cell.text, &cell.spans, links)
            };
            out.push_str(rendered.replace('|', "\\|").trim());
            out.push_str(" |");
        }
        out.push('\n');
        if i == 0 {
            out.push('|');
            for _ in &row.cells {
                out.push_str(" --- |");
            }
            out.push('\n');
        }
    }
    out
}
