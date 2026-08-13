//! Browse packs for native figure/weave smoke (`fixtures/packs/`).
//!
//! Not product templates. Regenerate with `cargo run --example gen_sample_fixtures`.

use std::fmt::Write;
use std::fs;
use std::path::Path;

use crate::error::Result;

/// One figure_* pack: only `align` / caption `text_align` differ.
struct FigurePack {
    id: &'static str,
    align: &'static str,
    caption_text_align: &'static str,
    comment: &'static str,
    /// Short README table cell (knobs to notice).
    knobs: &'static str,
}

const FIGURE_PACKS: &[FigurePack] = &[
    FigurePack {
        id: "figure_left",
        align: "left",
        caption_text_align: "follow",
        comment: "Left figure band — caption follows figure + wraps in the band.",
        knobs: "`[figure].align = left`, caption/title `follow`",
    },
    FigurePack {
        id: "figure_center",
        align: "center",
        caption_text_align: "follow",
        comment: "Center figure band — caption follows figure + wraps in the band.",
        knobs: "`align = center`, caption/title `follow`",
    },
    FigurePack {
        id: "figure_right",
        align: "right",
        caption_text_align: "follow",
        comment: "Right figure band — caption follows figure + wraps in the band.",
        knobs: "`align = right`, caption/title `follow`",
    },
    FigurePack {
        id: "figure_caption_justify",
        align: "center",
        caption_text_align: "justify",
        comment: "Same as figure_center (match_figure band + wrap), but caption text is justified.",
        knobs: "same as center, caption `text_align = justify`",
    },
];

/// One `page_chrome`_* pack (THI-392): header/footer align + format.
struct ChromePack {
    id: &'static str,
    comment: &'static str,
    knobs: &'static str,
    header_align: &'static str,
    header_format: &'static str,
    footer_align: &'static str,
    footer_format: &'static str,
}

const CHROME_PACKS: &[ChromePack] = &[
    ChromePack {
        id: "page_chrome",
        comment: "THI-392 reference — full header/footer knob dump (center footer, left title).",
        knobs: "all knobs; footer `{page} / {pages}` center; header `{title}` left",
        header_align: "left",
        header_format: "{title}",
        footer_align: "center",
        footer_format: "{page} / {pages}",
    },
    ChromePack {
        id: "page_chrome_footer_left",
        comment: "THI-392: footer left, header left, n/m.",
        knobs: "footer `align = left`",
        header_align: "left",
        header_format: "{title}",
        footer_align: "left",
        footer_format: "{page} / {pages}",
    },
    ChromePack {
        id: "page_chrome_footer_center",
        comment: "THI-392: footer center, header left, n/m.",
        knobs: "footer `align = center`",
        header_align: "left",
        header_format: "{title}",
        footer_align: "center",
        footer_format: "{page} / {pages}",
    },
    ChromePack {
        id: "page_chrome_footer_right",
        comment: "THI-392: footer right, header left, n/m.",
        knobs: "footer `align = right`",
        header_align: "left",
        header_format: "{title}",
        footer_align: "right",
        footer_format: "{page} / {pages}",
    },
    ChromePack {
        id: "page_chrome_fmt_slash",
        comment: "THI-392: format `{page} / {pages}`.",
        knobs: "footer `format = \"{page} / {pages}\"`",
        header_align: "left",
        header_format: "{title}",
        footer_align: "center",
        footer_format: "{page} / {pages}",
    },
    ChromePack {
        id: "page_chrome_fmt_of",
        comment: "THI-392: format `Page {page} of {pages}`.",
        knobs: "footer `format = \"Page {page} of {pages}\"`",
        header_align: "left",
        header_format: "{title}",
        footer_align: "center",
        footer_format: "Page {page} of {pages}",
    },
    ChromePack {
        id: "page_chrome_fmt_bare",
        comment: "THI-392: format `{page}` only.",
        knobs: "footer `format = \"{page}\"`",
        header_align: "left",
        header_format: "{title}",
        footer_align: "center",
        footer_format: "{page}",
    },
    ChromePack {
        id: "page_chrome_fmt_title_page",
        comment: "THI-392: format `{title} — {page}`.",
        knobs: "footer `format = \"{title} — {page}\"`",
        header_align: "left",
        header_format: "{title}",
        footer_align: "center",
        footer_format: "{title} — {page}",
    },
    ChromePack {
        id: "page_chrome_header_center",
        comment: "THI-392: header center `{title}`.",
        knobs: "header `align = center`",
        header_align: "center",
        header_format: "{title}",
        footer_align: "center",
        footer_format: "{page} / {pages}",
    },
    ChromePack {
        id: "page_chrome_header_right",
        comment: "THI-392: header right `{title}`.",
        knobs: "header `align = right`",
        header_align: "right",
        header_format: "{title}",
        footer_align: "center",
        footer_format: "{page} / {pages}",
    },
];

