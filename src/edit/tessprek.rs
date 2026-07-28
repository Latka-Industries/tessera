//! Tessera Markdown (Tessprek) encode/decode for virtual editor buffers.
//!
//! Format (v1): HTML-comment directives carry typed fields; bodies are Markdown.

use std::fmt::Write as _;

use crate::catalog::TesFile;
use crate::catalog::chunk::{CitePayload, ListKind, TextHeader, TextRole, decode_text_payload};
use crate::catalog::index::ChunkType;
use crate::catalog::media::{FigureRef, ImagePlacement};
use crate::catalog::slide::{SlidePayload, SlideRegion};
use crate::error::{Result, TesError};

use super::ContentBlock;

const HEADER_PREFIX: &str = "<!-- tessera:";
const CHUNK_PREFIX: &str = "<!-- tes ";
const CHUNK_SUFFIX: &str = " -->";

/// Encode a `.tes` file as Tessera Markdown, embedding `source_hash`.
///
/// # Errors
///
/// Returns decode errors for reading-order text/figure/cite payloads.
pub fn encode_tessprek(file: &TesFile, source_hash: &str) -> Result<String> {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{HEADER_PREFIX} format=tessprek version=1 source-hash={source_hash} -->"
    );
    out.push('\n');

    for entry in file.reading_order_chunks() {
        match entry.chunk_type {
            ChunkType::Text => {
                let raw = file.decode_payload(entry)?;
                let (header, body) = decode_text_payload(raw.as_ref())?;
                write_text_directive(&mut out, entry.chunk_id, &header);
                out.push_str(&render_text_body(&header, &body));
                out.push_str("\n\n");
            }
            ChunkType::Figure => {
                let raw = file.decode_payload(entry)?;
                let figure = FigureRef::from_bytes(raw.as_ref())?;
                write_figure_directive(&mut out, entry.chunk_id, &figure);
                let _ = writeln!(
                    out,
                    "![{}](media:chunk-{})",
                    escape_alt(&figure.alt_text),
                    figure.image_chunk_id
                );
                out.push('\n');
            }
            ChunkType::Cite => {
                let raw = file.decode_payload(entry)?;
                let cite = CitePayload::from_bytes(raw.as_ref())?;
                write_cite_directive(&mut out, entry.chunk_id, &cite);
                out.push_str(cite.quote.trim_end());
                out.push_str("\n\n");
            }
            ChunkType::Slide => {
                let raw = file.decode_payload(entry)?;
                let slide = SlidePayload::from_bytes(raw.as_ref())?;
                write_slide_directive(&mut out, entry.chunk_id, &slide);
                out.push('\n');
            }
            _ => {}
        }
    }
    Ok(out)
}

