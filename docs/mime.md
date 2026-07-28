# Tessera MIME type and `file(1)` identification

**Status:** provisional (not IANA-registered). Layout v0 wire may still change.

## Extension and MIME

| Field | Value |
| --- | --- |
| Filename extension | `.tes` |
| Provisional MIME type | `application/vnd.tessera` |
| Magic at offset 0 | ASCII `TESS` (4 bytes) |
| Layout version | `u32` LE at offset 4; this build: `0` |

Register under shared-mime-info / OS handlers with:

```xml
<mime-type type="application/vnd.tessera">
  <comment>Tessera document</comment>
  <glob pattern="*.tes"/>
  <magic priority="50">
    <match type="string" offset="0" value="TESS"/>
  </magic>
</mime-type>
```

Image chunk payloads carry their own IANA `media_type` (e.g. `image/png`); that is
unrelated to the container MIME.

## `file(1)` magic

Checked-in magic database: [`contrib/magic/tessera`](../contrib/magic/tessera).

```bash
file -m contrib/magic/tessera fixtures/v0/note_one_chunk.tes
# → Tessera document, application/vnd.tessera
```

To install system-wide (optional):

```bash
# Linux (example)
sudo cp contrib/magic/tessera /usr/share/misc/magic.d/tessera
sudo file -C -m /usr/share/misc/magic
```

## Spec text license

Wire-format and design documents under [`docs/`](.) are licensed separately from
the Rust crate. See [`docs/LICENSE`](LICENSE) (CC BY 4.0). Code remains
MIT OR Apache-2.0.

## Conformance kit

Must-accept / must-reject fixtures live under
[`fixtures/conformance/`](../fixtures/conformance/). Readers that claim Tessera
compatibility must:

1. **Accept** every file in `accept/` (`tes verify --deep` exits 0).
2. **Reject** every file in `reject/` (`tes verify --deep` exits 1 with at
   least one error finding).

Structural rejects fail even shallow verify; layout-v1 / attachment rejects
require deep payload decode. See
[`fixtures/conformance/README.md`](../fixtures/conformance/README.md) for the
file list and regenerate commands.
