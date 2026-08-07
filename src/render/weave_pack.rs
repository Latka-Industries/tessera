//! Pack `weave.toml` → [`ariadnes_weave::LayoutKnobs`] (D23 / THI-357).
//!
//! CSS themes stay Chromium-only. Native emit merges a sparse pack overlay onto
//! [`LayoutKnobs::bundled`].

use std::fs;
use std::path::Path;

use ariadnes_weave::LayoutKnobs;
use toml::Value;

use super::template::{DEFAULT_WEAVE_NAME, TemplatePack, resolve_template_id};
use crate::error::{Result, TesError};

/// Prose section tables that may sit at the pack `weave.toml` root (D23 aesthetics).
const PROSE_ROOT_SECTIONS: &[&str] = &[
    "paragraph",
    "heading",
    "quote",
    "code",
    "list",
    "figure",
    "wrap",
    "text",
    "cite",
];

/// Category keys on [`LayoutKnobs`].
const LAYOUT_CATEGORIES: &[&str] = &["prose", "table", "deck", "math", "page"];

/// Resolve pack layout for native emit: bundled knobs, optionally overlaid by pack
/// `weave.toml` (manifest `weave` path or convention [`DEFAULT_WEAVE_NAME`]).
///
/// Missing template pack/root yields bundled defaults (pre-D23 behavior). A present
/// pack with a bad `weave.toml` still errors.
///
/// # Errors
///
/// Returns [`TesError::InvalidTemplate`] if the overlay cannot be read or merged.
pub fn resolve_pack_layout(
    template_root: impl AsRef<Path>,
    template_id: Option<&str>,
    catalog_template_id: Option<&str>,
) -> Result<LayoutKnobs> {
    let id = resolve_template_id(template_id, catalog_template_id);
    match TemplatePack::resolve(template_root, id) {
        Ok(pack) => pack_layout_knobs(&pack),
        Err(TesError::TemplateNotFound { .. }) => Ok(LayoutKnobs::bundled()),
        Err(err) => Err(err),
    }
}

/// Layout knobs for a loaded pack (bundled baseline + optional overlay).
///
/// Sparse `weave.toml` and/or master `tessera.toml` `[weave]` (THI-367). Both is a
/// hard error.
///
/// # Errors
///
/// Returns [`TesError::InvalidTemplate`] / [`TesError::Io`] when an overlay is
/// present but invalid.
pub fn pack_layout_knobs(pack: &TemplatePack) -> Result<LayoutKnobs> {
    use crate::render::pack_master::{load_pack_master, resolve_weave_raw};

    let master = load_pack_master(pack)?;
    let Some((raw, source)) = resolve_weave_raw(pack, master.as_ref())? else {
        return Ok(LayoutKnobs::bundled());
    };
    merge_weave_toml(&raw).map_err(|message| TesError::InvalidTemplate {
        message: format!(
            "pack '{}' weave overlay ({source}): {message}",
            pack.manifest.id
        ),
    })
}

/// Merge sparse pack TOML onto [`LayoutKnobs::bundled`].
///
/// Accepts category-rooted tables (`[prose.quote]`, `[page.footer]`, …) and
/// convenience prose sections at the root (`[quote]`, `[text]`, `[cite]`, …).
///
/// # Errors
///
/// Returns a human-readable parse/merge error string.
pub fn merge_weave_toml(raw: &str) -> std::result::Result<LayoutKnobs, String> {
    let overlay: Value = toml::from_str(raw).map_err(|e| format!("TOML parse error: {e}"))?;
    let overlay = normalize_pack_overlay(overlay)?;
    let mut base = Value::try_from(LayoutKnobs::bundled())
        .map_err(|e| format!("serialize bundled knobs: {e}"))?;
    merge_toml_value(&mut base, &overlay);
    base.try_into()
        .map_err(|e| format!("merged knobs invalid: {e}"))
}

fn normalize_pack_overlay(overlay: Value) -> std::result::Result<Value, String> {
    let Value::Table(mut root) = overlay else {
        return Err("weave.toml root must be a table".into());
    };

    let mut prose_convenience = toml::map::Map::new();
    let mut unknown = Vec::new();
    for key in root.keys().cloned().collect::<Vec<_>>() {
        if LAYOUT_CATEGORIES.contains(&key.as_str()) {
            continue;
        }
        if PROSE_ROOT_SECTIONS.contains(&key.as_str()) {
            if let Some(val) = root.remove(&key) {
                prose_convenience.insert(key, val);
            }
        } else {
            unknown.push(key);
        }
    }
    if !unknown.is_empty() {
        unknown.sort();
        return Err(format!(
            "unknown top-level key(s): {} (use prose/table/deck/math/page, or prose sections {})",
            unknown.join(", "),
            PROSE_ROOT_SECTIONS.join("/")
        ));
    }
    if !prose_convenience.is_empty() {
        let prose_overlay = Value::Table(prose_convenience);
        match root.get_mut("prose") {
            Some(prose @ Value::Table(_)) => merge_toml_value(prose, &prose_overlay),
            Some(_) => return Err("[prose] must be a table".into()),
            None => {
                root.insert("prose".into(), prose_overlay);
            }
        }
    }
    Ok(Value::Table(root))
}