/// One hyphen_* pack (THI-394): wrap hyphenate + widow/orphan + narrow indent.
struct HyphenPack {
    id: &'static str,
    comment: &'static str,
    knobs: &'static str,
    hyphenate: bool,
    orphan_lines: u32,
    widow_lines: u32,
}

const HYPHEN_PACKS: &[HyphenPack] = &[
    HyphenPack {
        id: "hyphen_on",
        comment: "THI-394: hyphenate on, orphans/widows 2, narrow indent band.",
        knobs: "`hyphenate = true`, orphan/widow 2, `indent.step = 48`",
        hyphenate: true,
        orphan_lines: 2,
        widow_lines: 2,
    },
    HyphenPack {
        id: "hyphen_off",
        comment: "THI-394: hyphenate off (same narrow band) for eyeball compare.",
        knobs: "`hyphenate = false`, orphan/widow 2, `indent.step = 48`",
        hyphenate: false,
        orphan_lines: 2,
        widow_lines: 2,
    },
    HyphenPack {
        id: "hyphen_widows_3",
        comment: "THI-394: hyphenate on with widow_lines = 3.",
        knobs: "`hyphenate = true`, `widow_lines = 3`",
        hyphenate: true,
        orphan_lines: 2,
        widow_lines: 3,
    },
];

/// Column body packs (THI-391): page chrome + paragraph align for newspaper flow.
struct ColumnsPack {
    id: &'static str,
    comment: &'static str,
    knobs: &'static str,
    text_align: &'static str,
}

const COLUMNS_PACKS: &[ColumnsPack] = &[
    ColumnsPack {
        id: "columns_left",
        comment: "THI-391: 2/3-col article smoke — flush-left body (default).",
        knobs: "page chrome + `[paragraph] text_align = left`",
        text_align: "left",
    },
    ColumnsPack {
        id: "columns_justify",
        comment: "THI-391: same chrome, justified body in column bands.",
        knobs: "page chrome + `[paragraph] text_align = justify`",
        text_align: "justify",
    },
];

const STUB_CSS: &str = "/* stub — native PDF ignores pack CSS; required by TemplatePack::load */\nbody { margin: 0; }\n";

const CHROME_SMOKE_DOCS: &[&str] = &["manuscript_chapters", "field_notes", "studio_brief"];

/// Write every figure_* and `page_chrome`_* smoke pack under `dir`.
///
/// # Errors
///
/// Returns I/O errors from creating directories or writing files.
pub fn write_all(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    for pack in FIGURE_PACKS {
        write_figure_pack(dir, pack)?;
    }
    for pack in CHROME_PACKS {
        write_chrome_pack(dir, pack)?;
    }
    for pack in HYPHEN_PACKS {
        write_hyphen_pack(dir, pack)?;
    }
    for pack in COLUMNS_PACKS {
        write_columns_pack(dir, pack)?;
    }
    fs::write(dir.join("README.md"), packs_readme())?;
    fs::write(dir.join("page_chrome").join("README.md"), chrome_readme())?;
    fs::write(dir.join("hyphen_on").join("README.md"), hyphen_readme())?;
    fs::write(dir.join("columns_justify").join("README.md"), columns_readme())?;
    Ok(())
}

