//! Text-chunk semantic header: roles, list numbering, validation, Markdown render.

use serde::{Deserialize, Serialize};

use crate::error::{Result, TesError};

use super::inline::{InlineSpan, TextAlign, apply_spans_markdown, validate_spans};
use super::table::{TableCell, TableData, render_cell_markdown, render_table_markdown_with_links};

/// Maximum text-chunk semantic header size (4 KiB).
pub const TEXT_HEADER_MAX_BYTES: usize = 4 * 1024;

/// Max UTF-8 bytes for optional title / caption on table / math / `code_block`.
pub const TEXT_CAPTION_MAX: usize = 1024;

/// Semantic role of a text chunk body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextRole {
    /// Body paragraph.
    Paragraph,
    /// Heading; use with [`TextHeader::level`].
    Heading,
    /// List item; use with [`TextHeader::list_kind`].
    ListItem,
    /// Block quote / pull quote.
    Blockquote,
    /// Monospace block; optional [`TextHeader::code_lang`].
    CodeBlock,
    /// Table: prefer [`TextHeader::table`]; v0 TSV body remains accepted.
    Table,
    /// Meta row (Tessprek `\row{…}{…}…`); panes in [`TextHeader::panes`].
    Row,
    /// Display math; body is LaTeX source.
    Math,
    /// In-document table of contents (Tessprek `\toc` / `\toc{…}`; THI-390).
    ///
    /// Live marker: print/HTML expand from heading chunks. Body is empty.
    /// Not vault/hub nav.
    Toc,
    /// In-document list of figures (Tessprek `\lof` / `\lof{…}`; THI-395).
    ///
    /// Live marker: print/HTML expand from captioned/titled figures. Body empty.
    Lof,
    /// In-document list of tables (Tessprek `\lot` / `\lot{…}`; THI-395).
    ///
    /// Live marker: print/HTML expand from captioned/titled tables. Body empty.
    Lot,
    /// Multi-column body region open (Tessprek `\columns` / `\columns{…}`; THI-391).
    ///
    /// Empty body marker. Distinct from [`TextRole::Row`] (meta hfill panes).
    /// Print folds following chunks until [`TextRole::ColumnsEnd`] into
    /// weave `PrintBlock::Columns`.
    Columns,
    /// Multi-column body region close (Tessprek `\endcolumns`; THI-391).
    ///
    /// Empty body end marker pairing [`TextRole::Columns`].
    ColumnsEnd,
}

impl TextRole {
    /// Lowercase role name used in JSONL / debug headers.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Paragraph => "paragraph",
            Self::Heading => "heading",
            Self::ListItem => "list_item",
            Self::Blockquote => "blockquote",
            Self::CodeBlock => "code_block",
            Self::Table => "table",
            Self::Row => "row",
            Self::Math => "math",
            Self::Toc => "toc",
            Self::Lof => "lof",
            Self::Lot => "lot",
            Self::Columns => "columns",
            Self::ColumnsEnd => "columns_end",
        }
    }
}

/// List marker kind for [`TextRole::ListItem`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListKind {
    /// Unordered bullet.
    Bullet,
    /// Ordered / numbered.
    Ordered,
}

/// Sequential `1.` / `2.` / … markers for contiguous ordered list runs.
///
/// Tessera stores each list item as its own chunk with no stored index;
/// projections (Tessprek, Markdown export) use this to avoid every item
/// rendering as `1.`.
#[derive(Debug, Default, Clone)]
pub struct OrderedListNumbering {
    /// Next index is `stack[depth - 1]` after increment (depth is 1-based).
    stack: Vec<u32>,
}

impl OrderedListNumbering {
    /// Next marker for an ordered item at `depth` (1 = top-level).
    #[must_use]
    pub fn next(&mut self, depth: u32) -> u32 {
        let d = usize::try_from(depth.max(1)).unwrap_or(1);
        self.stack.truncate(d);
        while self.stack.len() < d {
            self.stack.push(0);
        }
        self.stack[d - 1] = self.stack[d - 1].saturating_add(1);
        self.stack[d - 1]
    }

    /// Reset when leaving an ordered run (bullet item, non-list chunk, …).
    pub fn clear(&mut self) {
        self.stack.clear();
    }

