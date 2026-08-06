//! Pack `typography.toml` + `aliases.toml` → expand once into Tessprek text (D23 / THI-354).
//!
//! Applied at `tes format` / edit-write compile. Sealed `.tes` stores the expanded
//! Unicode / strings — no live macros.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::template::{
    DEFAULT_ALIASES_NAME, DEFAULT_TYPOGRAPHY_NAME, TemplatePack, resolve_template_id,
};
use crate::error::{Result, TesError};

/// Loaded pack text-expansion rules (may be empty).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackTextRules {
    /// Plain string substitutions (`...` → `…`), longest `from` first.
    pub substitutions: Vec<(String, String)>,
    /// Named aliases expanded as `\name` → value, longest name first.
    pub aliases: Vec<(String, String)>,
}

impl PackTextRules {
    /// True when no rules are loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.substitutions.is_empty() && self.aliases.is_empty()
    }

    /// Apply aliases then typography substitutions, skipping fenced code bodies.
    #[must_use]
    pub fn apply(&self, input: &str) -> String {
        if self.is_empty() {
            return input.to_owned();
        }
        apply_outside_fences(input, |chunk| {
            let mut s = expand_aliases(chunk, &self.aliases);
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
    let id = resolve_template_id(template_id, catalog_template_id);
    match TemplatePack::resolve(template_root, id) {
        Ok(pack) => pack_text_rules(&pack),
        Err(TesError::TemplateNotFound { .. }) => Ok(PackTextRules::default()),
        Err(err) => Err(err),
    }
}

/// Load typography + aliases overlays for a pack.
///
/// # Errors
///
/// Returns [`TesError::InvalidTemplate`] / [`TesError::Io`] for bad overlays.
pub fn pack_text_rules(pack: &TemplatePack) -> Result<PackTextRules> {
    let mut rules = PackTextRules::default();
    if let Some(path) = pack.typography_path() {
        let file: TypographyFile = load_overlay_toml(pack, &path, DEFAULT_TYPOGRAPHY_NAME)?;
        rules.substitutions = sorted_pairs(file.substitutions);
    }
    if let Some(path) = pack.aliases_path() {
        let file: AliasesFile = load_overlay_toml(pack, &path, DEFAULT_ALIASES_NAME)?;
        for name in file.aliases.keys() {
            validate_alias_name(name)?;
        }
        rules.aliases = sorted_pairs(file.aliases);
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

fn load_overlay_toml<T: for<'de> Deserialize<'de>>(
    pack: &TemplatePack,
    path: &Path,
    fallback_name: &str,
) -> Result<T> {
    let raw = fs::read_to_string(path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            TesError::InvalidTemplate {
                message: format!("pack text overlay missing: {}", path.display()),
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

fn validate_alias_name(name: &str) -> Result<()> {
    if name.is_empty()
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        || !name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
    {
        return Err(TesError::InvalidTemplate {
            message: format!(
                "alias name must be an ASCII identifier (got {name:?}); expanded as \\{name}"
            ),
        });
    }
    Ok(())
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
                    // Do not treat `\name{…}` as an alias (phrase/face territory).
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

/// Map each non-fenced region; leave Markdown fenced code bodies untouched.
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
        };
        assert_eq!(
            rules.apply("Wait... go -> \\maryamlatin now"),
            "Wait… go → Maryam now"
        );
    }

    #[test]
    fn skips_fenced_code() {
        let rules = PackTextRules {
            substitutions: vec![("...".into(), "…".into())],
            aliases: vec![],
        };
        let input = "Prose...\n```\nkeep...\n```\nAfter...\n";
        assert_eq!(rules.apply(input), "Prose…\n```\nkeep...\n```\nAfter…\n");
    }

    #[test]
    fn braced_alias_left_alone() {
        let rules = PackTextRules {
            substitutions: vec![],
            aliases: vec![("phrase".into(), "NOPE".into())],
        };
        assert_eq!(rules.apply(r"\phrase{x}"), r"\phrase{x}");
    }

    #[test]
    fn minimal_pack_loads_typography() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates/minimal");
        let pack = TemplatePack::load(&root).unwrap();
        let rules = pack_text_rules(&pack).unwrap();
        assert!(!rules.substitutions.is_empty());
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
