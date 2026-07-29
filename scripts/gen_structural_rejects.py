# /// script
# requires-python = ">=3.12"
# ///
"""Derive structural must-reject fixtures from ``fixtures/v0/note_one_chunk.tes``.

Run from the repo root::

    uv run scripts/gen_structural_rejects.py
"""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "fixtures" / "v0" / "note_one_chunk.tes"
OUT = ROOT / "fixtures" / "conformance" / "reject"


def write_reject(name: str, data: bytes, fault: str) -> Path:
    path = OUT / name
    path.write_bytes(data)
    rel = path.relative_to(ROOT)
    print(f"  wrote {rel} ({len(data)} bytes) — {fault}")
    return path


def main() -> int:
    print(f"source: {SRC.relative_to(ROOT)}")
    if not SRC.is_file():
        print(f"missing {SRC} — run: cargo run --example gen_v0_fixtures", file=sys.stderr)
        return 1

    src = SRC.read_bytes()
    print(f"read {len(src)} bytes from note_one_chunk.tes")
    OUT.mkdir(parents=True, exist_ok=True)
    print(f"output: {OUT.relative_to(ROOT)}/")

    bad_magic = bytearray(src)
    bad_magic[0] = ord("X")
    write_reject("bad_magic.tes", bytes(bad_magic), "byte 0 ≠ 'T'")

    write_reject("truncated.tes", src[:-10], "last 10 payload bytes removed")
    write_reject("too_short.tes", bytes(10), "10 zero bytes (shorter than superblock)")

    idx = src.find(b"TIDX")
    if idx < 0:
        print(f"{SRC}: TIDX magic not found", file=sys.stderr)
        return 1
    print(f"TIDX magic at offset {idx}")
    bad_tidx = bytearray(src)
    bad_tidx[idx] = ord("Z")
    write_reject("bad_tidx_magic.tes", bytes(bad_tidx), f"TIDX magic corrupted at offset {idx}")

    bad_catalog = bytearray(src)
    bad_catalog[64] = ord("?")
    write_reject("bad_catalog.tes", bytes(bad_catalog), "catalog JSON first byte corrupted (offset 64)")

    history = bytearray(src)
    history[8:12] = (1).to_bytes(4, "little")
    write_reject(
        "history_flag_no_thst.tes",
        bytes(history),
        "history flag set without THST footer",
    )

    print(f"done: 6 structural rejects from {SRC.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