    /// Marker index when `header` is an ordered list item; otherwise clears
    /// state and returns `None`.
    pub fn take_for_text(&mut self, header: &TextHeader) -> Option<u32> {
        if header.role != TextRole::ListItem {
            self.clear();
            return None;
        }
        match header.list_kind.unwrap_or(ListKind::Bullet) {
            ListKind::Ordered => Some(self.next(header.list_depth_or_default())),
            ListKind::Bullet => {
                self.clear();
                None
            }
        }
    }
}

/// JSON header prefixed to a type-`1` text chunk payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextHeader {
    /// Semantic role of the body.
    pub role: TextRole,
    /// Heading level 1–6 when `role` is [`TextRole::Heading`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    /// List marker when `role` is [`TextRole::ListItem`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_kind: Option<ListKind>,
    /// Nesting depth for [`TextRole::ListItem`] (1 = top-level). Absent means 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_depth: Option<u32>,
    /// Print band indent level (0 = content margin). Points = `level ×` profile step.
    ///
    /// Orthogonal to [`Self::list_depth`]: nested bullets keep the same band and
    /// only deepen the list. Tessprek: `\block{indent=N}` before the chunk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indent: Option<u32>,
    /// Legacy string emphasis tags (prefer [`Self::spans`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emphasis: Vec<String>,
    /// Theme hints imported from semantic HTML `class` attributes.
    #[serde(default, rename = "class", skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<String>,
    /// Ranged inline formatting over the UTF-8 body.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<InlineSpan>,
    /// Optional BCP-47 language override for this block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    /// Optional semantic alignment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<TextAlign>,
    /// Optional programming language when `role` is [`TextRole::CodeBlock`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_lang: Option<String>,
    /// Optional title above a table, math, or code block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Optional caption under a table, math, or code block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// Structured table when `role` is [`TextRole::Table`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<TableData>,
    /// Ordered panes when `role` is [`TextRole::Row`] (Tessprek `\row{…}{…}`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panes: Option<Vec<TableCell>>,
    /// Max heading level (1–6) included when `role` is [`TextRole::Toc`].
    /// Absent means 3 (H1–H3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toc_depth: Option<u32>,
    /// Page numbers on TOC / LOF / LOT lines when `role` is [`TextRole::Toc`],
    /// [`TextRole::Lof`], or [`TextRole::Lot`].
    /// Absent means **on** for print (`toc_pages_or_default`); weave resolves
    /// digits from destinations. Set `false` to omit the page column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toc_pages: Option<bool>,
    /// Section numbers (`1`, `1.1`, …) on TOC lines when `role` is [`TextRole::Toc`].
    /// Absent means **on** (`toc_sections_or_default`). When on, nested levels
    /// also get band indent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toc_sections: Option<bool>,
    /// Dotted leaders between title and page when `role` is [`TextRole::Toc`],
    /// [`TextRole::Lof`], or [`TextRole::Lot`].
    /// Absent means **on** (`toc_leaders_or_default`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toc_leaders: Option<bool>,
    /// Column count when `role` is [`TextRole::Columns`]. Absent means 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns_count: Option<u8>,
    /// Gap between columns in points when `role` is [`TextRole::Columns`].
    /// Absent → weave pack `[body_columns].gap`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns_gap: Option<u16>,
}

impl TextHeader {
    /// Empty optional fields for `role`.
    #[must_use]
    pub fn with_role(role: TextRole) -> Self {
        Self {
            role,
            level: None,
            list_kind: None,
            list_depth: None,
            indent: None,
            emphasis: Vec::new(),
            classes: Vec::new(),
            spans: Vec::new(),
            lang: None,
            align: None,
            code_lang: None,
            title: None,
            caption: None,
            table: None,
            panes: None,
            toc_depth: None,
            toc_pages: None,
            toc_sections: None,
            toc_leaders: None,
            columns_count: None,
            columns_gap: None,
        }
    }

    /// A plain paragraph header.
    #[must_use]
    pub fn paragraph() -> Self {
        Self::with_role(TextRole::Paragraph)
    }

    /// In-document TOC marker (empty body; expand at print/HTML).
    #[must_use]
    pub fn toc() -> Self {
        Self::with_role(TextRole::Toc)
    }

    /// In-document list-of-figures marker (empty body; expand at print/HTML).
    #[must_use]
    pub fn lof() -> Self {
        Self::with_role(TextRole::Lof)
    }

    /// List of figures with an optional title above the list.
    #[must_use]
    pub fn lof_titled(title: impl Into<String>) -> Self {
        let mut h = Self::lof();
        h.title = Some(title.into());
        h
    }

