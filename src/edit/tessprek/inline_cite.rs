//! Inline `\cite{key}` extraction for Tessprek / Markdown bodies.

use crate::error::{Result, TesError};
use crate::io::cite::PendingCite;

use super::brace::find_unquoted_close_brace;
use super::util::parse_attrs;

/// Rewrite inline `\cite{…}` forms to bare key text + [`PendingCite`] spans.
///
/// Leaves block-shaped attrs (`target_chunk`, `target_doc`, `target_byte_*`)
/// untouched so line-start block directives stay lossless if they appear in a
/// body by mistake.
///
/// # Errors
///
/// Returns [`TesError::EditParse`] for malformed `\cite{` openers / empty keys.
pub(crate) fn extract_inline_cites(body: &str) -> Result<(String, Vec<PendingCite>)> {
    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len());
    let mut pending = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if let Some(rest) = body[i..].strip_prefix("\\cite{") {
            let abs_open = i;
            let inner_start = abs_open + "\\cite{".len();
            let Some(rel_close) = find_unquoted_close_brace(rest) else {
                return Err(TesError::EditParse {
                    line: 1,
                    column: abs_open.saturating_add(1),
                    message: "unclosed \\cite{…}".into(),
                });
            };
            let inner = &rest[..rel_close];
            let abs_close_end = inner_start + rel_close + 1; // past `}`
            if let Some(key) = inline_cite_key(inner) {
                let start = u32::try_from(out.len()).unwrap_or(u32::MAX);
                out.push_str(&key);
                let end = u32::try_from(out.len()).unwrap_or(u32::MAX);
                pending.push(PendingCite { start, end, key });
                i = abs_close_end;
                continue;
            }
            // Block-shaped or unrecognized — copy through unchanged.
            out.push_str(&body[abs_open..abs_close_end]);
            i = abs_close_end;
            continue;
        }
        let Some(ch) = body[i..].chars().next() else {
            break;
        };
        out.push(ch);
        i += ch.len_utf8();
    }
    Ok((out, pending))
}

fn inline_cite_key(inner: &str) -> Option<String> {
    let inner = inner.trim();
    if inner.is_empty() {
        return None;
    }
    // Block-shaped attrs: leave alone.
    if inner.contains("target_chunk")
        || inner.contains("target_doc")
        || inner.contains("target_byte")
    {
        return None;
    }
    if !inner.contains('=') {
        return is_simple_cite_key(inner).then(|| inner.to_owned());
    }
    // Only key= / label= (optional whitespace), single attr.
    let map = parse_attrs(inner, 1).ok()?;
    if map.len() != 1 {
        return None;
    }
    let (k, v) = map.into_iter().next()?;
    if !matches!(k.as_str(), "key" | "label") {
        return None;
    }
    if v.is_empty() || !is_simple_cite_key(&v) {
        return None;
    }
    Some(v)
}

fn is_simple_cite_key(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_bare_key() {
        let (body, cites) = extract_inline_cites("see \\cite{keller2020} now").unwrap();
        assert_eq!(body, "see keller2020 now");
        assert_eq!(cites.len(), 1);
        assert_eq!(cites[0].key, "keller2020");
        assert_eq!(
            &body[cites[0].start as usize..cites[0].end as usize],
            "keller2020"
        );
    }

    #[test]
    fn extracts_label_attr() {
        let (body, cites) = extract_inline_cites("x \\cite{label=foo_bar} y").unwrap();
        assert_eq!(body, "x foo_bar y");
        assert_eq!(cites[0].key, "foo_bar");
    }

    #[test]
    fn leaves_block_shaped_cite() {
        let raw = "x \\cite{label=a target_chunk=3} y";
        let (body, cites) = extract_inline_cites(raw).unwrap();
        assert_eq!(body, raw);
        assert!(cites.is_empty());
    }
}
