//! Bibliography interchange (BibTeX + CSL JSON).
//!
//! These formats are **import/export only** — never the canonical cite payload.
//! Canonical cites live as type-`4` chunks ([`CitePayload`](crate::catalog::CitePayload)).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::catalog::TesWriterSession;
use crate::catalog::chunk::CitePayload;
use crate::catalog::document::DocumentCatalog;
use crate::catalog::file::TesFile;
use crate::catalog::index::ChunkType;
use crate::error::{Result, TesError};
use crate::layout::DocKind;

/// Bibliographic source metadata carried on a cite chunk (optional).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BibEntry {
    /// Cite key (`keller2020chunking`).
    pub cite_key: String,
    /// BibTeX entry type (`article`, `book`, `misc`, …).
    pub entry_type: String,
    /// Author string as written in the source file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Work title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Journal / container title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub journal: Option<String>,
    /// Publication year.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<String>,
    /// Volume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<String>,
    /// Issue / number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    /// Page range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pages: Option<String>,
    /// DOI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    /// Publisher.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    /// Free-form note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// `howpublished` (BibTeX misc).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub howpublished: Option<String>,
    /// URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Bibliography interchange format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BibFormat {
    /// BibTeX (`.bib`).
    Bibtex,
    /// CSL-JSON array.
    CslJson,
}

impl BibFormat {
    /// Parse a CLI/format string.
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "bibtex" | "bib" => Ok(Self::Bibtex),
            "csl-json" | "csl" | "json" => Ok(Self::CslJson),
            other => Err(TesError::InvalidBib {
                message: format!("unknown bibliography format '{other}' (use bibtex or csl-json)"),
            }),
        }
    }

    /// Stable name for CLI help.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bibtex => "bibtex",
            Self::CslJson => "csl-json",
        }
    }
}

/// Options for importing a bibliography into a sealed `.tes`.
#[derive(Debug, Clone)]
pub struct BibImportOptions {
    /// Document kind (default research).
    pub doc_kind: DocKind,
    /// Catalog title override.
    pub title: Option<String>,
    /// Stable document UUID.
    pub doc_id: Option<String>,
    /// Catalog `cite_style_id` (default `numeric`).
    pub cite_style_id: Option<String>,
}

impl Default for BibImportOptions {
    fn default() -> Self {
        Self {
            doc_kind: DocKind::Research,
            title: None,
            doc_id: None,
            cite_style_id: Some("numeric".into()),
        }
    }
}

/// Parse a BibTeX subset sufficient for Tessera fixtures and interchange.
pub fn parse_bibtex(input: &str) -> Result<Vec<BibEntry>> {
    let mut entries = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        skip_ws_comments(bytes, &mut i);
        if i >= bytes.len() {
            break;
        }
        if bytes[i] != b'@' {
            return Err(TesError::InvalidBib {
                message: format!("expected '@' at byte {i}"),
            });
        }
        i += 1;
        let type_start = i;
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
        let entry_type = input[type_start..i].to_ascii_lowercase();
        skip_ws_comments(bytes, &mut i);
        if i >= bytes.len() || bytes[i] != b'{' {
            return Err(TesError::InvalidBib {
                message: "expected '{' after entry type".into(),
            });
        }
        i += 1;
        skip_ws_comments(bytes, &mut i);
        let key_start = i;
        while i < bytes.len() && bytes[i] != b',' && bytes[i] != b'}' {
            i += 1;
        }
        let cite_key = input[key_start..i].trim().to_owned();
        if cite_key.is_empty() {
            return Err(TesError::InvalidBib {
                message: "empty cite key".into(),
            });
        }
        let mut entry = BibEntry {
            cite_key,
            entry_type,
            ..BibEntry::default()
        };
        if i < bytes.len() && bytes[i] == b',' {
            i += 1;
        }
        loop {
            skip_ws_comments(bytes, &mut i);
            if i >= bytes.len() {
                return Err(TesError::InvalidBib {
                    message: "unterminated BibTeX entry".into(),
                });
            }
            if bytes[i] == b'}' {
                i += 1;
                break;
            }
            let field_start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let field = input[field_start..i].to_ascii_lowercase();
            skip_ws_comments(bytes, &mut i);
            if i >= bytes.len() || bytes[i] != b'=' {
                return Err(TesError::InvalidBib {
                    message: format!("expected '=' after field '{field}'"),
                });
            }
            i += 1;
            skip_ws_comments(bytes, &mut i);
            let value = parse_bib_value(input, bytes, &mut i)?;
            set_field(&mut entry, &field, value);
            skip_ws_comments(bytes, &mut i);
            if i < bytes.len() && bytes[i] == b',' {
                i += 1;
            }
        }
        entries.push(entry);
    }
    Ok(entries)
}