/// Parse Tessera Markdown into typed content blocks.
///
/// # Errors
///
/// Returns [`TesError::EditParse`] with line/column on malformed directives or bodies.
pub fn decode_tessprek(input: &str) -> Result<Vec<ContentBlock>> {
    let lines: Vec<&str> = input.lines().collect();
    let mut blocks = Vec::new();
    let mut i = 0usize;

    // Skip blank lines and the optional file header.
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.is_empty() || trimmed.starts_with(HEADER_PREFIX) {
            i += 1;
            continue;
        }
        break;
    }

    while i < lines.len() {
        let line_no = i + 1;
        let trimmed = lines[i].trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }
        if !trimmed.starts_with(CHUNK_PREFIX) || !trimmed.ends_with(CHUNK_SUFFIX) {
            return Err(parse_err(
                line_no,
                1,
                format!("expected `{CHUNK_PREFIX}...{CHUNK_SUFFIX}` directive, found: {trimmed}"),
            ));
        }
        let attrs = &trimmed[CHUNK_PREFIX.len()..trimmed.len() - CHUNK_SUFFIX.len()];
        let map = parse_attrs(attrs, line_no)?;
        i += 1;

        let chunk_id = required_u64(&map, "chunk", line_no)?;
        let kind = map
            .get("type")
            .map(String::as_str)
            .or_else(|| map.get("role").map(|_| "text"))
            .unwrap_or("text");

        let body_start = i;
        while i < lines.len() {
            let t = lines[i].trim();
            if t.starts_with(CHUNK_PREFIX) && t.ends_with(CHUNK_SUFFIX) {
                break;
            }
            i += 1;
        }
        let body = trim_block_body(&lines[body_start..i]);

        match kind {
            "text" | "paragraph" | "heading" | "list_item" | "blockquote" | "code_block"
            | "table" => {
                let role =
                    parse_role(map.get("role").map(String::as_str).unwrap_or(kind), line_no)?;
                let mut header = TextHeader {
                    role,
                    level: None,
                    list_kind: None,
                    emphasis: Vec::new(),
                    classes: parse_classes(map.get("class").map(String::as_str)),
                };
                if role == TextRole::Heading {
                    header.level = Some(required_u32(&map, "level", line_no)?.clamp(1, 6));
                }
                if role == TextRole::ListItem {
                    header.list_kind = Some(parse_list_kind(
                        map.get("list").map(String::as_str).unwrap_or("bullet"),
                        line_no,
                    )?);
                }
                let text_body = strip_markdown_wrapper(&header, &body);
                blocks.push(ContentBlock::Text {
                    chunk_id: Some(chunk_id),
                    header,
                    body: text_body,
                });
            }
            "figure" => {
                let image_chunk_id = required_u64(&map, "image", line_no)?;
                let placement = parse_placement(
                    map.get("placement").map(String::as_str).unwrap_or("flow"),
                    map.get("region").map(String::as_str),
                    line_no,
                )?;
                let caption = map.get("caption").cloned().filter(|s| !s.is_empty());
                let (alt_text, img_from_md) = parse_figure_markdown(&body, line_no)?;
                let image_chunk_id = img_from_md.unwrap_or(image_chunk_id);
                blocks.push(ContentBlock::Figure {
                    chunk_id: Some(chunk_id),
                    figure: FigureRef {
                        image_chunk_id,
                        alt_text,
                        caption,
                        placement,
                    },
                });
            }
            "cite" => {
                let label = map.get("label").cloned().filter(|s| !s.is_empty());
                let target_doc_id = map.get("target_doc").cloned().filter(|s| !s.is_empty());
                let target_chunk_id = optional_u64(&map, "target_chunk");
                blocks.push(ContentBlock::Cite {
                    chunk_id: Some(chunk_id),
                    cite: CitePayload {
                        quote: body,
                        target_doc_id,
                        target_chunk_id,
                        target_byte_start: None,
                        target_byte_end: None,
                        label,
                        page: optional_u32(&map, "page"),
                        source: None,
                    },
                });
            }
            "slide" => {
                let layout_id = map
                    .get("layout")
                    .cloned()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| parse_err(line_no, 1, "slide requires layout=…"))?;
                let regions = parse_slide_regions(
                    map.get("regions").map(String::as_str).unwrap_or(""),
                    line_no,
                )?;
                blocks.push(ContentBlock::Slide {
                    chunk_id: Some(chunk_id),
                    slide: SlidePayload { layout_id, regions },
                });
            }
            other => {
                return Err(parse_err(
                    line_no,
                    1,
                    format!("unknown tes directive type '{other}'"),
                ));
            }
        }
    }

    Ok(blocks)
}

fn write_text_directive(out: &mut String, chunk_id: u64, header: &TextHeader) {
    let _ = write!(
        out,
        "{CHUNK_PREFIX}chunk={chunk_id} role={}",
        header.role.as_str()
    );
    if let Some(level) = header.level {
        let _ = write!(out, " level={level}");
    }
    if let Some(list) = header.list_kind {
        let kind = match list {
            ListKind::Bullet => "bullet",
            ListKind::Ordered => "ordered",
        };
        let _ = write!(out, " list={kind}");
    }
    if !header.classes.is_empty() {
        let _ = write!(out, " class=\"{}\"", header.classes.join(" "));
    }
    let _ = writeln!(out, "{CHUNK_SUFFIX}");
}

