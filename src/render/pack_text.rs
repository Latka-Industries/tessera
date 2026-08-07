//! Pack `typography.toml` / `aliases.toml` / `phrases.toml` → expand into Tessprek
//! text (D23 / THI-354 / THI-355).
//!
//! Applied at `tes format` / edit-write compile. Sealed `.tes` stores the expanded
//! Unicode / styled prose — no live macros or phrase ids.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::template::{
    DEFAULT_ALIASES_NAME, DEFAULT_PHRASES_NAME, DEFAULT_TYPOGRAPHY_NAME, TemplatePack,
    with_resolved_pack,
};
use crate::catalog::chunk::is_ascii_ident;
use crate::error::{Result, TesError};

/// Loaded pack text-expansion rules (may be empty).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackTextRules {
    /// Plain string substitutions (`...` → `…`), longest `from` first.
    pub substitutions: Vec<(String, String)>,
    /// Named aliases expanded as `\name` → value, longest name first.
    pub aliases: Vec<(String, String)>,
    /// Phrase templates expanded as `\phrase{key}` / `\phrase{key}{arg}`.
    pub phrases: Vec<(String, String)>,
}

impl PackTextRules {
    /// True when no rules are loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.substitutions.is_empty() && self.aliases.is_empty() && self.phrases.is_empty()
    }

    /// Apply phrases, then aliases, then typography substitutions.
    ///
    /// Skips fenced code bodies and Markdown hyphen structure lines (GFM table
    /// separators, thematic breaks, setext underlines) so rules like `"--" → "–"`
    /// do not rewrite `| --- |` into a non-table.
    #[must_use]
    pub fn apply(&self, input: &str) -> String {
        if self.is_empty() {
            return input.to_owned();
        }
        apply_outside_fences(input, |chunk| {
            let mut s = expand_phrases(chunk, &self.phrases);
            s = expand_aliases(&s, &self.aliases);
            for (from, to) in &self.substitutions {
                if !from.is_empty() {
                    s = s.replace(from, to);
                }
            }
            s
        })
    }
}

/// Resolve pack text rules: missing pack → empty rules; bad TOML → error.
///
/// # Errors
///
/// Returns [`TesError::InvalidTemplate`] when an overlay file is present but invalid.
pub fn resolve_pack_text(
    template_root: impl AsRef<Path>,
    template_id: Option<&str>,
    catalog_template_id: Option<&str>,
) -> Result<PackTextRules> {
    with_resolved_pack(
        template_root,
        template_id,
        catalog_template_id,
        PackTextRules::default(),
        pack_text_rules,
    )
}

/// Load typography + aliases + phrases overlays for a pack.
///
/// Sparse files and/or master `tessera.toml` sections (THI-367). A concern present
/// in both forms is a hard error.
///
/// # Errors
///
/// Returns [`TesError::InvalidTemplate`] / [`TesError::Io`] for bad overlays.
pub fn pack_text_rules(pack: &TemplatePack) -> Result<PackTextRules> {
    use super::pack_master::{PackConcern, load_pack_master, resolve_map_concern};

    let master = load_pack_master(pack)?;
    let mut rules = PackTextRules::default();

    if let Some(map) = resolve_map_concern(
        pack,
        master.as_ref(),
        PackConcern::Typography,
        pack.typography_path(),
        |path| {
            let file: TypographyFile = load_overlay_toml(pack, path, DEFAULT_TYPOGRAPHY_NAME)?;
            Ok(file.substitutions)
        },
    )? {
        rules.substitutions = sorted_pairs(map);
    }

    if let Some(map) = resolve_map_concern(
        pack,
        master.as_ref(),
        PackConcern::Aliases,
        pack.aliases_path(),
        |path| {
            let file: AliasesFile = load_overlay_toml(pack, path, DEFAULT_ALIASES_NAME)?;
            Ok(file.aliases)
        },
    )? {
        for name in map.keys() {
            validate_ident(name, "alias")?;
        }
        rules.aliases = sorted_pairs(map);
    }

    if let Some(map) = resolve_map_concern(
        pack,
        master.as_ref(),
        PackConcern::Phrases,
        pack.phrases_path(),
        |path| {
            let file: PhrasesFile = load_overlay_toml(pack, path, DEFAULT_PHRASES_NAME)?;
            Ok(file.phrases)
        },
    )? {
        for name in map.keys() {
            validate_ident(name, "phrase")?;
        }
        rules.phrases = sorted_pairs(map);
    }

    Ok(rules)
}