/// Serialize entries to BibTeX.
#[must_use]
pub fn to_bibtex(entries: &[BibEntry]) -> String {
    let mut out = String::new();
    for (i, entry) in entries.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!("@{}{{{},\n", entry.entry_type, entry.cite_key),
        );
        write_bib_field(&mut out, "author", entry.author.as_deref());
        write_bib_field(&mut out, "title", entry.title.as_deref());
        write_bib_field(&mut out, "journal", entry.journal.as_deref());
        write_bib_field(&mut out, "year", entry.year.as_deref());
        write_bib_field(&mut out, "volume", entry.volume.as_deref());
        write_bib_field(&mut out, "number", entry.number.as_deref());
        write_bib_field(&mut out, "pages", entry.pages.as_deref());
        write_bib_field(&mut out, "doi", entry.doi.as_deref());
        write_bib_field(&mut out, "publisher", entry.publisher.as_deref());
        write_bib_field(&mut out, "note", entry.note.as_deref());
        write_bib_field(&mut out, "howpublished", entry.howpublished.as_deref());
        write_bib_field(&mut out, "url", entry.url.as_deref());
        out.push_str("}\n");
    }
    out
}

/// Serialize entries to a CSL-JSON array.
pub fn to_csl_json(entries: &[BibEntry]) -> Result<String> {
    let items: Vec<CslItem> = entries.iter().map(CslItem::from_bib).collect();
    Ok(serde_json::to_string_pretty(&items)?)
}

/// Parse a CSL-JSON array into bibliography entries.
pub fn parse_csl_json(input: &str) -> Result<Vec<BibEntry>> {
    let items: Vec<CslItem> = serde_json::from_str(input)?;
    Ok(items.into_iter().map(CslItem::into_bib).collect())
}

/// Collect bibliography entries from cite chunks in `file`.
pub fn collect_bib_entries(file: &TesFile) -> Result<Vec<BibEntry>> {
    let mut out = Vec::new();
    for entry in file.reading_order_chunks() {
        if entry.chunk_type != ChunkType::Cite {
            continue;
        }
        let raw = file.decode_payload(entry)?;
        let cite = CitePayload::from_bytes(&raw).map_err(|e| TesError::Decode {
            chunk_id: entry.chunk_id,
            message: e.to_string(),
        })?;
        out.push(bib_from_cite(&cite));
    }
    Ok(out)
}

/// Export bibliography from a `.tes` path.
pub fn export_bibliography(path: impl AsRef<Path>, format: BibFormat) -> Result<String> {
    let file = TesFile::open(path.as_ref())?;
    let entries = collect_bib_entries(&file)?;
    match format {
        BibFormat::Bibtex => Ok(to_bibtex(&entries)),
        BibFormat::CslJson => to_csl_json(&entries),
    }
}

/// Import BibTeX or CSL-JSON into a new research `.tes` (one cite chunk per entry).
pub fn import_bibliography(
    input: impl AsRef<Path>,
    output: impl AsRef<Path>,
    format: BibFormat,
    options: &BibImportOptions,
) -> Result<()> {
    let text = fs::read_to_string(input.as_ref())?;
    let entries = match format {
        BibFormat::Bibtex => parse_bibtex(&text)?,
        BibFormat::CslJson => parse_csl_json(&text)?,
    };
    if entries.is_empty() {
        return Err(TesError::InvalidBib {
            message: "bibliography contains no entries".into(),
        });
    }

    let doc_id = match &options.doc_id {
        Some(id) => {
            Uuid::parse_str(id).map_err(|_| TesError::InvalidDocId { value: id.clone() })?;
            id.clone()
        }
        None => Uuid::new_v4().to_string(),
    };
    let title = options
        .title
        .clone()
        .unwrap_or_else(|| "Bibliography".into());
    let now = "2026-07-25T00:00:00Z";

    let mut catalog = DocumentCatalog::new(&doc_id, title, now, now, options.doc_kind);
    catalog.cite_style_id = options
        .cite_style_id
        .clone()
        .or_else(|| Some("numeric".into()));

    let mut session = TesWriterSession::create(output.as_ref(), options.doc_kind);
    session.set_catalog(catalog)?;
    for entry in &entries {
        let cite = CitePayload {
            quote: entry.title.clone().unwrap_or_default(),
            target_doc_id: None,
            target_chunk_id: None,
            target_byte_start: None,
            target_byte_end: None,
            label: Some(entry.cite_key.clone()),
            page: None,
            source: Some(entry.clone()),
        };
        session.add_cite_chunk(&cite)?;
    }
    session.commit()?;
    Ok(())
}

