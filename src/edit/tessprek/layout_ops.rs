//! Parse / emit Tessprek `\layout{…}` op lines (D24 / THI-363).

use crate::catalog::chunk::{InlineKind, InlineSpan};
use crate::catalog::layout::{
    EmAmount, LayoutOp, LayoutPayload, MeasureFrac, PlaceSkip, RuleWidth, VspaceAmount,
};
use crate::error::{Result, TesError};
use crate::io::font::PendingFont;

use super::brace::find_unquoted_close_brace;
use super::inline_font::extract_inline_fonts;
use super::util::{parse_attrs, parse_err};

/// Parse the raw brace-inner of `\layout{…}` into a payload.
///
/// Ops are whitespace-separated (newlines become spaces in
/// [`super::brace::take_brace_command`]). Each op starts with `place`,
/// `vspace`, `vspace=…`, or `rule`.
///
/// # Errors
///
/// Returns [`TesError::EditParse`] for unknown ops or invalid frac / attrs.
pub(crate) fn parse_layout_inner(inner: &str, line_no: usize) -> Result<LayoutPayload> {
    let mut ops = Vec::new();
    let mut rest = inner.trim();
    while !rest.is_empty() {
        let (op, next) = take_one_op(rest, line_no)?;
        ops.push(op);
        rest = next.trim_start();
    }
    let layout = LayoutPayload { ops };
    layout.validate().map_err(|e| match e {
        TesError::InvalidLayout { message } => parse_err(line_no, 1, message),
        other => other,
    })?;
    Ok(layout)
}

/// Emit one Tessprek line per op (for multiline `\layout{…}`).
#[must_use]
pub(crate) fn layout_op_parts(layout: &LayoutPayload) -> Vec<String> {
    layout.ops.iter().map(format_op_line).collect()
}

fn take_one_op(rest: &str, line_no: usize) -> Result<(LayoutOp, &str)> {
    let rest = rest.trim_start();
    if rest.is_empty() {
        return Err(parse_err(line_no, 1, "expected layout op"));
    }
    if let Some(after) = take_op_keyword(rest, "place", line_no)? {
        return take_place(after, line_no);
    }
    if let Some(after) = take_op_keyword(rest, "rule", line_no)? {
        return take_rule(after, line_no);
    }
    if let Some(after) = take_op_keyword(rest, "vspace", line_no)? {
        return take_vspace(after, line_no);
    }
    Err(parse_err(
        line_no,
        1,
        format!("unknown layout op near '{rest}' (expected place, vspace, or rule)"),
    ))
}

/// Match `name` at the start of `rest`, rejecting longer identifiers (`placeholder`).
fn take_op_keyword<'a>(rest: &'a str, name: &str, line_no: usize) -> Result<Option<&'a str>> {
    let Some(after) = rest.strip_prefix(name) else {
        return Ok(None);
    };
    if after.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_') {
        return Err(parse_err(
            line_no,
            1,
            format!("unknown layout op near '{rest}'"),
        ));
    }
    Ok(Some(after.trim_start()))
}

fn take_place(rest: &str, line_no: usize) -> Result<(LayoutOp, &str)> {
    let (attrs, content_brace, next) = take_op_attrs_and_optional_brace(rest, line_no)?;
    let skip = parse_place_skip(&attrs, line_no)?;
    let raw_content = if let Some(c) = content_brace {
        c
    } else if let Some(c) = attrs.get("content") {
        c.clone()
    } else {
        String::new()
    };
    let (content, pending_fonts) = extract_inline_fonts(&raw_content).map_err(|e| match e {
        TesError::EditParse {
            column, message, ..
        } => TesError::EditParse {
            line: line_no,
            column,
            message,
        },
        other => other,
    })?;
    let spans = pending_to_font_spans(&pending_fonts);
    Ok((
        LayoutOp::Place {
            skip,
            content,
            spans,
        },
        next,
    ))
}

fn take_vspace(rest: &str, line_no: usize) -> Result<(LayoutOp, &str)> {
    const VSPACE_NEED: &str = "vspace requires =small|=med|=big or em=…";
    // `vspace=small` compact form (no space after vspace).
    if let Some(after_eq) = rest.strip_prefix('=') {
        let (token, next) = split_token(after_eq);
        let amount = parse_vspace_named(token, line_no)?;
        return Ok((LayoutOp::Vspace { amount }, next));
    }
    let (attrs, _, next) = take_op_attrs_and_optional_brace(rest, line_no)?;
    let amount = if let Some(em) = attrs.get("em") {
        VspaceAmount::Em {
            em: parse_em(em, line_no)?,
        }
    } else if let Some(named) = attrs.get("amount").or_else(|| attrs.get("vspace")) {
        parse_vspace_named(named, line_no)?
    } else {
        return Err(parse_err(line_no, 1, VSPACE_NEED));
    };
    Ok((LayoutOp::Vspace { amount }, next))
}

