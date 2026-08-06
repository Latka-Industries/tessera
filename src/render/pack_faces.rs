//! Pack `faces.toml` → pinned TTF bytes for native emit (D23 / THI-356).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::pack_text::{load_overlay_toml, validate_ident};
use super::template::{DEFAULT_FACES_NAME, TemplatePack, with_resolved_pack};
use crate::error::{Result, TesError};

/// Face id → TrueType/OpenType bytes loaded from a pack.
pub type PackFaces = BTreeMap<String, Vec<u8>>;

/// Resolve pack faces: missing pack/overlay → empty map; bad TOML / missing TTF → error.
///
/// # Errors
///
/// Returns [`TesError::InvalidTemplate`] when `faces.toml` is present but invalid,
/// or a referenced font file is missing.
pub fn resolve_pack_faces(
    template_root: impl AsRef<Path>,
    template_id: Option<&str>,
    catalog_template_id: Option<&str>,
) -> Result<PackFaces> {
    with_resolved_pack(
        template_root,
        template_id,
        catalog_template_id,
        PackFaces::new(),
        pack_faces,
    )
}

/// Load `[faces]` overlay for a pack (id → relative `.ttf` / `.otf`).
///
/// # Errors
///
/// Returns [`TesError::InvalidTemplate`] / [`TesError::Io`] for bad overlays.
pub fn pack_faces(pack: &TemplatePack) -> Result<PackFaces> {
    let Some(path) = pack.faces_path() else {
        return Ok(PackFaces::new());
    };
    let file: FacesFile = load_overlay_toml(pack, &path, DEFAULT_FACES_NAME)?;
    let mut out = PackFaces::new();
    for (id, rel) in file.faces {
        validate_ident(&id, "face")?;
        validate_font_rel(&rel)?;
        let font_path = pack.root.join(&rel);
        let bytes = fs::read(&font_path).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                TesError::InvalidTemplate {
                    message: format!(
                        "pack '{}' face '{id}' font missing: {}",
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
                    "pack '{}' face '{id}' is not a TrueType/OpenType font: {}",
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
struct FacesFile {
    #[serde(default)]
    faces: BTreeMap<String, String>,
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
            message: format!("face font path must be relative without '..': {rel}"),
        });
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(ext.as_str(), "ttf" | "otf") {
        return Err(TesError::InvalidTemplate {
            message: format!("face font must be .ttf or .otf (got {rel})"),
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
    fn loads_minimal_test_face() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates/minimal");
        let pack = TemplatePack::load(&root).unwrap();
        let faces = pack_faces(&pack).unwrap();
        assert!(faces.contains_key("test"), "{faces:?}");
        assert!(looks_like_sfnt(&faces["test"]));
    }

    #[test]
    fn missing_pack_empty() {
        let faces = resolve_pack_faces("/no/such/templates", None, None).unwrap();
        assert!(faces.is_empty());
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
            dir.path().join("faces.toml"),
            "[faces]\ntest = \"../evil.ttf\"\n",
        )
        .unwrap();
        let pack = TemplatePack::load(dir.path()).unwrap();
        let err = pack_faces(&pack).unwrap_err();
        assert!(matches!(err, TesError::InvalidTemplate { .. }));
    }
}
