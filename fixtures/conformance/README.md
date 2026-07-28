# Conformance fixtures

Must-accept and must-reject `.tes` files for open-format readers
([docs/mime.md](../../docs/mime.md)).

| Tree | Expectation |
| --- | --- |
| `accept/` | `tes verify --deep` exits 0 |
| `reject/` | `tes verify` exits 1 |

## Accept set (v0)

Copied from `fixtures/v0/` goldens so Windows CI does not depend on symlinks:

- `empty.tes` — superblock-only skeleton
- `note_one_chunk.tes` — catalog + one text chunk
- `hub_links.tes` — hub with `TLNK` edges

## Reject set

Derived from `note_one_chunk.tes` (except `too_short.tes`):

| File | Fault |
| --- | --- |
| `bad_magic.tes` | Byte 0 ≠ `T` |
| `bad_tidx_magic.tes` | `TIDX` magic corrupted |
| `bad_catalog.tes` | Catalog JSON first byte corrupted |
| `truncated.tes` | Last 10 payload bytes removed |
| `history_flag_no_thst.tes` | History flag set without `THST` footer |
| `too_short.tes` | 10 zero bytes (shorter than superblock) |

Regenerate rejects after changing the note golden:

```bash
python3 - <<'PY'
from pathlib import Path
src = Path('fixtures/v0/note_one_chunk.tes').read_bytes()
out = Path('fixtures/conformance/reject')
b = bytearray(src); b[0] = ord('X'); (out/'bad_magic.tes').write_bytes(b)
(out/'truncated.tes').write_bytes(src[:-10])
(out/'too_short.tes').write_bytes(bytes(10))
idx = src.find(b'TIDX'); b = bytearray(src); b[idx] = ord('Z'); (out/'bad_tidx_magic.tes').write_bytes(b)
b = bytearray(src); b[64] = ord('?'); (out/'bad_catalog.tes').write_bytes(b)
b = bytearray(src); b[8:12] = (1).to_bytes(4, 'little'); (out/'history_flag_no_thst.tes').write_bytes(b)
PY
cp fixtures/v0/*.tes fixtures/conformance/accept/
```