fn take_rule(rest: &str, line_no: usize) -> Result<(LayoutOp, &str)> {
    let (attrs, _, next) = take_op_attrs_and_optional_brace(rest, line_no)?;
    let frac = match attrs.get("frac") {
        Some(v) => Some(parse_frac(v, line_no)?),
        None => None,
    };
    let em = match attrs.get("em") {
        Some(v) => Some(parse_em(v, line_no)?),
        None => None,
    };
    if frac.is_none() && em.is_none() {
        return Err(parse_err(line_no, 1, "rule requires frac= and/or em="));
    }
    Ok((
        LayoutOp::Rule {
            width: RuleWidth { frac, em },
        },
        next,
    ))
}

fn parse_place_skip(
    attrs: &std::collections::BTreeMap<String, String>,
    line_no: usize,
) -> Result<PlaceSkip> {
    let has_frac = attrs.contains_key("frac");
    let has_em = attrs.contains_key("em");
    match (has_frac, has_em) {
        (true, false) => Ok(PlaceSkip::Frac {
            frac: parse_frac(attrs.get("frac").unwrap(), line_no)?,
        }),
        (false, true) => Ok(PlaceSkip::Em {
            em: parse_em(attrs.get("em").unwrap(), line_no)?,
        }),
        (true, true) => Err(parse_err(
            line_no,
            1,
            "place accepts either frac= or em=, not both",
        )),
        (false, false) => Err(parse_err(line_no, 1, "place requires frac= or em=")),
    }
}

fn parse_vspace_named(raw: &str, line_no: usize) -> Result<VspaceAmount> {
    match raw.trim() {
        "small" => Ok(VspaceAmount::Small),
        "med" | "medium" => Ok(VspaceAmount::Med),
        "big" | "large" => Ok(VspaceAmount::Big),
        other => Err(parse_err(
            line_no,
            1,
            format!("unknown vspace amount '{other}' (expected small, med, or big)"),
        )),
    }
}

fn parse_frac(raw: &str, line_no: usize) -> Result<MeasureFrac> {
    let raw = raw.trim();
    let value: f32 = raw
        .parse()
        .map_err(|_| parse_err(line_no, 1, format!("invalid frac '{raw}' (expected 0..=1)")))?;
    MeasureFrac::try_from_f32(value).map_err(|e| match e {
        TesError::InvalidLayout { message } => parse_err(line_no, 1, message),
        other => other,
    })
}

fn parse_em(raw: &str, line_no: usize) -> Result<EmAmount> {
    let raw = raw.trim();
    let value: f32 = raw
        .parse()
        .map_err(|_| parse_err(line_no, 1, format!("invalid em '{raw}' (expected number)")))?;
    if !value.is_finite() {
        return Err(parse_err(line_no, 1, format!("invalid em '{raw}'")));
    }
    Ok(EmAmount::from_em(value))
}

/// Consume `key=value` pairs until the next op keyword, optional `{content}`, or end.
fn take_op_attrs_and_optional_brace(
    rest: &str,
    line_no: usize,
) -> Result<(
    std::collections::BTreeMap<String, String>,
    Option<String>,
    &str,
)> {
    let (attr_src, after_attrs) = split_attrs_before_next_op(rest);
    let mut map = if attr_src.trim().is_empty() {
        std::collections::BTreeMap::new()
    } else {
        parse_attrs(attr_src, line_no)?
    };
    let after = after_attrs.trim_start();
    if let Some(inner) = after.strip_prefix('{') {
        let Some(close) = find_unquoted_close_brace(inner) else {
            return Err(parse_err(line_no, 1, "unterminated layout content brace"));
        };
        let content = inner[..close].to_owned();
        let next = inner[close + 1..].trim_start();
        // Prefer brace body over content= when both present.
        map.remove("content");
        Ok((map, Some(content), next))
    } else {
        Ok((map, None, after))
    }
}

fn split_attrs_before_next_op(rest: &str) -> (&str, &str) {
    let mut i = 0usize;
    let mut in_quote = false;
    let mut escape = false;
    while i < rest.len() {
        let ch = rest[i..].chars().next().unwrap();
        let ch_len = ch.len_utf8();
        if escape {
            escape = false;
            i += ch_len;
            continue;
        }
        match ch {
            '\\' if in_quote => {
                escape = true;
                i += ch_len;
            }
            '"' => {
                in_quote = !in_quote;
                i += ch_len;
            }
            '{' if !in_quote => {
                // Content brace starts — stop attrs before it.
                break;
            }
            _ if !in_quote && at_op_boundary(rest, i) => {
                break;
            }
            _ => {
                i += ch_len;
            }
        }
    }
    (&rest[..i], &rest[i..])
}