fn write_figure_directive(out: &mut String, chunk_id: u64, figure: &FigureRef) {
    let _ = write!(
        out,
        "{CHUNK_PREFIX}chunk={chunk_id} type=figure image={} placement={}",
        figure.image_chunk_id,
        figure.placement.as_str()
    );
    if let ImagePlacement::Region { name } = &figure.placement {
        let _ = write!(out, " region=\"{}\"", escape_attr(name));
    }
    if let Some(caption) = figure.caption.as_deref() {
        let _ = write!(out, " caption=\"{}\"", escape_attr(caption));
    }
    let _ = writeln!(out, "{CHUNK_SUFFIX}");
}

fn write_cite_directive(out: &mut String, chunk_id: u64, cite: &CitePayload) {
    let _ = write!(out, "{CHUNK_PREFIX}chunk={chunk_id} type=cite");
    if let Some(label) = cite.label.as_deref() {
        let _ = write!(out, " label={}", attr_token(label));
    }
    if let Some(doc) = cite.target_doc_id.as_deref() {
        let _ = write!(out, " target_doc={doc}");
    }
    if let Some(chunk) = cite.target_chunk_id {
        let _ = write!(out, " target_chunk={chunk}");
    }
    if let Some(page) = cite.page {
        let _ = write!(out, " page={page}");
    }
    let _ = writeln!(out, "{CHUNK_SUFFIX}");
}

fn write_slide_directive(out: &mut String, chunk_id: u64, slide: &SlidePayload) {
    let regions = slide
        .regions
        .iter()
        .map(|r| format!("{}:{}", r.name, r.chunk_id))
        .collect::<Vec<_>>()
        .join(",");
    let _ = writeln!(
        out,
        "{CHUNK_PREFIX}chunk={chunk_id} type=slide layout={} regions=\"{}\"{CHUNK_SUFFIX}",
        attr_token(&slide.layout_id),
        escape_attr(&regions)
    );
}

fn parse_slide_regions(raw: &str, line_no: usize) -> Result<Vec<SlideRegion>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(parse_err(line_no, 1, "slide regions= must be non-empty"));
    }
    let mut regions = Vec::new();
    for part in raw.split(',') {
        let part = part.trim();
        let Some((name, id)) = part.split_once(':') else {
            return Err(parse_err(
                line_no,
                1,
                format!("bad region '{part}' (expected name:chunk_id)"),
            ));
        };
        let chunk_id = id
            .trim()
            .parse::<u64>()
            .map_err(|_| parse_err(line_no, 1, format!("invalid region chunk id in '{part}'")))?;
        regions.push(SlideRegion {
            name: name.trim().to_owned(),
            chunk_id,
        });
    }
    Ok(regions)
}

fn render_text_body(header: &TextHeader, body: &str) -> String {
    let body = body.trim_end();
    match header.role {
        TextRole::Heading => {
            let level = header.level.unwrap_or(1).clamp(1, 6) as usize;
            format!("{} {body}", "#".repeat(level))
        }
        TextRole::ListItem => match header.list_kind.unwrap_or(ListKind::Bullet) {
            ListKind::Bullet => format!("- {body}"),
            ListKind::Ordered => format!("1. {body}"),
        },
        TextRole::Blockquote => body
            .lines()
            .map(|line| format!("> {line}"))
            .collect::<Vec<_>>()
            .join("\n"),
        TextRole::CodeBlock => format!("```\n{body}\n```"),
        TextRole::Table => format!("```tsv\n{body}\n```"),
        TextRole::Paragraph => body.to_owned(),
    }
}