    /// In-document list-of-tables marker (empty body; expand at print/HTML).
    #[must_use]
    pub fn lot() -> Self {
        Self::with_role(TextRole::Lot)
    }

    /// List of tables with an optional title above the list.
    #[must_use]
    pub fn lot_titled(title: impl Into<String>) -> Self {
        let mut h = Self::lot();
        h.title = Some(title.into());
        h
    }

    /// Multi-column body open marker (empty body; default 2 columns).
    #[must_use]
    pub fn columns() -> Self {
        Self::with_role(TextRole::Columns)
    }

    /// Multi-column body open with count and optional gap (points).
    #[must_use]
    pub fn columns_with(count: u8, gap: Option<u16>) -> Self {
        let mut h = Self::columns();
        h.columns_count = Some(count.clamp(1, 6));
        h.columns_gap = gap;
        h
    }

    /// Multi-column body close marker (empty body).
    #[must_use]
    pub fn columns_end() -> Self {
        Self::with_role(TextRole::ColumnsEnd)
    }

    /// Effective column count (absent → 2).
    #[must_use]
    pub fn columns_count_or_default(&self) -> u8 {
        self.columns_count.unwrap_or(2).clamp(1, 6)
    }

    /// TOC with optional title and max heading depth.
    #[must_use]
    pub fn toc_titled(title: impl Into<String>, depth: u32) -> Self {
        let mut h = Self::toc();
        h.title = Some(title.into());
        if depth != 3 {
            h.toc_depth = Some(depth.clamp(1, 6));
        }
        h
    }

    /// Effective TOC max heading depth (absent → 3).
    #[must_use]
    pub fn toc_depth_or_default(&self) -> u32 {
        self.toc_depth.unwrap_or(3).clamp(1, 6)
    }

    /// Whether print TOC should show a page column (absent → `true`).
    #[must_use]
    pub fn toc_pages_or_default(&self) -> bool {
        self.toc_pages.unwrap_or(true)
    }

    /// Whether TOC lines get hierarchical section numbers (absent → `true`).
    #[must_use]
    pub fn toc_sections_or_default(&self) -> bool {
        self.toc_sections.unwrap_or(true)
    }

    /// Whether TOC lines get dotted leaders (absent → `true`).
    #[must_use]
    pub fn toc_leaders_or_default(&self) -> bool {
        self.toc_leaders.unwrap_or(true)
    }

    /// Whether this header uses additive layout-v1 fields (`text_spans` feature).
    #[must_use]
    pub fn uses_layout_v1_features(&self) -> bool {
        !self.spans.is_empty()
            || self.lang.is_some()
            || self.align.is_some()
            || self.code_lang.is_some()
            || self.title.is_some()
            || self.caption.is_some()
            || self.table.is_some()
            || self.panes.is_some()
            || self.toc_depth.is_some()
            || self.toc_pages.is_some()
            || self.toc_sections.is_some()
            || self.toc_leaders.is_some()
            || self.columns_count.is_some()
            || self.columns_gap.is_some()
            || self.list_depth.is_some_and(|d| d > 1)
            || self.indent.is_some_and(|n| n > 0)
            || matches!(
                self.role,
                TextRole::Table
                    | TextRole::Row
                    | TextRole::Math
                    | TextRole::Toc
                    | TextRole::Lof
                    | TextRole::Lot
                    | TextRole::Columns
                    | TextRole::ColumnsEnd
            )
    }

    /// Effective print band indent level (absent → 0).
    #[must_use]
    pub fn indent_or_default(&self) -> u32 {
        self.indent.unwrap_or(0).min(16)
    }

    /// A heading header at `level` (1–6).
    #[must_use]
    pub fn heading(level: u32) -> Self {
        let mut h = Self::with_role(TextRole::Heading);
        h.level = Some(level);
        h
    }

    /// A list-item header at depth 1 (top-level).
    #[must_use]
    pub fn list_item(kind: ListKind) -> Self {
        Self::list_item_at(kind, 1)
    }

    /// A list-item header at `depth` (1 = top-level, 2 = one nest, …).
    #[must_use]
    pub fn list_item_at(kind: ListKind, depth: u32) -> Self {
        let mut h = Self::with_role(TextRole::ListItem);
        h.list_kind = Some(kind);
        if depth > 1 {
            h.list_depth = Some(depth);
        }
        h
    }

