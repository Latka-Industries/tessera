//! External template / theme packs (`docs/structure_v1.md`).
//!
//! Pack bytes stay outside `.tes`. The document catalog may reference a pack
//! by `template_id` / `theme_id`; this module loads the folder + manifest.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, TesError};

/// Filename of a pack manifest.
pub const MANIFEST_NAME: &str = "manifest.json";

/// Built-in pack id shipped under `templates/minimal`.
pub const DEFAULT_TEMPLATE_ID: &str = "minimal";

/// Screen-oriented theme id.
pub const THEME_DRAFT: &str = "draft";

/// Print-oriented theme id (shared preview/PDF path).
pub const THEME_PRINT: &str = "print";

/// Versioned pack manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateManifest {
    /// Stable pack id (folder name by convention).
    pub id: String,
    /// Semver-ish pack version string.
    pub version: String,
    /// Highest Tessera layout version this pack claims to understand.
    #[serde(default)]
    pub compatible_layout: u32,
    /// Default `doc_kind` when creating from a starter.
    #[serde(default)]
    pub doc_kind_default: Option<String>,
    /// Citation style id or cite-style pack reference.
    #[serde(default)]
    pub cite_style_id: Option<String>,
    /// Theme id → CSS path relative to the pack root.
    pub themes: BTreeMap<String, String>,
    /// Named slide regions for deck templates.
    #[serde(default)]
    pub slide_regions: Vec<String>,
    /// Optional allow-list of block roles.
    #[serde(default)]
    pub allowed_blocks: Vec<String>,
    /// Declared export targets (`html`, `pdf`, …).
    #[serde(default)]
    pub export_targets: Vec<String>,
    /// Optional starter Tessera Markdown path relative to the pack root.
    #[serde(default)]
    pub starter: Option<String>,
    /// When true, pack ships JS that requires `--allow-theme-js`.
    #[serde(default)]
    pub requires_theme_js: bool,
}

/// A loaded pack directory + parsed manifest.
#[derive(Debug, Clone)]
pub struct TemplatePack {
    /// Absolute or resolved pack root.
    pub root: PathBuf,
    /// Parsed `manifest.json`.
    pub manifest: TemplateManifest,
}

impl TemplatePack {
    /// Load `root/manifest.json` and validate basic invariants.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::TemplateNotFound`] if `manifest.json` is missing,
    /// [`TesError::Json`] / [`TesError::InvalidTemplate`] if the manifest is invalid,
    /// or [`TesError::Io`] on other filesystem errors.
    pub fn load(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let manifest_path = root.join(MANIFEST_NAME);
        let raw = fs::read_to_string(&manifest_path).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                TesError::TemplateNotFound {
                    id_or_path: root.display().to_string(),
                }
            } else {
                TesError::Io(err)
            }
        })?;
        let manifest: TemplateManifest = serde_json::from_str(&raw)?;
        if manifest.id.trim().is_empty() {
            return Err(TesError::InvalidTemplate {
                message: "manifest id must be non-empty".into(),
            });
        }
        if manifest.themes.is_empty() {
            return Err(TesError::InvalidTemplate {
                message: "manifest must declare at least one theme".into(),
            });
        }
        if manifest.requires_theme_js {
            // Packs may declare JS, but the default serve path refuses it.
            // Callers that pass `--allow-theme-js` can opt in later.
        }
        for (theme_id, rel) in &manifest.themes {
            validate_pack_relative(rel, &format!("theme '{theme_id}'"))?;
            let css_path = root.join(rel);
            if !css_path.is_file() {
                return Err(TesError::InvalidTemplate {
                    message: format!("theme '{theme_id}' CSS missing: {}", css_path.display()),
                });
            }
        }
        if let Some(starter) = &manifest.starter {
            validate_pack_relative(starter, "starter")?;
        }
        Ok(Self { root, manifest })
    }

    /// Resolve a pack by id under `template_root/{id}`.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::InvalidTemplate`] for a bad pack id, or errors from [`Self::load`].
    pub fn resolve(template_root: impl AsRef<Path>, id: &str) -> Result<Self> {
        let id = id.trim();
        if id.is_empty() || id.contains(['/', '\\']) || id == "." || id == ".." {
            return Err(TesError::InvalidTemplate {
                message: format!("invalid template id '{id}'"),
            });
        }
        Self::load(template_root.as_ref().join(id))
    }

    /// Read CSS for `theme_id` (`draft`, `print`, or a pack-defined id).
    ///
    /// # Errors
    ///
    /// Returns [`TesError::ThemeNotFound`] if `theme_id` is undeclared, or [`TesError::Io`]
    /// if the CSS file cannot be read.
    pub fn theme_css(&self, theme_id: &str) -> Result<String> {
        let rel = self
            .manifest
            .themes
            .get(theme_id)
            .ok_or_else(|| TesError::ThemeNotFound {
                template_id: self.manifest.id.clone(),
                theme_id: theme_id.to_string(),
            })?;
        let path = self.root.join(rel);
        Ok(fs::read_to_string(path)?)
    }

    /// Relative CSS path for a theme, for `/theme.css` serving.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::ThemeNotFound`] if `theme_id` is not declared by the pack.
    pub fn theme_relative_path(&self, theme_id: &str) -> Result<&str> {
        self.manifest
            .themes
            .get(theme_id)
            .map(String::as_str)
            .ok_or_else(|| TesError::ThemeNotFound {
                template_id: self.manifest.id.clone(),
                theme_id: theme_id.to_string(),
            })
    }
}

