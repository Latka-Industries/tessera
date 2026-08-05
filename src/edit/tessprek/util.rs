use std::collections::BTreeMap;

use crate::catalog::media::ImagePlacement;
use crate::catalog::slide::SlideRegion;
use crate::error::{Result, TesError};

pub(super) fn parse_figure_markdown(body: &str, line_no: usize) -> Result<(String, Option<u64>)> {
    let body = body.trim();
    // ![alt](media:N) — also accept legacy `media:chunk-N`
    let Some(rest) = body.strip_prefix("![") else {
        return Err(parse_err(line_no, 1, "figure body must be ![alt](media:N)"));
    };
    let Some((alt, after_alt)) = rest.split_once("](") else {
        return Err(parse_err(line_no, 1, "figure markdown missing ']('"));
    };
    let Some(url) = after_alt.strip_suffix(')') else {
        return Err(parse_err(line_no, 1, "figure markdown missing closing ')'"));
    };
    let id_str = url
        .strip_prefix("media:")
        .map(|s| s.strip_prefix("chunk-").unwrap_or(s));
    let image_id = id_str.and_then(|s| s.parse::<u64>().ok());
    Ok((unescape_alt(alt), image_id))
}

pub(crate) fn parse_attrs(attrs: &str, line_no: usize) -> Result<BTreeMap<String, String>> {
    let mut map = BTreeMap::new();
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

pub(super) fn parse_placement(
    raw: &str,
    region: Option<&str>,
    line_no: usize,
) -> Result<ImagePlacement> {
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

pub(super) fn parse_slide_regions(raw: &str, line_no: usize) -> Result<Vec<SlideRegion>> {
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

pub(super) fn required_u64(
    map: &BTreeMap<String, String>,
    key: &str,
    line_no: usize,
) -> Result<u64> {
    let raw = map
        .get(key)
        .ok_or_else(|| parse_err(line_no, 1, format!("missing required attribute '{key}'")))?;
    raw.parse::<u64>()
        .map_err(|_| parse_err(line_no, 1, format!("invalid {key} value '{raw}'")))
}

pub(super) fn optional_u64(map: &BTreeMap<String, String>, key: &str) -> Option<u64> {
    map.get(key)?.parse().ok()
}

pub(super) fn optional_u32(map: &BTreeMap<String, String>, key: &str) -> Option<u32> {
    map.get(key)?.parse().ok()
}

pub(crate) fn trim_block_body(lines: &[&str]) -> String {
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

pub(super) fn escape_attr(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

pub(super) fn kv_attr(key: &str, value: &str) -> String {
    format!("{key}={}", attr_token(value))
}

pub(super) fn quoted_attr(key: &str, value: &str) -> String {
    format!("{key}=\"{}\"", escape_attr(value))
}

pub(super) fn attr_token(s: &str) -> String {
    if s.chars().any(|c| c.is_whitespace() || c == '"' || c == '=') {
        format!("\"{}\"", escape_attr(s))
    } else {
        s.to_owned()
    }
}

fn unescape_alt(s: &str) -> String {
    s.replace("\\[", "[")
        .replace("\\]", "]")
        .replace("\\\\", "\\")
}

pub(super) fn parse_err(line: usize, column: usize, message: impl Into<String>) -> TesError {
    TesError::EditParse {
        line,
        column,
        message: message.into(),
    }
}