    /// Effective list nesting depth (defaults to 1).
    #[must_use]
    pub fn list_depth_or_default(&self) -> u32 {
        self.list_depth.unwrap_or(1).clamp(1, 16)
    }

    /// Indent + `- ` / `N. ` prefix for a list item (empty when not a list item).
    #[must_use]
    pub fn list_marker_prefix(&self, ordered_index: Option<u32>) -> String {
        if self.role != TextRole::ListItem {
            return String::new();
        }
        let indent = "  ".repeat(self.list_depth_or_default().saturating_sub(1) as usize);
        match self.list_kind.unwrap_or(ListKind::Bullet) {
            ListKind::Bullet => format!("{indent}- "),
            ListKind::Ordered => {
                format!("{indent}{}. ", ordered_index.unwrap_or(1))
            }
        }
    }

    /// A code-block header with optional fence language.
    #[must_use]
    pub fn code_block(code_lang: Option<&str>) -> Self {
        let mut h = Self::with_role(TextRole::CodeBlock);
        h.code_lang = code_lang.map(str::to_owned).filter(|s| !s.is_empty());
        h
    }

    /// A display-math header (body is LaTeX).
    #[must_use]
    pub fn math() -> Self {
        Self::with_role(TextRole::Math)
    }

    /// A structured table header.
    #[must_use]
    pub fn table(data: TableData) -> Self {
        let mut h = Self::with_role(TextRole::Table);
        h.table = Some(data);
        h
    }

    /// A meta-row header (`\row{…}{…}`).
    #[must_use]
    pub fn row(panes: Vec<TableCell>) -> Self {
        let mut h = Self::with_role(TextRole::Row);
        h.panes = Some(panes);
        h
    }

    fn validate_block_label(&self, name: &str, value: Option<&str>) -> Result<()> {
        let Some(value) = value else {
            return Ok(());
        };
        if value.is_empty() {
            return Err(TesError::InvalidTextHeader {
                message: format!("{name} must be non-empty when set"),
            });
        }
        if value.len() > TEXT_CAPTION_MAX {
            return Err(TesError::InvalidTextHeader {
                message: format!("{name} exceeds {TEXT_CAPTION_MAX} bytes"),
            });
        }
        let ok = match name {
            "title" => matches!(
                self.role,
                TextRole::Table
                    | TextRole::Math
                    | TextRole::CodeBlock
                    | TextRole::Toc
                    | TextRole::Lof
                    | TextRole::Lot
            ),
            _ => matches!(
                self.role,
                TextRole::Table | TextRole::Math | TextRole::CodeBlock
            ),
        };
        if !ok {
            return Err(TesError::InvalidTextHeader {
                message: format!(
                    "{name} is only valid on table, math, or code_block (title also on toc/lof/lot)"
                ),
            });
        }
        Ok(())
    }