fn strip_markdown_wrapper(header: &TextHeader, body: &str) -> String {
    let body = body.trim();
    match header.role {
        TextRole::Heading => {
            let trimmed = body.trim_start_matches('#').trim_start();
            trimmed.to_owned()
        }
        TextRole::ListItem => {
            let t = body.trim_start();
            if let Some(rest) = t.strip_prefix("- ") {
                rest.to_owned()
            } else if let Some(rest) = t.strip_prefix("* ") {
                rest.to_owned()
            } else {
                // Ordered: "N. rest"
                let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
                if digits > 0 {
                    let after = &t[digits..];
                    if let Some(rest) = after.strip_prefix(". ") {
                        rest.to_owned()
                    } else if let Some(rest) = after.strip_prefix('.') {
                        rest.trim_start().to_owned()
                    } else {
                        body.to_owned()
                    }
                } else {
                    body.to_owned()
                }
            }
        }
        TextRole::Blockquote => body
            .lines()
            .map(|line| {
                line.strip_prefix("> ")
                    .or_else(|| line.strip_prefix('>'))
                    .unwrap_or(line)
            })
            .collect::<Vec<_>>()
            .join("\n"),
        TextRole::CodeBlock | TextRole::Table => strip_fence(body),
        TextRole::Paragraph => body.to_owned(),
    }
}

fn strip_fence(body: &str) -> String {
    let mut lines = body.lines().peekable();
    if lines
        .peek()
        .is_some_and(|l| l.trim_start().starts_with("```"))
    {
        let _ = lines.next();
    }
    let collected: Vec<&str> = lines.collect();
    let end = if collected
        .last()
        .is_some_and(|l| l.trim() == "```" || l.trim_start().starts_with("```"))
    {
        collected.len().saturating_sub(1)
    } else {
        collected.len()
    };
    collected[..end].join("\n")
}

fn parse_figure_markdown(body: &str, line_no: usize) -> Result<(String, Option<u64>)> {
    let body = body.trim();
    // ![alt](media:chunk-N)
    let Some(rest) = body.strip_prefix("![") else {
        return Err(parse_err(
            line_no,
            1,
            "figure body must be ![alt](media:chunk-N)",
        ));
    };
    let Some((alt, after_alt)) = rest.split_once("](") else {
        return Err(parse_err(line_no, 1, "figure markdown missing ']('"));
    };
    let Some(url) = after_alt.strip_suffix(')') else {
        return Err(parse_err(line_no, 1, "figure markdown missing closing ')'"));
    };
    let image_id = url
        .strip_prefix("media:chunk-")
        .and_then(|s| s.parse::<u64>().ok());
    Ok((unescape_alt(alt), image_id))
}

fn parse_attrs(attrs: &str, line_no: usize) -> Result<std::collections::BTreeMap<String, String>> {
    let mut map = std::collections::BTreeMap::new();
    let mut rest = attrs.trim();
    while !rest.is_empty() {
        let eq = rest.find('=').ok_or_else(|| {
            parse_err(
                line_no,
                1,
                format!("malformed attribute near '{rest}' (expected key=value)"),
            )
        })?;
        let key = rest[..eq].trim();
        if key.is_empty() {
            return Err(parse_err(line_no, 1, "empty attribute key"));
        }
        rest = rest[eq + 1..].trim_start();
        let (value, next) = if let Some(quoted) = rest.strip_prefix('"') {
            let end = quoted
                .find('"')
                .ok_or_else(|| parse_err(line_no, 1, "unterminated quoted attribute"))?;
            let value = quoted[..end].to_owned();
            (value, quoted[end + 1..].trim_start())
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            (rest[..end].to_owned(), rest[end..].trim_start())
        };
        map.insert(key.to_owned(), value);
        rest = next;
    }
    Ok(map)
}

fn parse_role(raw: &str, line_no: usize) -> Result<TextRole> {
    match raw {
        "paragraph" => Ok(TextRole::Paragraph),
        "heading" => Ok(TextRole::Heading),
        "list_item" => Ok(TextRole::ListItem),
        "blockquote" => Ok(TextRole::Blockquote),
        "code_block" => Ok(TextRole::CodeBlock),
        "table" => Ok(TextRole::Table),
        other => Err(parse_err(line_no, 1, format!("unknown role '{other}'"))),
    }
}

fn parse_list_kind(raw: &str, line_no: usize) -> Result<ListKind> {
    match raw {
        "bullet" => Ok(ListKind::Bullet),
        "ordered" => Ok(ListKind::Ordered),
        other => Err(parse_err(
            line_no,
            1,
            format!("unknown list kind '{other}'"),
        )),
    }
}