/// Format a short in-text citation for numeric style (`[1]`).
#[must_use]
pub fn format_numeric_marker(n: usize) -> String {
    format!("[{n}]")
}

/// Format a Pandoc-style Markdown citation.
#[must_use]
pub fn format_pandoc_cite(label: &str) -> String {
    format!("[@{label}]")
}

/// One-line bibliography body (no generated number).
#[must_use]
pub fn format_reference_body(entry: &BibEntry) -> String {
    let mut parts = Vec::new();
    if let Some(author) = entry.author.as_deref() {
        parts.push(author.to_owned());
    }
    if let Some(year) = entry.year.as_deref() {
        parts.push(format!("({year})"));
    }
    if let Some(title) = entry.title.as_deref() {
        parts.push(title.to_owned());
    } else {
        parts.push(entry.cite_key.clone());
    }
    if let Some(journal) = entry.journal.as_deref() {
        parts.push(journal.to_owned());
    }
    parts.join(". ")
}

/// One-line numeric bibliography item.
#[must_use]
pub fn format_numeric_reference(n: usize, entry: &BibEntry) -> String {
    format!("{n}. {}", format_reference_body(entry))
}

fn bib_from_cite(cite: &CitePayload) -> BibEntry {
    if let Some(source) = &cite.source {
        return source.clone();
    }
    BibEntry {
        cite_key: cite.label.clone().unwrap_or_else(|| "unknown".into()),
        entry_type: "misc".into(),
        title: if cite.quote.trim().is_empty() {
            None
        } else {
            Some(cite.quote.clone())
        },
        note: cite.page.map(|p| format!("page {p}")),
        ..BibEntry::default()
    }
}

fn set_field(entry: &mut BibEntry, field: &str, value: String) {
    match field {
        "author" => entry.author = Some(value),
        "title" => entry.title = Some(value),
        "journal" | "journaltitle" => entry.journal = Some(value),
        "year" | "date" => entry.year = Some(value),
        "volume" => entry.volume = Some(value),
        "number" | "issue" => entry.number = Some(value),
        "pages" => entry.pages = Some(value),
        "doi" => entry.doi = Some(value),
        "publisher" => entry.publisher = Some(value),
        "note" => entry.note = Some(value),
        "howpublished" => entry.howpublished = Some(value),
        "url" => entry.url = Some(value),
        _ => {}
    }
}

fn write_bib_field(out: &mut String, name: &str, value: Option<&str>) {
    let Some(value) = value else {
        return;
    };
    let escaped = value.replace('{', "\\{").replace('}', "\\}");
    let _ = std::fmt::Write::write_fmt(out, format_args!("  {name} = {{{escaped}}},\n"));
}

fn skip_ws_comments(bytes: &[u8], i: &mut usize) {
    loop {
        while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
            *i += 1;
        }
        if *i + 1 < bytes.len() && bytes[*i] == b'%' {
            while *i < bytes.len() && bytes[*i] != b'\n' {
                *i += 1;
            }
            continue;
        }
        break;
    }
}

fn parse_bib_value(input: &str, bytes: &[u8], i: &mut usize) -> Result<String> {
    if *i >= bytes.len() {
        return Err(TesError::InvalidBib {
            message: "expected field value".into(),
        });
    }
    match bytes[*i] {
        b'{' => {
            *i += 1;
            let mut depth = 1usize;
            let start = *i;
            while *i < bytes.len() {
                match bytes[*i] {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            let value = strip_bib_wrappers(&input[start..*i]);
                            *i += 1;
                            return Ok(value);
                        }
                    }
                    _ => {}
                }
                *i += 1;
            }
            Err(TesError::InvalidBib {
                message: "unterminated '{' value".into(),
            })
        }
        b'"' => {
            *i += 1;
            let start = *i;
            while *i < bytes.len() && bytes[*i] != b'"' {
                *i += 1;
            }
            if *i >= bytes.len() {
                return Err(TesError::InvalidBib {
                    message: "unterminated '\"' value".into(),
                });
            }
            let value = input[start..*i].to_owned();
            *i += 1;
            Ok(value)
        }
        _ => {
            let start = *i;
            while *i < bytes.len()
                && !bytes[*i].is_ascii_whitespace()
                && bytes[*i] != b','
                && bytes[*i] != b'}'
            {
                *i += 1;
            }
            Ok(input[start..*i].to_owned())
        }
    }
}

