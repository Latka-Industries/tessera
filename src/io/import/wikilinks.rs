//! `[[wikilink]]` scan / rewrite helpers for vault import.

/// One `[[target]]` / `[[target|label]]` span in Markdown source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WikilinkSpan<'a> {
    /// Byte offset of the opening `[[`.
    pub start: usize,
    /// Byte offset immediately after the closing `]]`.
    pub end: usize,
    /// Link target (left of `|`, trimmed).
    pub target: &'a str,
    /// Display label (right of `|`, or the full inner text when unlabeled).
    pub label: &'a str,
}

/// Invoke `visitor` for each Obsidian-style wikilink in `markdown`.
pub fn visit_wikilinks(markdown: &str, mut visitor: impl FnMut(WikilinkSpan<'_>)) {
    let bytes = markdown.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'['
            && bytes[i + 1] == b'['
            && let Some(close) = find_wikilink_end(markdown, i + 2)
        {
            let inner = &markdown[i + 2..close];
            let (target, label) = if let Some((t, l)) = inner.split_once('|') {
                (t.trim(), l.trim())
            } else {
                let t = inner.trim();
                (t, t)
            };
            visitor(WikilinkSpan {
                start: i,
                end: close + 2,
                target,
                label,
            });
            i = close + 2;
            continue;
        }
        i += 1;
    }
}

/// Collect wikilink targets for which `is_resolved` returns false (unique via `out`).
pub fn collect_unresolved_wikilinks(
    markdown: &str,
    is_resolved: impl Fn(&str) -> bool,
    out: &mut std::collections::HashSet<String>,
) {
    visit_wikilinks(markdown, |span| {
        if !span.target.is_empty() && !is_resolved(span.target) {
            out.insert(span.target.to_owned());
        }
    });
}

/// Rewrite `[[target]]` / `[[target|label]]` to `[label](uuid)` when `resolve` returns an id.
///
/// Unresolved wikilinks are left unchanged in the output.
#[must_use]
pub fn rewrite_wikilinks(markdown: &str, resolve: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut cursor = 0;
    visit_wikilinks(markdown, |span| {
        if let Some(uuid) = resolve(span.target) {
            out.push_str(&markdown[cursor..span.start]);
            out.push('[');
            out.push_str(span.label);
            out.push_str("](");
            out.push_str(&uuid);
            out.push(')');
            cursor = span.end;
        }
    });
    out.push_str(&markdown[cursor..]);
    out
}

fn find_wikilink_end(markdown: &str, start: usize) -> Option<usize> {
    let bytes = markdown.as_bytes();
    let mut j = start;
    while j + 1 < bytes.len() {
        if bytes[j] == b']' && bytes[j + 1] == b']' {
            return Some(j);
        }
        j += 1;
    }
    None
}
