# Template packs

Packs live under this directory. Each pack is a folder with `manifest.json` and
theme CSS.

Optional D23 authoring overlays may be:

- **Sparse** convention files (`weave.toml`, `typography.toml`, `aliases.toml`,
  `phrases.toml`, `fonts.toml`), or
- **Master** `tessera.toml` (THI-367; manifest field `pack` for a non-default
  path)

Normative rules (section shape, conflict = hard error, Tesscriptor targets
master): [docs/decisions.md — D23](../docs/decisions.md).

| Example | Form |
| --- | --- |
| [`minimal/`](minimal/) | Sparse siblings |
| [`master_pack/`](master_pack/) | Master `tessera.toml` only |
