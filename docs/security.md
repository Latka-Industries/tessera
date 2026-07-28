# Security model

**Status:** requirements for readers, writers, preview servers, and themes.

Tessera documents are data. Opening, mapping, verifying, or previewing a
foreign `.tes` file must never execute code.

## No document macros

- The wire format has no script, macro, auto-open, or executable chunk type.
- Document content cannot supply HTML event handlers or executable URLs.
- Theme JavaScript lives in an external pack, is disabled by default, and
  requires an explicit trust decision such as `--allow-theme-js`.
- AI HTML and print/PDF exports never include scripts.
- A future trusted-theme mode must use content hashes/signatures, a restrictive
  CSP, and no filesystem or network access by default.

## Inert attachments

Attachment chunks are opaque bytes until a user explicitly exports or opens
them (`tes export --attachment`, or `tes serve` `/attachment/{id}`). Readers
and preview servers do not auto-extract or execute attachments.

- Normalize filenames to a basename; reject absolute paths and `..`.
- Warn or deny executable/script media types and suffixes by default.
- Serve downloads with a safe content disposition and `nosniff`.
- An integrity hash proves identity, not safety.

## Resource limits

Readers and `tes verify` enforce configurable upper bounds for:

- file length, chunk count, and individual stored/raw payload length;
- catalog/header/link string lengths;
- zstd expansion ratio and exact decoded length;
- image dimensions and decoded pixel count;
- attachment count and aggregate attachment bytes;
- nested structured-table depth/size and inline span count.

Overflow-safe checked arithmetic is required for every offset/length pair.

## Safe rendering and serving

- Escape all text and attribute values on HTML export.
- Sanitize external URI schemes; allow `http`, `https`, `mailto`, and explicit
  application-approved schemes rather than arbitrary `javascript:`.
- AI HTML is a semantic fragment with no style, script, navigation, or theme
  wrappers.
- `tes serve` binds to loopback by default and applies CSS-only themes unless
  the user trusts a theme explicitly.
- Preview responses use a restrictive CSP and do not expose arbitrary local
  paths.

## Safe mutation

Every human or AI mutation follows:

1. acquire a short advisory lock for the target file;
2. re-check the supplied source hash;
3. compile into a sibling temporary file;
4. deep-verify the result;
5. flush and atomically replace the original.

On failure, the original remains untouched. Models never write raw `TESS`
bytes; they submit Tessera Markdown or typed operations through this gate.

Treat a foreign `.tes` like a foreign Office or PDF file: preview with active
content disabled and do not blindly export/open attachments.
