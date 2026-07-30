//! UTF-16 LSP position helpers shared by hover / diagnostics / didChange.

use tower_lsp::lsp_types::{Position, Range};

/// Return `(0-based line index, line text)` for an LSP line, if present.
pub(super) fn nth_line(text: &str, line: u32) -> Option<(u32, &str)> {
    text.lines()
        .nth(usize::try_from(line).ok()?)
        .map(|s| (line, s))
}

/// UTF-16 code-unit length of `s` (LSP column units).
pub(super) fn utf16_len(s: &str) -> u32 {
    s.chars()
        .map(|c| u32::try_from(c.len_utf16()).unwrap_or(0))
        .sum()
}

/// Map 1-based Tessprek `EditParse` line/column onto an LSP [`Range`].
///
/// Column `1` (the common case from the parser) highlights the whole line.
/// Otherwise the range starts at the given column and runs to end-of-line.
pub(super) fn line_column_range(text: &str, line_1based: usize, column_1based: usize) -> Range {
    let line_idx = u32::try_from(line_1based.saturating_sub(1)).unwrap_or(0);
    let Some((_, line)) = nth_line(text, line_idx) else {
        return Range {
            start: Position {
                line: line_idx,
                character: 0,
            },
            end: Position {
                line: line_idx,
                character: 1,
            },
        };
    };
    let line_len = utf16_len(line);
    if column_1based <= 1 {
        return Range {
            start: Position {
                line: line_idx,
                character: 0,
            },
            end: Position {
                line: line_idx,
                character: line_len.max(1),
            },
        };
    }
    let start_ch = u32::try_from(column_1based - 1).unwrap_or(0).min(line_len);
    Range {
        start: Position {
            line: line_idx,
            character: start_ch,
        },
        end: Position {
            line: line_idx,
            character: line_len.max(start_ch.saturating_add(1)),
        },
    }
}

/// LSP positions are UTF-16 code units; map to a UTF-8 byte offset.
pub(super) fn position_to_utf8_offset(text: &str, pos: Position) -> Option<usize> {
    let mut line = 0u32;
    let mut utf16_col = 0u32;
    for (byte_idx, ch) in text.char_indices() {
        if line == pos.line && utf16_col == pos.character {
            return Some(byte_idx);
        }
        if ch == '\n' {
            if line == pos.line {
                return None;
            }
            line += 1;
            utf16_col = 0;
        } else {
            utf16_col += u32::try_from(ch.len_utf16()).ok()?;
        }
    }
    if line == pos.line && utf16_col == pos.character {
        return Some(text.len());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_line_when_column_one() {
        let text = "a\nbad line here\nc\n";
        let range = line_column_range(text, 2, 1);
        assert_eq!(range.start.line, 1);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 1);
        assert_eq!(range.end.character, utf16_len("bad line here"));
    }

    #[test]
    fn column_offset_to_eol() {
        let text = "hello world\n";
        let range = line_column_range(text, 1, 7);
        assert_eq!(range.start.character, 6);
        assert_eq!(range.end.character, utf16_len("hello world"));
    }
}
