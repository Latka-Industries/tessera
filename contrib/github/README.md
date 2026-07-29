# GitHub helpers for Tessera vaults

GitHub does **not** run local `tes textconv` / merge drivers, so `.tes` blobs
stay binary on the Files tab. Use the Tessprek preview Action instead of
dual-committing Markdown sidecars.

## Tessprek preview (THI-212 / THI-219)

1. Copy [`tes-pr-preview.yml`](tes-pr-preview.yml) to
   `.github/workflows/tes-pr-preview.yml`.
2. Copy these scripts from the Tessera repo into your vault:
   - [`scripts/tes-pr-resolve-range.sh`](../../scripts/tes-pr-resolve-range.sh)
   - [`scripts/tes-pr-textconv-diff.sh`](../../scripts/tes-pr-textconv-diff.sh)
   - [`scripts/tes-pr-upsert-comment.sh`](../../scripts/tes-pr-upsert-comment.sh)
   - [`scripts/tes-pr-write-summary.sh`](../../scripts/tes-pr-write-summary.sh)
3. Ensure a `tes` binary is available (template uses
   `cargo install tessera-doc`, or pin a release).

When `*.tes` changes:

| Event | Where readable diffs show up |
| --- | --- |
| Pull request | Sticky PR comment (upserted) |
| Push (any branch, including `master` / `dev`) | Actions **job summary** + `tessprek-tes-preview` artifact |

**Limitations:** github.com file pages and merge UI stay opaque. Local clones
should still use `.gitattributes` `diff=tessera` + `tes textconv` (see
[docs/cli.md](../../docs/cli.md)).

The Tessera reference repo builds `tes` from source in
`.github/workflows/tes-pr-preview.yml` and reuses the same `scripts/` helpers.
