//! External template / theme packs under [`crate::render`] (`docs/structure_v1.md`).
//!
//! Pack bytes stay outside `.tes`. The document catalog may reference a pack
//! by `template_id` / `theme_id`; this module loads the folder + manifest.
//!
//! Optional D23 overlays (convention filenames or manifest paths): `weave.toml`,
//! `typography.toml`, `aliases.toml`, `phrases.toml`, `fonts.toml`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, TesError};

/// Filename of a pack manifest.
pub const MANIFEST_NAME: &str = "manifest.json";

/// Convention filename for native layout knob overlay (D23).
pub const DEFAULT_WEAVE_NAME: &str = "weave.toml";

/// Convention filename for typography substitutions (D23 / THI-354).
pub const DEFAULT_TYPOGRAPHY_NAME: &str = "typography.toml";

/// Convention filename for fixed-string aliases (D23 / THI-354).
pub const DEFAULT_ALIASES_NAME: &str = "aliases.toml";

/// Convention filename for parameterized phrase templates (D23 / THI-355).
pub const DEFAULT_PHRASES_NAME: &str = "phrases.toml";

/// Convention filename for pack-pinned font TTFs (D23 / THI-356).
pub const DEFAULT_FONTS_NAME: &str = "fonts.toml";

/// Built-in pack id shipped under `templates/minimal`.
pub const DEFAULT_TEMPLATE_ID: &str = "minimal";

/// Screen-oriented theme id.
pub const THEME_DRAFT: &str = "draft";

/// Print-oriented theme id (shared preview/PDF path).
pub const THEME_PRINT: &str = "print";