    /// Validate spans/table fields against `body`.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::InvalidTextHeader`] when ranges are empty, inverted,
    /// out of bounds, not on UTF-8 character boundaries, or when table cell
    /// spans are invalid.
    pub fn validate(&self, body: &str) -> Result<()> {
        validate_spans(body, &self.spans)?;
        if let Some(table) = &self.table {
            if self.role != TextRole::Table {
                return Err(TesError::InvalidTextHeader {
                    message: "table payload requires role=table".into(),
                });
            }
            for (ri, row) in table.rows.iter().enumerate() {
                for (ci, cell) in row.cells.iter().enumerate() {
                    validate_spans(&cell.text, &cell.spans).map_err(|e| match e {
                        TesError::InvalidTextHeader { message } => TesError::InvalidTextHeader {
                            message: format!("table[{ri}][{ci}]: {message}"),
                        },
                        other => other,
                    })?;
                    if matches!(cell.rowspan, Some(0)) || matches!(cell.colspan, Some(0)) {
                        return Err(TesError::InvalidTextHeader {
                            message: format!("table[{ri}][{ci}]: rowspan/colspan must be >= 1"),
                        });
                    }
                }
            }
        }
        if let Some(panes) = &self.panes {
            if self.role != TextRole::Row {
                return Err(TesError::InvalidTextHeader {
                    message: "panes payload requires role=row".into(),
                });
            }
            if panes.len() < 2 {
                return Err(TesError::InvalidTextHeader {
                    message: "row requires at least 2 panes".into(),
                });
            }
            for (i, pane) in panes.iter().enumerate() {
                validate_spans(&pane.text, &pane.spans).map_err(|e| match e {
                    TesError::InvalidTextHeader { message } => TesError::InvalidTextHeader {
                        message: format!("row pane[{i}]: {message}"),
                    },
                    other => other,
                })?;
            }
        } else if self.role == TextRole::Row {
            return Err(TesError::InvalidTextHeader {
                message: "role=row requires panes".into(),
            });
        }
        if self.code_lang.is_some() && self.role != TextRole::CodeBlock {
            return Err(TesError::InvalidTextHeader {
                message: "code_lang is only valid on code_block".into(),
            });
        }
        self.validate_block_label("title", self.title.as_deref())?;
        self.validate_block_label("caption", self.caption.as_deref())?;
        if self.role == TextRole::Heading
            && let Some(level) = self.level
            && !(1..=6).contains(&level)
        {
            return Err(TesError::InvalidTextHeader {
                message: format!("heading level {level} must be 1..=6"),
            });
        }
        if self.toc_depth.is_some() && self.role != TextRole::Toc {
            return Err(TesError::InvalidTextHeader {
                message: "toc_depth is only valid on toc".into(),
            });
        }
        if self.toc_pages.is_some()
            && !matches!(
                self.role,
                TextRole::Toc | TextRole::Lof | TextRole::Lot
            )
        {
            return Err(TesError::InvalidTextHeader {
                message: "toc_pages is only valid on toc, lof, or lot".into(),
            });
        }
        if self.toc_sections.is_some() && self.role != TextRole::Toc {
            return Err(TesError::InvalidTextHeader {
                message: "toc_sections is only valid on toc".into(),
            });
        }
        if self.toc_leaders.is_some()
            && !matches!(
                self.role,
                TextRole::Toc | TextRole::Lof | TextRole::Lot
            )
        {
            return Err(TesError::InvalidTextHeader {
                message: "toc_leaders is only valid on toc, lof, or lot".into(),
            });
        }
        if self.role == TextRole::Toc
            && let Some(depth) = self.toc_depth
            && !(1..=6).contains(&depth)
        {
            return Err(TesError::InvalidTextHeader {
                message: format!("toc_depth {depth} must be 1..=6"),
            });
        }
        if self.columns_count.is_some() && self.role != TextRole::Columns {
            return Err(TesError::InvalidTextHeader {
                message: "columns_count is only valid on columns".into(),
            });
        }
        if self.columns_gap.is_some() && self.role != TextRole::Columns {
            return Err(TesError::InvalidTextHeader {
                message: "columns_gap is only valid on columns".into(),
            });
        }
        if self.role == TextRole::Columns
            && let Some(count) = self.columns_count
            && !(1..=6).contains(&count)
        {
            return Err(TesError::InvalidTextHeader {
                message: format!("columns_count {count} must be 1..=6"),
            });
        }
        if self.list_depth.is_some() && self.role != TextRole::ListItem {
            return Err(TesError::InvalidTextHeader {
                message: "list_depth is only valid on list_item".into(),
            });
        }
        if self.role == TextRole::ListItem
            && let Some(depth) = self.list_depth
            && !(1..=16).contains(&depth)
        {
            return Err(TesError::InvalidTextHeader {
                message: format!("list_depth {depth} must be 1..=16"),
            });
        }
        if let Some(indent) = self.indent
            && indent > 16
        {
            return Err(TesError::InvalidTextHeader {
                message: format!("indent {indent} must be 0..=16"),
            });
        }
        Ok(())
    }

    /// Lossy Markdown projection of a text-chunk body (export + Tessprek).
    #[must_use]
    pub fn render_markdown(&self, body: &str) -> String {
        self.render_markdown_with_links(body, &[])
    }

    /// Markdown projection resolving [`super::InlineKind::Link`] via the document
    /// link table.
    #[must_use]
    pub fn render_markdown_with_links(
        &self,
        body: &str,
        links: &[crate::catalog::LinkEntry],
    ) -> String {
        self.render_markdown_with_links_indexed(body, links, None)
    }

