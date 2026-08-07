//! Browse packs for native figure/weave smoke (`fixtures/packs/`).
//!
//! Not product templates. Regenerate with `cargo run --example gen_sample_fixtures`.

use std::fs;
use std::path::Path;

use crate::error::Result;

/// One figure_* pack: only `align` / caption `text_align` differ.
struct FigurePack {
    id: &'static str,
    align: &'static str,
    caption_text_align: &'static str,
    comment: &'static str,
}

const FIGURE_PACKS: &[FigurePack] = &[
    FigurePack {
        id: "figure_left",
        align: "left",
        caption_text_align: "follow",
        comment: "Left figure band — caption follows figure + wraps in the band.",
    },
    FigurePack {
        id: "figure_center",
        align: "center",
        caption_text_align: "follow",
        comment: "Center figure band — caption follows figure + wraps in the band.",
    },
    FigurePack {
        id: "figure_right",
        align: "right",
        caption_text_align: "follow",
        comment: "Right figure band — caption follows figure + wraps in the band.",
    },
    FigurePack {
        id: "figure_caption_justify",
        align: "center",
        caption_text_align: "justify",
        comment: "Same as figure_center (match_figure band + wrap), but caption text is justified.",
    },
];

const STUB_CSS: &str = "/* stub — native PDF ignores pack CSS; required by TemplatePack::load */\nbody { margin: 0; }\n";

/// Write every figure_* smoke pack under `dir` (typically `fixtures/packs`).
///
/// # Errors
///
/// Returns I/O errors from creating directories or writing files.
pub fn write_all(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    for pack in FIGURE_PACKS {
        write_figure_pack(dir, pack)?;
    }
    fs::write(dir.join("README.md"), PACKS_README)?;
    Ok(())
}

fn write_figure_pack(root: &Path, pack: &FigurePack) -> Result<()> {
    let pack_dir = root.join(pack.id);
    let themes = pack_dir.join("themes");
    fs::create_dir_all(&themes)?;
    fs::write(
        pack_dir.join("manifest.json"),
        format!(
            "{{\n  \"id\": \"{}\",\n  \"version\": \"0.1.0\",\n  \"compatible_layout\": 0,\n  \"doc_kind_default\": \"note\",\n  \"themes\": {{\n    \"print\": \"themes/print.css\"\n  }},\n  \"export_targets\": [\"pdf\"]\n}}\n",
            pack.id
        ),
    )?;
    fs::write(themes.join("print.css"), STUB_CSS)?;
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

const PACKS_README: &str = "# Browse packs (figure / weave knobs)

Sparse packs for native PDF smoke — not product templates. Each declares a stub
`print` theme (Chromium unused here) plus a `weave.toml` overlay.

Regenerate with `cargo run --example gen_sample_fixtures` (same as samples).

Pair with [`../samples/figure_align.tes`](../samples/figure_align.tes):

```bash
cargo run --example gen_sample_fixtures

for id in figure_left figure_center figure_right figure_caption_justify; do
  cargo run -q --bin tes --features native-pdf -- export \\
    fixtures/samples/figure_align.tes \\
    --pdf --backend native \\
    --template-root fixtures/packs --template \"$id\" \\
    -o \"tmp/tessera-349-smoke/${id}.pdf\"
done
```

All packs: caption `band = match_figure` (wraps under the image). Left/center/right use
`text_align = follow`. Justify is the same geometry with `text_align = justify`.

| Pack | Knobs to notice |
| --- | --- |
| `figure_left` | `[figure].align = left`, caption/title `follow` |
| `figure_center` | `align = center`, caption/title `follow` |
| `figure_right` | `align = right`, caption/title `follow` |
| `figure_caption_justify` | same as center, caption `text_align = justify` |
";