#[derive(Debug, Default, Deserialize)]
struct TypographyFile {
    #[serde(default)]
    substitutions: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
struct AliasesFile {
    #[serde(default)]
    aliases: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
struct PhrasesFile {
    #[serde(default)]
    phrases: std::collections::BTreeMap<String, String>,
}

pub(crate) fn load_overlay_toml<T: for<'de> Deserialize<'de>>(
    pack: &TemplatePack,
    path: &Path,
    fallback_name: &str,
) -> Result<T> {
    let raw = fs::read_to_string(path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            TesError::InvalidTemplate {
                message: format!("pack overlay missing: {}", path.display()),
            }
        } else {
            TesError::Io(err)
        }
    })?;
    toml::from_str(&raw).map_err(|e| TesError::InvalidTemplate {
        message: format!(
            "pack '{}' {}: {e}",
            pack.manifest.id,
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(fallback_name)
        ),
    })
}

fn sorted_pairs(map: std::collections::BTreeMap<String, String>) -> Vec<(String, String)> {
    let mut pairs: Vec<_> = map.into_iter().collect();
    pairs.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
    pairs
}

pub(crate) fn validate_ident(name: &str, kind: &str) -> Result<()> {
    if !is_ascii_ident(name) {
        return Err(TesError::InvalidTemplate {
            message: format!("{kind} name must be an ASCII identifier (got {name:?})"),
        });
    }
    Ok(())
}

