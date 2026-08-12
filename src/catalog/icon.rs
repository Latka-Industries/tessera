//! Named Font Awesome icons for Tessprek `\icon{name}` (pack faces `fab` / `fas`).
//!
//! Sealed as ordinary [`InlineKind::Font`](crate::catalog::InlineKind::Font) + glyph
//! text — same wire as `\font{fab}{…}`. Encode prefers `\icon{name}` when the
//! face+glyph match a catalog entry.

/// One named icon: Tessprek id → pack face + FA PUA glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IconDef {
    /// Tessprek `\icon{…}` name (`github`, `python`, …).
    pub name: &'static str,
    /// Pack font id (`fab` brands, `fas` solid).
    pub face: &'static str,
    /// Font Awesome codepoint (Private Use Area).
    pub glyph: char,
}

/// CV / dogfood starter set (Font Awesome 6 Free brands + solid).
pub const ICONS: &[IconDef] = &[
    IconDef {
        name: "github",
        face: "fab",
        glyph: '\u{f09b}',
    },
    IconDef {
        name: "linkedin",
        face: "fab",
        glyph: '\u{f08c}',
    },
    IconDef {
        name: "orcid",
        face: "fab",
        glyph: '\u{f8d2}',
    },
    IconDef {
        name: "python",
        face: "fab",
        glyph: '\u{f3e2}',
    },
    IconDef {
        name: "js",
        face: "fab",
        glyph: '\u{f3b8}',
    },
    IconDef {
        name: "globe",
        face: "fas",
        glyph: '\u{f0ac}',
    },
];

/// Look up a Tessprek icon name.
#[must_use]
pub fn icon_by_name(name: &str) -> Option<&'static IconDef> {
    ICONS.iter().find(|i| i.name == name)
}

/// Reverse-map a sealed font face + single glyph to an icon name.
#[must_use]
pub fn icon_name_for_face_glyph(face: &str, text: &str) -> Option<&'static str> {
    let mut chars = text.chars();
    let glyph = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    ICONS
        .iter()
        .find(|i| i.face == face && i.glyph == glyph)
        .map(|i| i.name)
}

/// Sorted icon names (LSP / docs).
#[must_use]
pub fn icon_names() -> Vec<&'static str> {
    ICONS.iter().map(|i| i.name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_round_maps() {
        let ic = icon_by_name("github").unwrap();
        assert_eq!(ic.face, "fab");
        assert_eq!(
            icon_name_for_face_glyph("fab", &ic.glyph.to_string()),
            Some("github")
        );
    }
}