    /// Like [`Self::render_markdown_with_links`], with an optional ordered-list
    /// marker index (defaults to `1` when absent).
    #[must_use]
    pub fn render_markdown_with_links_indexed(
        &self,
        body: &str,
        links: &[crate::catalog::LinkEntry],
        ordered_index: Option<u32>,
    ) -> String {
        let body = body.trim_end();
        let spanned = apply_spans_markdown(body, &self.spans, links);
        match self.role {
            TextRole::Heading => {
                let level = self.level.unwrap_or(1).clamp(1, 6) as usize;
                format!("{} {spanned}", "#".repeat(level))
            }
            TextRole::ListItem => {
                format!("{}{spanned}", self.list_marker_prefix(ordered_index))
            }
            TextRole::Blockquote => spanned
                .lines()
                .map(|line| format!("> {line}"))
                .collect::<Vec<_>>()
                .join("\n"),
            TextRole::CodeBlock => {
                let lang = self.code_lang.as_deref().unwrap_or("");
                format!("```{lang}\n{body}\n```")
            }
            TextRole::Table => {
                if let Some(table) = &self.table {
                    render_table_markdown_with_links(table, links)
                } else {
                    format!("```tsv\n{body}\n```")
                }
            }
            TextRole::Row => {
                if let Some(panes) = &self.panes {
                    render_row_tessprek(panes, links)
                } else {
                    spanned
                }
            }
            TextRole::Math => format!("$$\n{body}\n$$"),
            TextRole::Toc => render_toc_tessprek(self),
            TextRole::Lof => render_float_list_tessprek(self, "lof"),
            TextRole::Lot => render_float_list_tessprek(self, "lot"),
            TextRole::Columns => render_columns_tessprek(self),
            TextRole::ColumnsEnd => "\\endcolumns".into(),
            TextRole::Paragraph => spanned,
        }
    }
}

/// Tessprek projection: `\toc` or `\toc{depth=… title="…"}`.
fn render_toc_tessprek(header: &TextHeader) -> String {
    let mut parts = Vec::new();
    if let Some(title) = header.title.as_deref().filter(|s| !s.is_empty()) {
        parts.push(format!("title=\"{title}\""));
    }
    if let Some(depth) = header.toc_depth {
        parts.push(format!("depth={depth}"));
    }
    // Defaults are on; only emit explicit `false` (or legacy `true` when sealed).
    match header.toc_pages {
        Some(false) => parts.push("page_numbers=false".into()),
        Some(true) => parts.push("page_numbers=true".into()),
        None => {}
    }
    match header.toc_sections {
        Some(false) => parts.push("section_numbers=false".into()),
        Some(true) => parts.push("section_numbers=true".into()),
        None => {}
    }
    match header.toc_leaders {
        Some(false) => parts.push("leaders=false".into()),
        Some(true) => parts.push("leaders=true".into()),
        None => {}
    }
    if parts.is_empty() {
        "\\toc".into()
    } else {
        format!("\\toc{{{}}}", parts.join(" "))
    }
}

/// Tessprek projection: `\lof` / `\lot` or braced attrs (THI-395).
fn render_float_list_tessprek(header: &TextHeader, cmd: &str) -> String {
    let mut parts = Vec::new();
    if let Some(title) = header.title.as_deref().filter(|s| !s.is_empty()) {
        parts.push(format!("title=\"{title}\""));
    }
    match header.toc_pages {
        Some(false) => parts.push("page_numbers=false".into()),
        Some(true) => parts.push("page_numbers=true".into()),
        None => {}
    }
    match header.toc_leaders {
        Some(false) => parts.push("leaders=false".into()),
        Some(true) => parts.push("leaders=true".into()),
        None => {}
    }
    if parts.is_empty() {
        format!("\\{cmd}")
    } else {
        format!("\\{cmd}{{{}}}", parts.join(" "))
    }
}

/// Tessprek projection: `\columns` or `\columns{n=… gap=…}`.
fn render_columns_tessprek(header: &TextHeader) -> String {
    let mut parts = Vec::new();
    if let Some(n) = header.columns_count {
        parts.push(format!("n={n}"));
    }
    if let Some(gap) = header.columns_gap {
        parts.push(format!("gap={gap}"));
    }
    if parts.is_empty() {
        "\\columns".into()
    } else {
        format!("\\columns{{{}}}", parts.join(" "))
    }
}

/// Tessprek projection: `\row{pane0}{pane1}…`.
fn render_row_tessprek(panes: &[TableCell], links: &[crate::catalog::LinkEntry]) -> String {
    let mut out = String::from("\\row");
    for pane in panes {
        out.push('{');
        out.push_str(&render_cell_markdown(pane, links));
        out.push('}');
    }
    out
}