fn merge_toml_value(base: &mut Value, overlay: &Value) {
    match (base, overlay) {
        (Value::Table(base_table), Value::Table(over_table)) => {
            for (key, value) in over_table {
                match base_table.get_mut(key) {
                    Some(existing) => merge_toml_value(existing, value),
                    None => {
                        base_table.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base, overlay) => *base = overlay.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    use ariadnes_weave::EmitOptions;

    #[test]
    fn sparse_quote_root_merges_onto_bundled() {
        let knobs = merge_weave_toml(
            r##"
[quote]
indent = 40.0
italic = false
color = "#445566"

[text]
color = "#112233"

[cite]
underline = true
"##,
        )
        .unwrap();
        assert!((knobs.prose.quote.indent - 40.0).abs() < f32::EPSILON);
        assert!(!knobs.prose.quote.italic);
        assert_eq!(knobs.prose.quote.color.unwrap().to_hex_string(), "#445566");
        assert_eq!(knobs.prose.text.color.unwrap().to_hex_string(), "#112233");
        assert!(knobs.prose.cite.underline);
        // Untouched keys keep bundled defaults.
        assert!((knobs.prose.paragraph.gap_after - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn category_font_pins_merge_onto_bundled() {
        let knobs = merge_weave_toml(
            r#"
[heading]
font = "display"

[text]
font = "body"

[quote]
font = "armenian"

[cite]
font = "body"
"#,
        )
        .unwrap();
        assert_eq!(knobs.prose.heading.font.as_deref(), Some("display"));
        assert_eq!(knobs.prose.text.font.as_deref(), Some("body"));
        assert_eq!(knobs.prose.quote.font.as_deref(), Some("armenian"));
        assert_eq!(knobs.prose.cite.font.as_deref(), Some("body"));
        // Spacing defaults untouched.
        assert!((knobs.prose.heading.gap_after - 8.0).abs() < f32::EPSILON);
    }

    #[test]
    fn category_rooted_overlay() {
        let knobs = merge_weave_toml(
            r#"
[prose.quote]
indent = 22.0

[page.footer]
font_size = 11.0
"#,
        )
        .unwrap();
        assert!((knobs.prose.quote.indent - 22.0).abs() < f32::EPSILON);
        assert!((knobs.page.footer.font_size - 11.0).abs() < f32::EPSILON);
    }

    #[test]
    fn rejects_unknown_top_level_key() {
        let err = merge_weave_toml("[fonts]\nfoo = 1\n").unwrap_err();
        assert!(err.contains("unknown top-level key"));
    }

    #[test]
    fn minimal_pack_weave_changes_emit_options() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates/minimal");
        let pack = TemplatePack::load(&root).unwrap();
        let layout = pack_layout_knobs(&pack).unwrap();
        assert!((layout.prose.quote.indent - 28.0).abs() < f32::EPSILON);
        assert_eq!(layout.prose.heading.font.as_deref(), Some("test"));
        let opts = EmitOptions::bundled_only().with_layout(layout);
        assert!((opts.layout.prose.quote.indent - 28.0).abs() < f32::EPSILON);
        assert_eq!(opts.layout.prose.heading.font.as_deref(), Some("test"));
    }

    #[test]
    fn master_pack_fixture_weave_matches_minimal() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates/master_pack");
        let pack = TemplatePack::load(&root).unwrap();
        let layout = pack_layout_knobs(&pack).unwrap();
        assert!((layout.prose.quote.indent - 28.0).abs() < f32::EPSILON);
        assert_eq!(layout.prose.heading.font.as_deref(), Some("test"));
    }

    #[test]
    fn missing_pack_keeps_bundled() {
        let knobs = resolve_pack_layout("/no/such/templates", None, None).unwrap();
        assert_eq!(knobs, LayoutKnobs::bundled());
    }

    #[test]
    fn missing_convention_file_keeps_bundled() {
        let dir = tempfile::tempdir().unwrap();
        let mut file = fs::File::create(dir.path().join("manifest.json")).unwrap();
        write!(
            file,
            r#"{{"id":"bare","version":"0.0.1","themes":{{"draft":"draft.css"}}}}"#
        )
        .unwrap();
        fs::write(dir.path().join("draft.css"), "body{}").unwrap();
        let pack = TemplatePack::load(dir.path()).unwrap();
        assert_eq!(pack_layout_knobs(&pack).unwrap(), LayoutKnobs::bundled());
    }
}