/// Manuscript / beta-reader print theme id (fiction; distinct from academic print).
pub const THEME_MANUSCRIPT: &str = "manuscript";

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
    /// Optional path to ariadnes-weave layout overlay relative to the pack root.
    ///
    /// When omitted, native emit uses [`DEFAULT_WEAVE_NAME`] if that file exists.
    #[serde(default)]
    pub weave: Option<String>,
    /// Optional path to typography substitutions relative to the pack root.
    ///
    /// When omitted, format/compile uses [`DEFAULT_TYPOGRAPHY_NAME`] if present.
    #[serde(default)]
    pub typography: Option<String>,
    /// Optional path to fixed-string aliases relative to the pack root.
    ///
    /// When omitted, format/compile uses [`DEFAULT_ALIASES_NAME`] if present.
    #[serde(default)]
    pub aliases: Option<String>,
    /// Optional path to parameterized phrase templates relative to the pack root.
    ///
    /// When omitted, format/compile uses [`DEFAULT_PHRASES_NAME`] if present.
    #[serde(default)]
    pub phrases: Option<String>,
    /// Optional path to pack-pinned font map relative to the pack root.
    ///
    /// When omitted, native emit uses [`DEFAULT_FONTS_NAME`] if present.
    #[serde(default)]
    pub fonts: Option<String>,
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
            require_pack_relative_file(&root, rel, &format!("theme '{theme_id}' CSS"))?;
        }
        if let Some(starter) = &manifest.starter {
            validate_pack_relative(starter, "starter")?;
        }
        if let Some(weave) = &manifest.weave {
            require_pack_relative_file(&root, weave, "weave overlay")?;
        }
        if let Some(typography) = &manifest.typography {
            require_pack_relative_file(&root, typography, "typography overlay")?;
        }
        if let Some(aliases) = &manifest.aliases {
            require_pack_relative_file(&root, aliases, "aliases overlay")?;
        }
        if let Some(phrases) = &manifest.phrases {
            require_pack_relative_file(&root, phrases, "phrases overlay")?;
        }
        if let Some(fonts) = &manifest.fonts {
            require_pack_relative_file(&root, fonts, "fonts overlay")?;
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

    /// Absolute path to the pack's native weave overlay, if present.
    ///
    /// Prefer manifest `weave` when set; otherwise use [`DEFAULT_WEAVE_NAME`] when
    /// that file exists under the pack root.
    #[must_use]
    pub fn weave_path(&self) -> Option<PathBuf> {
        self.optional_overlay_path(self.manifest.weave.as_deref(), DEFAULT_WEAVE_NAME)
    }

    /// Absolute path to typography substitutions, if present.
    #[must_use]
    pub fn typography_path(&self) -> Option<PathBuf> {
        self.optional_overlay_path(self.manifest.typography.as_deref(), DEFAULT_TYPOGRAPHY_NAME)
    }

    /// Absolute path to fixed-string aliases, if present.
    #[must_use]
    pub fn aliases_path(&self) -> Option<PathBuf> {
        self.optional_overlay_path(self.manifest.aliases.as_deref(), DEFAULT_ALIASES_NAME)
    }

    /// Absolute path to parameterized phrase templates, if present.
    #[must_use]
    pub fn phrases_path(&self) -> Option<PathBuf> {
        self.optional_overlay_path(self.manifest.phrases.as_deref(), DEFAULT_PHRASES_NAME)
    }

    /// Absolute path to pack-pinned fonts map, if present.
    #[must_use]
    pub fn fonts_path(&self) -> Option<PathBuf> {
        self.optional_overlay_path(self.manifest.fonts.as_deref(), DEFAULT_FONTS_NAME)
    }

    fn optional_overlay_path(
        &self,
        manifest_rel: Option<&str>,
        convention: &str,
    ) -> Option<PathBuf> {
        if let Some(rel) = manifest_rel {
            return Some(self.root.join(rel));
        }
        let path = self.root.join(convention);
        path.is_file().then_some(path)
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
    /// Prefer [`THEME_MANUSCRIPT`] when present, else [`THEME_PRINT`] / [`default_theme_id`].
    PreferManuscript,
}

/// Pack + theme selected for preview or print HTML.
#[derive(Debug, Clone)]
pub struct ResolvedPackTheme {
    /// Loaded template pack.
    pub pack: TemplatePack,
    /// Selected theme id (`draft`, `print`, …).
    pub theme_id: String,
}

/// Effective pack id: CLI override → catalog → [`DEFAULT_TEMPLATE_ID`].
#[must_use]
pub fn resolve_template_id<'a>(
    template_id: Option<&'a str>,
    catalog_template_id: Option<&'a str>,
) -> &'a str {
    template_id
        .or(catalog_template_id)
        .unwrap_or(DEFAULT_TEMPLATE_ID)
}

/// Resolve a pack and run `load`, or return `empty` when the pack is missing.
///
/// # Errors
///
/// Propagates resolve/load errors other than [`TesError::TemplateNotFound`].
pub(crate) fn with_resolved_pack<T, F>(
    template_root: impl AsRef<Path>,
    template_id: Option<&str>,
    catalog_template_id: Option<&str>,
    empty: T,
    load: F,
) -> Result<T>
where
    F: FnOnce(&TemplatePack) -> Result<T>,
{
    let id = resolve_template_id(template_id, catalog_template_id);
    match TemplatePack::resolve(template_root, id) {
        Ok(pack) => load(&pack),
        Err(TesError::TemplateNotFound { .. }) => Ok(empty),
        Err(err) => Err(err),
    }
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
    let template_id = resolve_template_id(template_id, catalog_template_id);
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
            ThemeFallback::PreferManuscript => {
                if pack.manifest.themes.contains_key(THEME_MANUSCRIPT) {
                    THEME_MANUSCRIPT.to_string()
                } else if pack.manifest.themes.contains_key(THEME_PRINT) {
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

fn require_pack_relative_file(root: &Path, rel: &str, field: &str) -> Result<()> {
    validate_pack_relative(rel, field)?;
    let path = root.join(rel);
    if !path.is_file() {
        return Err(TesError::InvalidTemplate {
            message: format!("{field} missing: {}", path.display()),
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
        let weave = pack.weave_path().expect("minimal ships weave.toml");
        assert!(weave.ends_with(DEFAULT_WEAVE_NAME));
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