fn expand_phrases(input: &str, phrases: &[(String, String)]) -> String {
    if phrases.is_empty() {
        return input.to_owned();
    }
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && match_phrase_opener(&chars, i) {
            let key_start = i + "\\phrase{".chars().count();
            if let Some((key_end, key)) = take_brace_inner(&chars, key_start) {
                let mut j = key_end;
                let mut arg = String::new();
                if j < chars.len() && chars[j] == '{' {
                    if let Some((arg_end, a)) = take_brace_inner(&chars, j + 1) {
                        arg = a;
                        j = arg_end;
                    } else {
                        out.push(chars[i]);
                        i += 1;
                        continue;
                    }
                }
                if let Some((_, template)) = phrases.iter().find(|(n, _)| n == &key) {
                    out.push_str(&substitute_arg(template, &arg));
                    i = j;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn match_phrase_opener(chars: &[char], i: usize) -> bool {
    const OPENER: &[char] = &['\\', 'p', 'h', 'r', 'a', 's', 'e', '{'];
    chars[i..].starts_with(OPENER)
}

fn take_brace_inner(chars: &[char], start: usize) -> Option<(usize, String)> {
    let mut depth = 1usize;
    let mut j = start;
    while j < chars.len() {
        match chars[j] {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let inner: String = chars[start..j].iter().collect();
                    return Some((j + 1, inner));
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

fn substitute_arg(template: &str, arg: &str) -> String {
    template.replace("{arg}", arg).replace("$1", arg)
}

fn expand_aliases(input: &str, aliases: &[(String, String)]) -> String {
    if aliases.is_empty() {
        return input.to_owned();
    }
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' {
            let start = i + 1;
            let mut end = start;
            while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '_') {
                end += 1;
            }
            if end > start {
                let name: String = chars[start..end].iter().collect();
                if let Some((_, value)) = aliases.iter().find(|(n, _)| n == &name) {
                    // Do not treat `\name{…}` as an alias (phrase/font territory).
                    if end >= chars.len() || chars[end] != '{' {
                        out.push_str(value);
                        i = end;
                        continue;
                    }
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Map each unprotected line; leave fenced code and hyphen structure untouched.
fn apply_outside_fences(input: &str, mut f: impl FnMut(&str) -> String) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_fence = false;
    let mut fence_marker: Option<&str> = None;
    for line in input.split_inclusive('\n') {
        let trimmed = line.trim();
        if in_fence {
            if fence_close(trimmed, fence_marker.unwrap_or("```")) {
                in_fence = false;
                fence_marker = None;
            }
            out.push_str(line);
        } else {
            if let Some(marker) = fence_open(trimmed) {
                in_fence = true;
                fence_marker = Some(marker);
                out.push_str(line);
                continue;
            }
            let (body, nl) = split_trailing_nl(line);
            // Keep GFM `| --- |` / thematic `---` as ASCII hyphens.
            if crate::io::import::is_markdown_hyphen_structure(body) {
                out.push_str(line);
                continue;
            }
            out.push_str(&f(body));
            out.push_str(nl);
        }
    }
    out
}

fn fence_open(trimmed: &str) -> Option<&'static str> {
    if trimmed.starts_with("```") {
        Some("```")
    } else if trimmed.starts_with("~~~") {
        Some("~~~")
    } else {
        None
    }
}

fn fence_close(trimmed: &str, marker: &str) -> bool {
    let ch = marker.chars().next().unwrap_or('`');
    trimmed.starts_with(marker) && trimmed.chars().all(|c| c == ch)
}

fn split_trailing_nl(line: &str) -> (&str, &str) {
    if let Some(stripped) = line.strip_suffix('\n') {
        if let Some(stripped) = stripped.strip_suffix('\r') {
            (stripped, "\r\n")
        } else {
            (stripped, "\n")
        }
    } else {
        (line, "")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    #[test]
    fn typography_and_aliases_expand() {
        let rules = PackTextRules {
            substitutions: vec![("...".into(), "…".into()), ("->".into(), "→".into())],
            aliases: vec![("maryamlatin".into(), "Maryam".into())],
            phrases: vec![],
        };
        assert_eq!(
            rules.apply("Wait... go -> \\maryamlatin now"),
            "Wait… go → Maryam now"
        );
    }

    #[test]
    fn phrase_expands_with_arg() {
        let rules = PackTextRules {
            substitutions: vec![],
            aliases: vec![],
            phrases: vec![("yegourdoon".into(), "*{arg}*".into())],
        };
        assert_eq!(rules.apply(r"\phrase{yegourdoon}{I am Yes}"), "*I am Yes*");
    }

    #[test]
    fn phrase_without_arg_uses_empty_slot() {
        let rules = PackTextRules {
            substitutions: vec![],
            aliases: vec![],
            phrases: vec![("greet".into(), "Hello, {arg}.".into())],
        };
        assert_eq!(rules.apply(r"\phrase{greet}"), "Hello, .");
    }

    #[test]
    fn phrase_then_typography() {
        let rules = PackTextRules {
            substitutions: vec![("...".into(), "…".into())],
            aliases: vec![],
            phrases: vec![("pause".into(), "Wait...".into())],
        };
        assert_eq!(rules.apply(r"\phrase{pause}"), "Wait…");
    }

    #[test]
    fn unknown_phrase_left_alone() {
        let rules = PackTextRules {
            substitutions: vec![],
            aliases: vec![],
            phrases: vec![("known".into(), "x".into())],
        };
        assert_eq!(rules.apply(r"\phrase{unknown}{a}"), r"\phrase{unknown}{a}");
    }

    #[test]
    fn skips_fenced_code() {
        let rules = PackTextRules {
            substitutions: vec![("...".into(), "…".into())],
            aliases: vec![],
            phrases: vec![],
        };
        let input = "Prose...\n```\nkeep...\n```\nAfter...\n";
        assert_eq!(rules.apply(input), "Prose…\n```\nkeep...\n```\nAfter…\n");
    }

    fn dash_typography() -> PackTextRules {
        PackTextRules {
            substitutions: vec![("--".into(), "–".into())],
            aliases: vec![],
            phrases: vec![],
        }
    }

    #[test]
    fn typography_skips_gfm_table_separators() {
        let out = dash_typography().apply("| A | B |\n| --- | --- |\n| x -- y | z |\n");
        assert!(out.contains("| --- | --- |"), "{out}");
        assert!(out.contains("x – y"), "{out}");
    }

    #[test]
    fn typography_skips_thematic_break_hyphens() {
        assert_eq!(
            dash_typography().apply("before\n---\nafter -- ok\n"),
            "before\n---\nafter – ok\n"
        );
    }

    #[test]
    fn braced_alias_left_alone() {
        let rules = PackTextRules {
            substitutions: vec![],
            aliases: vec![("phrase".into(), "NOPE".into())],
            phrases: vec![],
        };
        assert_eq!(rules.apply(r"\phrase{x}"), r"\phrase{x}");
    }

    #[test]
    fn master_pack_fixture_loads_text_rules() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates/master_pack");
        let pack = TemplatePack::load(&root).unwrap();
        let rules = pack_text_rules(&pack).unwrap();
        assert!(
            rules
                .substitutions
                .iter()
                .any(|(from, to)| from == "..." && to == "…"),
            "{rules:?}"
        );
        assert_eq!(rules.apply(r"\phrase{yegourdoon}{hi}"), "*hi*");
    }

    #[test]
    fn master_and_sparse_typography_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let mut file = fs::File::create(dir.path().join("manifest.json")).unwrap();
        write!(
            file,
            r#"{{"id":"conflict","version":"0.0.1","themes":{{"draft":"draft.css"}}}}"#
        )
        .unwrap();
        fs::write(dir.path().join("draft.css"), "body{}").unwrap();
        fs::write(
            dir.path().join("tessera.toml"),
            "[typography]\n\"...\" = \"…\"\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("typography.toml"),
            "[substitutions]\n\"->\" = \"→\"\n",
        )
        .unwrap();
        let pack = TemplatePack::load(dir.path()).unwrap();
        let err = pack_text_rules(&pack).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("both"), "{msg}");
    }

    #[test]
    fn minimal_pack_loads_phrases() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates/minimal");
        let pack = TemplatePack::load(&root).unwrap();
        let rules = pack_text_rules(&pack).unwrap();
        assert!(!rules.phrases.is_empty());
        assert_eq!(rules.apply(r"\phrase{yegourdoon}{I am Yes}"), "*I am Yes*");
        assert!(rules.apply("a...b").contains('…'));
    }

    #[test]
    fn missing_pack_empty_rules() {
        let rules = resolve_pack_text("/no/such/templates", None, None).unwrap();
        assert!(rules.is_empty());
    }

    #[test]
    fn rejects_bad_alias_name() {
        let dir = tempfile::tempdir().unwrap();
        let mut file = fs::File::create(dir.path().join("manifest.json")).unwrap();
        write!(
            file,
            r#"{{"id":"bad","version":"0.0.1","themes":{{"draft":"draft.css"}}}}"#
        )
        .unwrap();
        fs::write(dir.path().join("draft.css"), "body{}").unwrap();
        fs::write(
            dir.path().join("aliases.toml"),
            "[aliases]\n\"bad-name\" = \"x\"\n",
        )
        .unwrap();
        let pack = TemplatePack::load(dir.path()).unwrap();
        let err = pack_text_rules(&pack).unwrap_err();
        assert!(matches!(err, TesError::InvalidTemplate { .. }));
    }
}