fn strip_bib_wrappers(value: &str) -> String {
    let mut out = value.to_owned();
    // Unwrap a single outer brace pair used for protecting capitals / urls.
    if out.starts_with('{') && out.ends_with('}') && out.len() >= 2 {
        out = out[1..out.len() - 1].to_owned();
    }
    out = out.replace("\\url{", "").replace('\\', "");
    if out.ends_with('}') {
        out.pop();
    }
    out
}

#[derive(Debug, Serialize, Deserialize)]
struct CslItem {
    id: String,
    #[serde(rename = "type")]
    item_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    author: Option<Vec<CslName>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    issued: Option<CslIssued>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "container-title")]
    container_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    volume: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    issue: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    page: Option<String>,
    #[serde(default, rename = "DOI", skip_serializing_if = "Option::is_none")]
    doi: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    publisher: Option<String>,
    #[serde(default, rename = "URL", skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CslName {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    given: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    literal: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CslIssued {
    #[serde(rename = "date-parts")]
    date_parts: Vec<Vec<i32>>,
}

impl CslItem {
    fn from_bib(entry: &BibEntry) -> Self {
        let item_type = match entry.entry_type.as_str() {
            "article" => "article-journal",
            "book" => "book",
            _ => "document",
        }
        .to_owned();
        let author = entry.author.as_ref().map(|a| {
            vec![CslName {
                family: None,
                given: None,
                literal: Some(a.clone()),
            }]
        });
        let issued = entry.year.as_ref().and_then(|y| {
            y.parse::<i32>().ok().map(|year| CslIssued {
                date_parts: vec![vec![year]],
            })
        });
        Self {
            id: entry.cite_key.clone(),
            item_type,
            title: entry.title.clone(),
            author,
            issued,
            container_title: entry.journal.clone(),
            volume: entry.volume.clone(),
            issue: entry.number.clone(),
            page: entry.pages.clone(),
            doi: entry.doi.clone(),
            publisher: entry.publisher.clone(),
            url: entry.url.clone().or_else(|| {
                entry.howpublished.clone().and_then(|h| {
                    let trimmed = h.trim();
                    if trimmed.starts_with("http") {
                        Some(trimmed.to_owned())
                    } else {
                        None
                    }
                })
            }),
            note: entry.note.clone(),
        }
    }

    fn into_bib(self) -> BibEntry {
        let year = self
            .issued
            .and_then(|issued| issued.date_parts.first()?.first().copied())
            .map(|y| y.to_string());
        let author = self.author.map(|names| {
            names
                .into_iter()
                .map(|n| {
                    if let Some(literal) = n.literal {
                        literal
                    } else {
                        match (n.family, n.given) {
                            (Some(f), Some(g)) => format!("{f}, {g}"),
                            (Some(f), None) => f,
                            (None, Some(g)) => g,
                            _ => String::new(),
                        }
                    }
                })
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" and ")
        });
        let entry_type = match self.item_type.as_str() {
            "article-journal" => "article",
            "book" => "book",
            _ => "misc",
        }
        .to_owned();
        BibEntry {
            cite_key: self.id,
            entry_type,
            author,
            title: self.title,
            journal: self.container_title,
            year,
            volume: self.volume,
            number: self.issue,
            pages: self.page,
            doi: self.doi,
            publisher: self.publisher,
            note: self.note,
            howpublished: None,
            url: self.url,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sample_bibtex_fixture() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/assets/citations/sample.bib");
        let text = fs::read_to_string(path).unwrap();
        let entries = parse_bibtex(&text).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].cite_key, "keller2020chunking");
        assert_eq!(entries[0].entry_type, "article");
        assert!(
            entries[0]
                .title
                .as_deref()
                .unwrap()
                .contains("Chunk-Oriented")
        );
        assert_eq!(entries[1].cite_key, "latka2026tessera");
        assert_eq!(entries[2].cite_key, "picsum2026");
    }

    #[test]
    fn bibtex_csl_round_trip_keys() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/assets/citations/sample.bib");
        let entries = parse_bibtex(&fs::read_to_string(path).unwrap()).unwrap();
        let csl = to_csl_json(&entries).unwrap();
        let back = parse_csl_json(&csl).unwrap();
        assert_eq!(back.len(), 3);
        assert_eq!(back[0].cite_key, "keller2020chunking");
        assert_eq!(back[1].title, entries[1].title);
    }
}
