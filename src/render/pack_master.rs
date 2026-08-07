//! Optional master `tessera.toml` (THI-367): one file with sections that map 1:1
//! onto sparse D23 overlays.
//!
//! Sparse siblings (`typography.toml`, …) still work. A master **section** and a
//! sibling file for the same concern is a hard error.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use toml::Value;

use super::template::{DEFAULT_PACK_NAME, TemplatePack};
use crate::error::{Result, TesError};

/// Concern name used in conflict / parse errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PackConcern {
    Typography,
    Aliases,
    Phrases,
    Fonts,
    Weave,
}

impl PackConcern {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Typography => "typography",
            Self::Aliases => "aliases",
            Self::Phrases => "phrases",
            Self::Fonts => "fonts",
            Self::Weave => "weave",
        }
    }

    const fn sparse_convention(self) -> &'static str {
        match self {
            Self::Typography => super::template::DEFAULT_TYPOGRAPHY_NAME,
            Self::Aliases => super::template::DEFAULT_ALIASES_NAME,
            Self::Phrases => super::template::DEFAULT_PHRASES_NAME,
            Self::Fonts => super::template::DEFAULT_FONTS_NAME,
            Self::Weave => super::template::DEFAULT_WEAVE_NAME,
        }
    }
}

/// Parsed master pack tables (absent section = `None`).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct PackMaster {
    /// Substitutions (same entries as sparse `typography.toml` `[substitutions]`).
    typography: Option<BTreeMap<String, String>>,
    aliases: Option<BTreeMap<String, String>>,
    phrases: Option<BTreeMap<String, String>>,
    fonts: Option<BTreeMap<String, String>>,
    /// Same shape as sparse `weave.toml` root (nested under `[weave]`).
    weave: Option<Value>,
}

impl PackMaster {
    fn has(&self, concern: PackConcern) -> bool {
        match concern {
            PackConcern::Typography => self.typography.is_some(),
            PackConcern::Aliases => self.aliases.is_some(),
            PackConcern::Phrases => self.phrases.is_some(),
            PackConcern::Fonts => self.fonts.is_some(),
            PackConcern::Weave => self.weave.is_some(),
        }
    }