/// Default theme when the catalog or CLI does not pick one.
pub fn default_theme_id(pack: &TemplatePack) -> &str {
    if pack.manifest.themes.contains_key(THEME_DRAFT) {
        THEME_DRAFT
    } else {
        pack.manifest
            .themes
            .keys()
            .next()
            .map_or(THEME_DRAFT, String::as_str)
    }
}

/// How to pick a theme when neither CLI nor catalog specifies one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeFallback {
    /// Prefer [`THEME_DRAFT`], else the first declared theme.
    Draft,
    /// Prefer [`THEME_PRINT`] when present, else [`default_theme_id`].
    PreferPrint,
}

/// Pack + theme selected for preview or print HTML.
#[derive(Debug, Clone)]
pub struct ResolvedPackTheme {
    /// Loaded template pack.
    pub pack: TemplatePack,
    /// Selected theme id (`draft`, `print`, …).
    pub theme_id: String,
}

/// Resolve pack/theme from CLI overrides, catalog fields, and a fallback policy.
///
/// Order: explicit override → catalog → fallback. Validates that the theme CSS
/// exists on disk.
///
/// # Errors
///
/// Returns template/theme errors from [`TemplatePack::resolve`] /
/// [`TemplatePack::theme_css`].
pub fn resolve_pack_and_theme(
    catalog_template_id: Option<&str>,
    catalog_theme_id: Option<&str>,
    template_root: impl AsRef<Path>,
    template_id: Option<&str>,
    theme_id: Option<&str>,
    fallback: ThemeFallback,
) -> Result<ResolvedPackTheme> {
    let template_id = template_id
        .or(catalog_template_id)
        .unwrap_or(DEFAULT_TEMPLATE_ID);
    let pack = TemplatePack::resolve(template_root, template_id)?;
    let theme_id = theme_id
        .map(str::to_string)
        .or_else(|| catalog_theme_id.map(str::to_string))
        .unwrap_or_else(|| match fallback {
            ThemeFallback::Draft => default_theme_id(&pack).to_string(),
            ThemeFallback::PreferPrint => {
                if pack.manifest.themes.contains_key(THEME_PRINT) {
                    THEME_PRINT.to_string()
                } else {
                    default_theme_id(&pack).to_string()
                }
            }
        });
    let _ = pack.theme_css(&theme_id)?;
    Ok(ResolvedPackTheme { pack, theme_id })
}

fn validate_pack_relative(rel: &str, field: &str) -> Result<()> {
    let path = Path::new(rel);
    if path.is_absolute()
        || rel.is_empty()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(TesError::InvalidTemplate {
            message: format!("{field} path must be a relative path without '..': {rel}"),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn loads_minimal_pack_from_repo() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates/minimal");
        let pack = TemplatePack::load(&root).unwrap();
        assert_eq!(pack.manifest.id, "minimal");
        assert!(pack.manifest.themes.contains_key("draft"));
        assert!(pack.manifest.themes.contains_key("print"));
        let draft = pack.theme_css("draft").unwrap();
        assert!(draft.contains("--tes-bg"));
        let print = pack.theme_css("print").unwrap();
        assert!(print.contains("@page"));
    }

    #[test]
    fn rejects_parent_dir_theme_path() {
        let dir = tempfile::tempdir().unwrap();
        let mut file = fs::File::create(dir.path().join(MANIFEST_NAME)).unwrap();
        write!(
            file,
            r#"{{"id":"bad","version":"0.0.1","themes":{{"draft":"../evil.css"}}}}"#
        )
        .unwrap();
        let err = TemplatePack::load(dir.path()).unwrap_err();
        assert!(matches!(err, TesError::InvalidTemplate { .. }));
    }
}
