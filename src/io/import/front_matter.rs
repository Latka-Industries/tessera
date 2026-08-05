use super::MarkdownFrontMatter;

/// Split Obsidian-style YAML front matter from the Markdown body.
#[must_use]
pub fn parse_front_matter(source: &str) -> (MarkdownFrontMatter, &str) {
    let Some(after_open) = source
        .strip_prefix("---\n")
        .or_else(|| source.strip_prefix("---\r\n"))
    else {
        return (MarkdownFrontMatter::default(), source);
    };
    let Some(end) = after_open
        .find("\n---\n")
        .or_else(|| after_open.find("\n---\r\n"))
    else {
        return (MarkdownFrontMatter::default(), source);
    };
    let front = &after_open[..end];
    let body_start = if after_open[end..].starts_with("\n---\r\n") {
        end + "\n---\r\n".len()
    } else {
        end + "\n---\n".len()
    };
    (parse_front_matter_body(front), &after_open[body_start..])
}

fn parse_front_matter_body(front: &str) -> MarkdownFrontMatter {
    let mut out = MarkdownFrontMatter::default();
    let mut lines = front.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("title:") {
            out.title = Some(unquote(rest.trim()));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("id:") {
            let value = unquote(rest.trim());
            if !value.is_empty() {
                out.id = Some(value);
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("tags:") {
            out.tags = parse_yaml_string_list(rest.trim(), &mut lines);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("aliases:") {
            out.aliases = parse_yaml_string_list(rest.trim(), &mut lines);
        }
    }
    out
}

fn parse_yaml_string_list<'a>(
    inline: &str,
    lines: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
) -> Vec<String> {
    if inline == "[]" {
        return Vec::new();
    }
    if inline.starts_with('[') && inline.ends_with(']') {
        return inline[1..inline.len() - 1]
            .split(',')
            .map(|s| unquote(s.trim()))
            .filter(|s| !s.is_empty())
            .collect();
    }
    if !inline.is_empty() {
        return vec![unquote(inline)];
    }
    let mut items = Vec::new();
    while let Some(next) = lines.peek() {
        let t = next.trim();
        if let Some(item) = t.strip_prefix("- ") {
            items.push(unquote(item.trim()));
            lines.next();
        } else if t.is_empty() {
            lines.next();
        } else {
            break;
        }
    }
    items
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        value[1..value.len() - 1].to_owned()
    } else {
        value.to_owned()
    }
}