fn at_op_boundary(s: &str, i: usize) -> bool {
    if i > 0 {
        let prev = s[..i].chars().next_back();
        if prev.is_some_and(|c| !c.is_whitespace()) {
            return false;
        }
    }
    let tail = &s[i..];
    for op in ["place", "vspace", "rule"] {
        if let Some(after) = tail.strip_prefix(op)
            && (after.is_empty()
                || after.starts_with(|c: char| c.is_whitespace() || c == '=' || c == '{'))
        {
            return true;
        }
    }
    false
}

fn split_token(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    let end = s.find(char::is_whitespace).unwrap_or(s.len());
    (&s[..end], s[end..].trim_start())
}

fn pending_to_font_spans(pending: &[PendingFont]) -> Vec<InlineSpan> {
    pending
        .iter()
        .map(|p| InlineSpan {
            start: p.start,
            end: p.end,
            kind: InlineKind::Font {
                font_id: p.font_id.clone(),
            },
        })
        .collect()
}

fn format_op_line(op: &LayoutOp) -> String {
    match op {
        LayoutOp::Place {
            skip,
            content,
            spans,
        } => {
            let mut parts = Vec::new();
            parts.push("place".into());
            match skip {
                PlaceSkip::Frac { frac } => {
                    parts.push(format!("frac={}", frac.tessprek_token()));
                }
                PlaceSkip::Em { em } => {
                    parts.push(format!("em={}", em.tessprek_token()));
                }
            }
            let rendered = render_place_content(content, spans);
            if !rendered.is_empty() {
                parts.push(format!("content=\"{}\"", escape_attr(&rendered)));
            }
            parts.join(" ")
        }
        LayoutOp::Vspace { amount } => match amount {
            VspaceAmount::Small => "vspace=small".into(),
            VspaceAmount::Med => "vspace=med".into(),
            VspaceAmount::Big => "vspace=big".into(),
            VspaceAmount::Em { em } => format!("vspace em={}", em.tessprek_token()),
        },
        LayoutOp::Rule { width } => {
            let mut parts = vec!["rule".into()];
            if let Some(frac) = width.frac {
                parts.push(format!("frac={}", frac.tessprek_token()));
            }
            if let Some(em) = width.em {
                parts.push(format!("em={}", em.tessprek_token()));
            }
            parts.join(" ")
        }
    }
}

fn render_place_content(content: &str, spans: &[InlineSpan]) -> String {
    if spans.is_empty() {
        return content.to_owned();
    }
    // Same reverse-rewrite strategy as text Tessprek encode for `\font`.
    let mut out = content.to_owned();
    let mut fonts: Vec<_> = spans
        .iter()
        .filter(|s| matches!(s.kind, InlineKind::Font { .. }))
        .collect();
    fonts.sort_by_key(|s| std::cmp::Reverse(s.start));
    for span in fonts {
        let InlineKind::Font { font_id } = &span.kind else {
            continue;
        };
        let start = span.start as usize;
        let end = span.end as usize;
        if end > out.len() || start > end {
            continue;
        }
        let inner = out[start..end].to_owned();
        let replacement = format!("\\font{{{font_id}}}{{{inner}}}");
        out.replace_range(start..end, &replacement);
    }
    out
}

fn escape_attr(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_progressish_layout() {
        let layout = parse_layout_inner(
            r#"vspace=small place frac=0.875 content="87.5%" vspace=small"#,
            1,
        )
        .unwrap();
        assert_eq!(layout.ops.len(), 3);
        assert!(matches!(
            &layout.ops[1],
            LayoutOp::Place {
                skip: PlaceSkip::Frac { frac },
                content,
                ..
            } if frac.bps == 8750 && content == "87.5%"
        ));
    }

    #[test]
    fn parse_place_brace_content_and_font() {
        let layout = parse_layout_inner(r#"place frac=1 {\font{armenian}{barev}}"#, 1).unwrap();
        let LayoutOp::Place { content, spans, .. } = &layout.ops[0] else {
            panic!("expected place");
        };
        assert_eq!(content, "barev");
        assert_eq!(spans.len(), 1);
        assert!(matches!(&spans[0].kind, InlineKind::Font { font_id } if font_id == "armenian"));
    }

    #[test]
    fn rejects_unknown_op() {
        let err = parse_layout_inner("gauge value=1", 1).unwrap_err();
        assert!(matches!(err, TesError::EditParse { .. }));
    }

    #[test]
    fn rejects_bad_frac() {
        let err = parse_layout_inner("place frac=1.5 content=\"x\"", 1).unwrap_err();
        assert!(matches!(err, TesError::EditParse { .. }));
    }

    #[test]
    fn round_trip_op_lines() {
        let layout =
            parse_layout_inner(r#"place frac=1 content="▸" vspace=med rule frac=1"#, 1).unwrap();
        let parts = layout_op_parts(&layout);
        let again = parse_layout_inner(&parts.join(" "), 1).unwrap();
        assert_eq!(again, layout);
    }
}