fn parse_placement(raw: &str, region: Option<&str>, line_no: usize) -> Result<ImagePlacement> {
    match raw {
        "flow" => Ok(ImagePlacement::Flow),
        "full_width" => Ok(ImagePlacement::FullWidth),
        "float_start" => Ok(ImagePlacement::FloatStart),
        "float_end" => Ok(ImagePlacement::FloatEnd),
        "inline" => Ok(ImagePlacement::Inline),
        "background" => Ok(ImagePlacement::Background),
        "region" => Ok(ImagePlacement::Region {
            name: region.unwrap_or("default").to_owned(),
        }),
        other => Err(parse_err(
            line_no,
            1,
            format!("unknown placement '{other}'"),
        )),
    }
}

fn parse_classes(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or("")
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn required_u64(
    map: &std::collections::BTreeMap<String, String>,
    key: &str,
    line_no: usize,
) -> Result<u64> {
    let raw = map
        .get(key)
        .ok_or_else(|| parse_err(line_no, 1, format!("missing required attribute '{key}'")))?;
    raw.parse::<u64>()
        .map_err(|_| parse_err(line_no, 1, format!("invalid {key} value '{raw}'")))
}

fn required_u32(
    map: &std::collections::BTreeMap<String, String>,
    key: &str,
    line_no: usize,
) -> Result<u32> {
    let raw = map
        .get(key)
        .ok_or_else(|| parse_err(line_no, 1, format!("missing required attribute '{key}'")))?;
    raw.parse::<u32>()
        .map_err(|_| parse_err(line_no, 1, format!("invalid {key} value '{raw}'")))
}

fn optional_u64(map: &std::collections::BTreeMap<String, String>, key: &str) -> Option<u64> {
    map.get(key)?.parse().ok()
}

fn optional_u32(map: &std::collections::BTreeMap<String, String>, key: &str) -> Option<u32> {
    map.get(key)?.parse().ok()
}

fn trim_block_body(lines: &[&str]) -> String {
    let mut start = 0;
    let mut end = lines.len();
    while start < end && lines[start].trim().is_empty() {
        start += 1;
    }
    while end > start && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    lines[start..end].join("\n")
}

fn escape_attr(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn attr_token(s: &str) -> String {
    if s.chars().any(|c| c.is_whitespace() || c == '"' || c == '=') {
        format!("\"{}\"", escape_attr(s))
    } else {
        s.to_owned()
    }
}

fn escape_alt(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn unescape_alt(s: &str) -> String {
    s.replace("\\[", "[")
        .replace("\\]", "]")
        .replace("\\\\", "\\")
}

fn parse_err(line: usize, column: usize, message: impl Into<String>) -> TesError {
    TesError::EditParse {
        line,
        column,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{DocumentCatalog, TesWriterSession};
    use crate::layout::DocKind;
    use tempfile::tempdir;

    #[test]
    fn round_trip_text_classes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.tes");
        let mut session = TesWriterSession::create(&path, DocKind::Note);
        session
            .set_catalog(DocumentCatalog::new(
                "550e8400-e29b-41d4-a716-446655440000",
                "Demo",
                "2026-07-27T00:00:00Z",
                "2026-07-27T00:00:00Z",
                DocKind::Note,
            ))
            .unwrap();
        let mut header = TextHeader::heading(1);
        header.classes = vec!["lead".into()];
        session.add_text_chunk(&header, "Hello").unwrap();
        session
            .add_text_chunk(&TextHeader::paragraph(), "Body")
            .unwrap();
        session.commit().unwrap();

        let file = TesFile::open(&path).unwrap();
        let text = encode_tessprek(&file, "abc").unwrap();
        assert!(text.contains("class=\"lead\""));
        let blocks = decode_tessprek(&text).unwrap();
        assert_eq!(blocks.len(), 2);
        match &blocks[0] {
            ContentBlock::Text { header, body, .. } => {
                assert_eq!(header.role, TextRole::Heading);
                assert_eq!(header.classes, vec!["lead"]);
                assert_eq!(body, "Hello");
            }
            _ => panic!("expected text"),
        }
    }
}