fn write_stub_shell(pack_dir: &Path, id: &str) -> Result<()> {
    let themes = pack_dir.join("themes");
    fs::create_dir_all(&themes)?;
    fs::write(
        pack_dir.join("manifest.json"),
        format!(
            "{{\n  \"id\": \"{}\",\n  \"version\": \"0.1.0\",\n  \"compatible_layout\": 0,\n  \"doc_kind_default\": \"note\",\n  \"themes\": {{\n    \"print\": \"themes/print.css\"\n  }},\n  \"export_targets\": [\"pdf\"]\n}}\n",
            id
        ),
    )?;
    fs::write(themes.join("print.css"), STUB_CSS)?;
    Ok(())
}

fn write_figure_pack(root: &Path, pack: &FigurePack) -> Result<()> {
    let pack_dir = root.join(pack.id);
    write_stub_shell(&pack_dir, pack.id)?;
    fs::write(
        pack_dir.join("weave.toml"),
        format!(
            "# {}\n\
[figure]\n\
align = \"{}\"\n\
max_width_factor = 0.55\n\
title_align = \"follow\"\n\
title_text_align = \"follow\"\n\
\n\
[caption]\n\
band = \"match_figure\"\n\
text_align = \"{}\"\n\
italic = true\n\
size_factor = 0.9\n",
            pack.comment, pack.align, pack.caption_text_align
        ),
    )?;
    Ok(())
}

fn write_chrome_pack(root: &Path, pack: &ChromePack) -> Result<()> {
    let pack_dir = root.join(pack.id);
    write_stub_shell(&pack_dir, pack.id)?;
    fs::write(
        pack_dir.join("weave.toml"),
        format!(
            "# {}\n\
# Tokens: {{page}}, {{pages}}, {{title}} ({{heading}} deferred)\n\
\n\
[page.header]\n\
enabled = true\n\
format = \"{}\"\n\
align = \"{}\"\n\
font_size = 9.0\n\
y_margin_factor = 0.55\n\
\n\
[page.footer]\n\
enabled = true\n\
format = \"{}\"\n\
align = \"{}\"\n\
font_size = 9.0\n\
y_margin_factor = 0.45\n\
\n\
[page.content]\n\
top_clearance = 18.0\n\
bottom_clearance = 18.0\n",
            pack.comment,
            pack.header_format,
            pack.header_align,
            pack.footer_format,
            pack.footer_align
        ),
    )?;
    Ok(())
}

fn write_hyphen_pack(root: &Path, pack: &HyphenPack) -> Result<()> {
    let pack_dir = root.join(pack.id);
    write_stub_shell(&pack_dir, pack.id)?;
    fs::write(
        pack_dir.join("weave.toml"),
        format!(
            "# {}\n\
\n\
[indent]\n\
step = 48.0\n\
\n\
[wrap]\n\
hyphenate = {}\n\
orphan_lines = {}\n\
widow_lines = {}\n\
\n\
[page.header]\n\
enabled = true\n\
format = \"{{title}}\"\n\
align = \"left\"\n\
\n\
[page.content]\n\
top_clearance = 18.0\n\
",
            pack.comment, pack.hyphenate, pack.orphan_lines, pack.widow_lines
        ),
    )?;
    Ok(())
}

fn write_columns_pack(root: &Path, pack: &ColumnsPack) -> Result<()> {
    let pack_dir = root.join(pack.id);
    write_stub_shell(&pack_dir, pack.id)?;
    fs::write(
        pack_dir.join("weave.toml"),
        format!(
            "# {}\n\
# Pair with fixtures/samples/article_columns.tes\n\
\n\
[paragraph]\n\
text_align = \"{}\"\n\
\n\
[body_columns]\n\
gap = 16.0\n\
\n\
{}\
",
            pack.comment,
            pack.text_align,
            page_chrome_bands_toml()
        ),
    )?;
    Ok(())
}