    fn string_map(&self, concern: PackConcern) -> Option<&BTreeMap<String, String>> {
        match concern {
            PackConcern::Typography => self.typography.as_ref(),
            PackConcern::Aliases => self.aliases.as_ref(),
            PackConcern::Phrases => self.phrases.as_ref(),
            PackConcern::Fonts => self.fonts.as_ref(),
            PackConcern::Weave => None,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct MasterPackFile {
    #[serde(default)]
    typography: Option<BTreeMap<String, String>>,
    #[serde(default)]
    aliases: Option<BTreeMap<String, String>>,
    #[serde(default)]
    phrases: Option<BTreeMap<String, String>>,
    #[serde(default)]
    fonts: Option<BTreeMap<String, String>>,
    #[serde(default)]
    weave: Option<Value>,
}

/// Load optional `tessera.toml` (manifest `pack` or [`DEFAULT_PACK_NAME`]).
///
/// # Errors
///
/// Returns [`TesError::InvalidTemplate`] when the file is present but invalid.
pub(crate) fn load_pack_master(pack: &TemplatePack) -> Result<Option<PackMaster>> {
    let Some(path) = pack.pack_path() else {
        return Ok(None);
    };
    let raw = read_pack_file(pack, &path, DEFAULT_PACK_NAME)?;
    let file: MasterPackFile = toml::from_str(&raw).map_err(|e| TesError::InvalidTemplate {
        message: format!(
            "pack '{}' {}: {e}",
            pack.manifest.id,
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(DEFAULT_PACK_NAME)
        ),
    })?;
    if let Some(weave) = &file.weave
        && !matches!(weave, Value::Table(_))
    {
        return Err(TesError::InvalidTemplate {
            message: format!(
                "pack '{}' {DEFAULT_PACK_NAME}: [weave] must be a table",
                pack.manifest.id
            ),
        });
    }
    Ok(Some(PackMaster {
        typography: file.typography,
        aliases: file.aliases,
        phrases: file.phrases,
        fonts: file.fonts,
        weave: file.weave,
    }))
}

/// Sparse file **or** master string-map section for a concern (not both).
///
/// # Errors
///
/// Conflict, I/O, or `load_sparse` failures.
pub(crate) fn resolve_map_concern<F>(
    pack: &TemplatePack,
    master: Option<&PackMaster>,
    concern: PackConcern,
    sparse_path: Option<PathBuf>,
    load_sparse: F,
) -> Result<Option<BTreeMap<String, String>>>
where
    F: FnOnce(&Path) -> Result<BTreeMap<String, String>>,
{
    reject_master_sparse_conflict(pack, master, concern, sparse_path.as_deref())?;
    if let Some(path) = sparse_path {
        return Ok(Some(load_sparse(&path)?));
    }
    Ok(master.and_then(|m| m.string_map(concern).cloned()))
}

/// Sparse `weave.toml` text **or** serialized master `[weave]` (not both).
///
/// The `&str` is a short source label for error messages (`weave.toml` /
/// `tessera.toml`).
///
/// # Errors
///
/// Conflict, I/O, or TOML serialize failures.
pub(crate) fn resolve_weave_raw(
    pack: &TemplatePack,
    master: Option<&PackMaster>,
) -> Result<Option<(String, &'static str)>> {
    let weave_path = pack.weave_path();
    reject_master_sparse_conflict(pack, master, PackConcern::Weave, weave_path.as_deref())?;
    if let Some(path) = weave_path {
        let raw = read_pack_file(pack, &path, PackConcern::Weave.sparse_convention())?;
        return Ok(Some((raw, PackConcern::Weave.sparse_convention())));
    }
    if let Some(weave) = master.and_then(|m| m.weave.as_ref()) {
        let raw = toml::to_string(weave).map_err(|e| TesError::InvalidTemplate {
            message: format!(
                "pack '{}' {DEFAULT_PACK_NAME} [weave]: serialize error: {e}",
                pack.manifest.id
            ),
        })?;
        return Ok(Some((raw, DEFAULT_PACK_NAME)));
    }
    Ok(None)
}

fn reject_master_sparse_conflict(
    pack: &TemplatePack,
    master: Option<&PackMaster>,
    concern: PackConcern,
    sparse: Option<&Path>,
) -> Result<()> {
    if master.is_some_and(|m| m.has(concern)) && sparse.is_some() {
        return Err(TesError::InvalidTemplate {
            message: format!(
                "pack '{}': {} is defined in both {DEFAULT_PACK_NAME} and {} \
                 (or a manifest path for that overlay); use one or the other",
                pack.manifest.id,
                concern.as_str(),
                concern.sparse_convention()
            ),
        });
    }
    Ok(())
}

fn read_pack_file(pack: &TemplatePack, path: &Path, fallback_name: &str) -> Result<String> {
    fs::read_to_string(path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            TesError::InvalidTemplate {
                message: format!(
                    "pack '{}' {fallback_name} missing: {}",
                    pack.manifest.id,
                    path.display()
                ),
            }
        } else {
            TesError::Io(err)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    use crate::render::template::MANIFEST_NAME;

    fn write_pack(dir: &Path, pack_toml: &str) {
        let mut file = fs::File::create(dir.join(MANIFEST_NAME)).unwrap();
        write!(
            file,
            r#"{{"id":"t","version":"0.0.1","themes":{{"draft":"draft.css"}}}}"#
        )
        .unwrap();
        fs::write(dir.join("draft.css"), "body{}").unwrap();
        fs::write(dir.join(DEFAULT_PACK_NAME), pack_toml).unwrap();
    }

    #[test]
    fn loads_master_sections() {
        let dir = tempfile::tempdir().unwrap();
        write_pack(
            dir.path(),
            r#"
[typography]
"..." = "…"

[aliases]
maryamlatin = "Maryam"

[phrases]
yegourdoon = "*{arg}*"

[fonts]
test = "fonts/test-face.ttf"

[weave.quote]
indent = 28.0
"#,
        );
        let pack = TemplatePack::load(dir.path()).unwrap();
        let master = load_pack_master(&pack).unwrap().expect("master");
        assert_eq!(master.typography.as_ref().unwrap()["..."], "…");
        assert_eq!(master.aliases.as_ref().unwrap()["maryamlatin"], "Maryam");
        assert!(master.weave.is_some());
    }

    #[test]
    fn master_pack_fixture_loads() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates/master_pack");
        let pack = TemplatePack::load(&root).unwrap();
        assert_eq!(pack.manifest.id, "master_pack");
        let master = load_pack_master(&pack).unwrap().expect("tessera.toml");
        assert!(master.typography.is_some());
        assert!(master.phrases.is_some());
        assert!(master.fonts.is_some());
        assert!(master.weave.is_some());
        assert!(pack.typography_path().is_none());
        assert!(pack.weave_path().is_none());
    }
}
