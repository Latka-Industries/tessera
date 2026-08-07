//! Pack-derived completion catalogs for Tessprek (THI-369).
//!
//! Loads font ids / phrase keys / alias names from the document's template pack
//! (sparse overlays or master `tessera.toml`).

use std::env;
use std::path::PathBuf;

use crate::edit::tessprek::{TessprekDocMeta, parse_attrs, take_leading_tessera_header};
use crate::render::pack_fonts::pack_font_paths;
use crate::render::pack_text::pack_text_rules;
use crate::render::template::{TemplatePack, resolve_template_id};

/// Ids / keys offered inside `\font{…}` / `\phrase{…}` / `\alias`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct PackCompletionCatalog {
    pub font_ids: Vec<String>,
    pub phrase_keys: Vec<String>,
    pub alias_names: Vec<String>,
}

/// Resolve pack catalogs for a Tessprek buffer (or under an explicit root).
fn catalog_at(text: &str, template_root: &std::path::Path) -> PackCompletionCatalog {
    let template_id = template_id_from_tessprek(text);
    let id = resolve_template_id(template_id.as_deref(), None);
    match TemplatePack::resolve(template_root, id) {
        Ok(pack) => catalog_for_pack(&pack).unwrap_or_default(),
        Err(_) => PackCompletionCatalog::default(),
    }
}

/// Resolve pack catalogs for a Tessprek buffer.
///
/// Missing pack / bad overlays → empty catalog (generic snippets still work).
#[must_use]
pub(super) fn catalog_for_tessprek(text: &str) -> PackCompletionCatalog {
    catalog_at(text, &template_root())
}

/// Build a catalog from an already-loaded pack.
pub(super) fn catalog_for_pack(pack: &TemplatePack) -> crate::error::Result<PackCompletionCatalog> {
    let font_ids = pack_font_paths(pack)?.into_keys().collect::<Vec<_>>();
    let rules = pack_text_rules(pack)?;
    let phrase_keys = rules.phrases.into_iter().map(|(k, _)| k).collect();
    let alias_names = rules.aliases.into_iter().map(|(k, _)| k).collect();
    Ok(PackCompletionCatalog {
        font_ids,
        phrase_keys,
        alias_names,
    })
}

fn template_root() -> PathBuf {
    env::var_os("TES_TEMPLATE_ROOT").map_or_else(|| PathBuf::from("templates"), PathBuf::from)
}

fn template_id_from_tessprek(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let (attrs, _, _) = take_leading_tessera_header(&lines).ok()?;
    let map = parse_attrs(&attrs, 1).ok()?;
    TessprekDocMeta::from_attrs(&map).template_id
}

/// Test helper: catalog under an explicit templates root.
#[cfg(test)]
pub(super) fn catalog_for_tessprek_at(
    text: &str,
    template_root: &std::path::Path,
) -> PackCompletionCatalog {
    catalog_at(text, template_root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::path::Path;

    fn write_bare_pack(templates_root: &Path, id: &str) -> PathBuf {
        let pack_root = templates_root.join(id);
        fs::create_dir_all(pack_root.join("themes")).unwrap();
        let mut manifest = fs::File::create(pack_root.join("manifest.json")).unwrap();
        write!(
            manifest,
            r#"{{"id":"{id}","version":"0.0.1","themes":{{"draft":"themes/draft.css"}}}}"#
        )
        .unwrap();
        fs::write(pack_root.join("themes/draft.css"), "body{}").unwrap();
        pack_root
    }

    fn header(template_id: &str) -> String {
        format!(
            "\\tessera{{format=tessprek version=2 template_id={template_id}}}\n\\ids{{1}}\n\nHi.\n"
        )
    }

    #[test]
    fn minimal_pack_lists_font_and_phrase_keys() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates");
        let cat = catalog_for_tessprek_at(&header("minimal"), &root);
        assert!(cat.font_ids.iter().any(|id| id == "armenian"), "{cat:?}");
        assert!(cat.font_ids.iter().any(|id| id == "greek"), "{cat:?}");
        assert!(cat.font_ids.iter().any(|id| id == "cyrillic"), "{cat:?}");
        assert!(cat.phrase_keys.iter().any(|k| k == "yegourdoon"), "{cat:?}");
    }

    #[test]
    fn master_pack_fixture_lists_fonts() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates");
        let cat = catalog_for_tessprek_at(&header("master_pack"), &root);
        assert!(cat.font_ids.contains(&"armenian".into()), "{cat:?}");
        assert!(cat.font_ids.contains(&"greek".into()), "{cat:?}");
        assert!(cat.phrase_keys.contains(&"yegourdoon".into()), "{cat:?}");
    }

    #[test]
    fn tempfile_sparse_pack_fonts_and_phrases() {
        let dir = tempfile::tempdir().unwrap();
        let templates = dir.path().join("templates");
        let pack_root = write_bare_pack(&templates, "demo");
        fs::write(
            pack_root.join("fonts.toml"),
            "[fonts]\narmenian = \"fonts/a.ttf\"\ngreek = \"fonts/g.ttf\"\n",
        )
        .unwrap();
        fs::create_dir_all(pack_root.join("fonts")).unwrap();
        fs::write(
            pack_root.join("phrases.toml"),
            "[phrases]\ngreet = \"Hi {arg}\"\n",
        )
        .unwrap();
        fs::write(
            pack_root.join("aliases.toml"),
            "[aliases]\nshortname = \"Expanded\"\n",
        )
        .unwrap();

        let cat = catalog_for_tessprek_at(&header("demo"), &templates);
        assert_eq!(
            cat.font_ids,
            vec!["armenian".to_string(), "greek".to_string()]
        );
        assert_eq!(cat.phrase_keys, vec!["greet".to_string()]);
        assert_eq!(cat.alias_names, vec!["shortname".to_string()]);
    }

    #[test]
    fn tempfile_master_tessera_toml() {
        let dir = tempfile::tempdir().unwrap();
        let templates = dir.path().join("templates");
        let pack_root = write_bare_pack(&templates, "mastery");
        fs::write(
            pack_root.join("tessera.toml"),
            "[fonts]\ncyrillic = \"fonts/c.ttf\"\n\n[phrases]\nbye = \"Bye\"\n",
        )
        .unwrap();
        let cat = catalog_for_tessprek_at(&header("mastery"), &templates);
        assert_eq!(cat.font_ids, vec!["cyrillic".to_string()]);
        assert_eq!(cat.phrase_keys, vec!["bye".to_string()]);
    }
}