/// Shared header/footer band knobs for column smoke packs (same as page_chrome defaults).
fn page_chrome_bands_toml() -> &'static str {
    "[page.header]\n\
enabled = true\n\
format = \"{title}\"\n\
align = \"left\"\n\
font_size = 9.0\n\
y_margin_factor = 0.55\n\
\n\
[page.footer]\n\
enabled = true\n\
format = \"{page} / {pages}\"\n\
align = \"center\"\n\
font_size = 9.0\n\
y_margin_factor = 0.45\n\
\n\
[page.content]\n\
top_clearance = 18.0\n\
bottom_clearance = 18.0\n"
}

fn packs_readme() -> String {
    let figure_ids: Vec<&str> = FIGURE_PACKS.iter().map(|p| p.id).collect();
    let figure_loop = figure_ids.join(" ");
    let mut figure_table = String::from("| Pack | Knobs to notice |\n| --- | --- |\n");
    for pack in FIGURE_PACKS {
        let _ = writeln!(figure_table, "| `{}` | {} |", pack.id, pack.knobs);
    }
    let chrome_ids: Vec<&str> = CHROME_PACKS.iter().map(|p| p.id).collect();
    let chrome_loop = chrome_ids.join(" ");
    format!(
        r#"# Browse packs (figure / weave knobs)

Sparse packs for native PDF smoke — not product templates. Each declares a stub
`print` theme (Chromium unused here) plus a `weave.toml` overlay.

Regenerate with `cargo run --example gen_sample_fixtures` (same as samples).

## Figures

Pair with [`../samples/figure_align.tes`](../samples/figure_align.tes):

```bash
cargo run --example gen_sample_fixtures

for id in {figure_loop}; do
  cargo run -q --bin tes --features native-pdf -- export \
    fixtures/samples/figure_align.tes \
    --pdf --backend native \
    --template-root fixtures/packs --template "$id" \
    -o "tmp/tessera-349-smoke/${{id}}.pdf"
done
```

All packs: caption `band = match_figure` (wraps under the image). Left/center/right use
`text_align = follow`. Justify is the same geometry with `text_align = justify`.

{figure_table}
## Page chrome (THI-392)

See [`page_chrome/README.md`](page_chrome/README.md). Tokens: `{{page}}`, `{{pages}}`, `{{title}}`.

```bash
mkdir -p tmp/thi-392-smoke
for doc in {docs}; do
  for pack in {chrome_loop}; do
    cargo run -q --bin tes --features native-pdf -- export \
      "fixtures/samples/${{doc}}.tes" \
      --pdf --backend native \
      --template-root fixtures/packs --template "$pack" \
      -o "tmp/thi-392-smoke/${{doc}}__${{pack}}.pdf"
  done
done
```

## Hyphenation (THI-394)

See [`hyphen_on/README.md`](hyphen_on/README.md). Pair with
[`../samples/hyphen_dense.tes`](../samples/hyphen_dense.tes):

```bash
mkdir -p tmp/thi-394-smoke
for pack in {hyphen_loop}; do
  cargo run -q --bin tes --features native-pdf -- export \
    fixtures/samples/hyphen_dense.tes \
    --pdf --backend native \
    --template-root fixtures/packs --template "$pack" \
    -o "tmp/thi-394-smoke/${{pack}}.pdf"
done
```

## In-document TOC (THI-390)

[`../samples/manuscript_chapters.tes`](../samples/manuscript_chapters.tes) seals
`\toc{{title="Contents" depth=2}}` after front matter. Smoke with page chrome:

```bash
mkdir -p tmp/thi-390-smoke
cargo run -q --bin tes --features native-pdf -- export \
  fixtures/samples/manuscript_chapters.tes \
  --pdf --backend native \
  --template-root fixtures/packs --template page_chrome \
  -o tmp/thi-390-smoke/manuscript_chapters__page_chrome.pdf
```

## Multi-column body (THI-391)

See [`columns_justify/README.md`](columns_justify/README.md). Pair with
[`../samples/article_columns.tes`](../samples/article_columns.tes):

```bash
mkdir -p tmp/thi-391-smoke
for pack in {columns_loop}; do
  cargo run -q --bin tes --features native-pdf -- export \
    fixtures/samples/article_columns.tes \
    --pdf --backend native \
    --template-root fixtures/packs --template "$pack" \
    -o "tmp/thi-391-smoke/article_columns__${{pack}}.pdf"
done
```
"#,
        docs = CHROME_SMOKE_DOCS.join(" "),
        hyphen_loop = HYPHEN_PACKS
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
            .join(" "),
        columns_loop = COLUMNS_PACKS
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn chrome_readme() -> String {
    let mut table = String::from("| Pack | Shows |\n| --- | --- |\n");
    for pack in CHROME_PACKS {
        let _ = writeln!(table, "| `{}` | {} |", pack.id, pack.knobs);
    }
    let chrome_ids: Vec<&str> = CHROME_PACKS.iter().map(|p| p.id).collect();
    format!(
        r#"# Page chrome smoke packs (THI-392)

Native PDF header/footer knobs. Tokens: `{{page}}`, `{{pages}}`, `{{title}}`.
Regenerated by `cargo run --example gen_sample_fixtures`.

{table}
Smoke PDFs: `tmp/thi-392-smoke/<doc>__<pack>.pdf` (`{docs}`).

```bash
mkdir -p tmp/thi-392-smoke
for doc in {docs}; do
  for pack in {packs}; do
    cargo run -q --bin tes -- export "fixtures/samples/${{doc}}.tes" \
      --pdf --backend native \
      --template-root fixtures/packs --template "$pack" \
      -o "tmp/thi-392-smoke/${{doc}}__${{pack}}.pdf"
  done
done
```
"#,
        docs = CHROME_SMOKE_DOCS.join(" / "),
        packs = chrome_ids.join(" "),
    )
}

fn hyphen_readme() -> String {
    let mut table = String::from("| Pack | Shows |\n| --- | --- |\n");
    for pack in HYPHEN_PACKS {
        let _ = writeln!(table, "| `{}` | {} |", pack.id, pack.knobs);
    }
    let ids: Vec<&str> = HYPHEN_PACKS.iter().map(|p| p.id).collect();
    format!(
        r#"# Hyphenation smoke packs (THI-394)

Narrow indent band (`indent.step = 48`) + dense long words in
[`../../samples/hyphen_dense.tes`](../../samples/hyphen_dense.tes).
Compare `hyphen_on` vs `hyphen_off` side by side.

{table}
```bash
mkdir -p tmp/thi-394-smoke
for pack in {packs}; do
  cargo run -q --bin tes -- export fixtures/samples/hyphen_dense.tes \
    --pdf --backend native \
    --template-root fixtures/packs --template "$pack" \
    -o "tmp/thi-394-smoke/${{pack}}.pdf"
done
```
"#,
        packs = ids.join(" "),
    )
}

fn columns_readme() -> String {
    let mut table = String::from("| Pack | Shows |\n| --- | --- |\n");
    for pack in COLUMNS_PACKS {
        let _ = writeln!(table, "| `{}` | {} |", pack.id, pack.knobs);
    }
    let ids: Vec<&str> = COLUMNS_PACKS.iter().map(|p| p.id).collect();
    format!(
        r#"# Multi-column body smoke packs (THI-391)

Pair with [`../../samples/article_columns.tes`](../../samples/article_columns.tes)
(2-col then 3-col lorem; mid heading spans). Paragraph align is pack-global —
export both packs to compare flush-left vs justified column bands.

{table}
```bash
mkdir -p tmp/thi-391-smoke
for pack in {packs}; do
  cargo run -q --bin tes --features native-pdf -- export \
    fixtures/samples/article_columns.tes \
    --pdf --backend native \
    --template-root fixtures/packs --template "$pack" \
    -o "tmp/thi-391-smoke/article_columns__${{pack}}.pdf"
done
```
"#,
        packs = ids.join(" "),
    )
}
