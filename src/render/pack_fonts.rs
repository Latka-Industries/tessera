//! Pack `fonts.toml` → pinned TTF bytes for native emit (D23 / THI-356).
//!
//! Ids here must match Tessprek `\font{id}{…}` / sealed [`InlineKind::Font`](crate::catalog::InlineKind::Font).
//! Loaded into weave `EmitOptions::pinned_faces` at native PDF emit (weave still
//! names the pin API `face`).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::pack_text::{load_overlay_toml, validate_ident};
use super::template::{DEFAULT_FONTS_NAME, TemplatePack, with_resolved_pack};
use crate::error::{Result, TesError};

/// Font id → TrueType/OpenType bytes loaded from a pack.
pub type PackFonts = BTreeMap<String, Vec<u8>>;

/// Resolve pack fonts: missing pack/overlay → empty map; bad TOML / missing TTF → error.
///
/// # Errors
///
/// Returns [`TesError::InvalidTemplate`] when `fonts.toml` is present but invalid,
/// or a referenced font file is missing.
pub fn resolve_pack_fonts(
    template_root: impl AsRef<Path>,
    template_id: Option<&str>,
    catalog_template_id: Option<&str>,
) -> Result<PackFonts> {
    with_resolved_pack(
        template_root,
        template_id,
        catalog_template_id,
        PackFonts::new(),
        pack_fonts,
    )
}

/// Load `[fonts]` overlay for a pack (id → relative `.ttf` / `.otf`).
///
/// # Errors
///
/// Returns [`TesError::InvalidTemplate`] / [`TesError::Io`] for bad overlays.
pub fn pack_fonts(pack: &TemplatePack) -> Result<PackFonts> {
    let Some(path) = pack.fonts_path() else {
        return Ok(PackFonts::new());
    };
    let file: FontsFile = load_overlay_toml(pack, &path, DEFAULT_FONTS_NAME)?;
    let mut out = PackFonts::new();
    for (id, rel) in file.fonts {
        validate_ident(&id, "font")?;
        validate_font_rel(&rel)?;
        let font_path = pack.root.join(&rel);
        let bytes = fs::read(&font_path).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                TesError::InvalidTemplate {
                    message: format!(
                        "pack '{}' font '{id}' file missing: {}",
                        pack.manifest.id,
                        font_path.display()
                    ),
                }
            } else {
                TesError::Io(err)
            }
        })?;
        if !looks_like_sfnt(&bytes) {
            return Err(TesError::InvalidTemplate {
                message: format!(
                    "pack '{}' font '{id}' is not a TrueType/OpenType font: {}",
                    pack.manifest.id,
                    font_path.display()
                ),
            });
        }
        out.insert(id, bytes);
    }
    Ok(out)
}

#[derive(Debug, Default, Deserialize)]
struct FontsFile {
    #[serde(default)]
    fonts: BTreeMap<String, String>,
}

fn validate_font_rel(rel: &str) -> Result<()> {
    let path = Path::new(rel);
    if path.is_absolute()
        || rel.is_empty()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(TesError::InvalidTemplate {
            message: format!("font path must be relative without '..': {rel}"),
        });
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(ext.as_str(), "ttf" | "otf") {
        return Err(TesError::InvalidTemplate {
            message: format!("font file must be .ttf or .otf (got {rel})"),
        });
    }
    Ok(())
}

fn looks_like_sfnt(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\0\x01\0\0") // TrueType
        || bytes.starts_with(b"OTTO") // CFF OpenType
        || bytes.starts_with(b"true")
        || bytes.starts_with(b"typ1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    #[test]
    fn loads_minimal_test_font() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates/minimal");
        let pack = TemplatePack::load(&root).unwrap();
        let fonts = pack_fonts(&pack).unwrap();
        assert!(fonts.contains_key("test"), "{fonts:?}");
        assert!(looks_like_sfnt(&fonts["test"]));
    }

    #[test]
    fn missing_pack_empty() {
        let fonts = resolve_pack_fonts("/no/such/templates", None, None).unwrap();
        assert!(fonts.is_empty());
    }

    #[test]
    fn rejects_parent_dir_font() {
        let dir = tempfile::tempdir().unwrap();
        let mut file = fs::File::create(dir.path().join("manifest.json")).unwrap();
        write!(
            file,
            r#"{{"id":"bad","version":"0.0.1","themes":{{"draft":"draft.css"}}}}"#
        )
        .unwrap();
        fs::write(dir.path().join("draft.css"), "body{}").unwrap();
        fs::write(
            dir.path().join("fonts.toml"),
            "[fonts]\ntest = \"../evil.ttf\"\n",
        )
        .unwrap();
        let pack = TemplatePack::load(dir.path()).unwrap();
        let err = pack_fonts(&pack).unwrap_err();
        assert!(matches!(err, TesError::InvalidTemplate { .. }));
    }
}
