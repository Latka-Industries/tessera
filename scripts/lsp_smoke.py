# /// script
# requires-python = ">=3.12"
# ///
"""Stdio smoke tests for ``tes-lsp`` (THI-241–244).

Run from the repo root::

    cargo build --bin tes-lsp
    uv run scripts/lsp_smoke.py

Or via mise::

    mise run lsp-smoke
"""

from __future__ import annotations

import json
import os
import select
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CLEAN = ROOT / "fixtures" / "v0" / "note_one_chunk.tes"
BAD_MAGIC = ROOT / "fixtures" / "conformance" / "reject" / "bad_magic.tes"


def find_tes_lsp() -> Path:
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    candidates = [
        target / "debug" / "tes-lsp",
        ROOT / "target" / "debug" / "tes-lsp",
    ]
    for path in candidates:
        if path.is_file() and os.access(path, os.X_OK):
            return path
    which = shutil.which("tes-lsp")
    if which:
        return Path(which)
    raise SystemExit(
        "tes-lsp binary not found — run: cargo build --bin tes-lsp "
        "(or: mise run tes-lsp)"
    )


@dataclass
class LspSession:
    proc: subprocess.Popen[bytes]
    _buf: bytearray = field(default_factory=bytearray)
    messages: list[dict[str, Any]] = field(default_factory=list)

    def send(self, obj: dict[str, Any]) -> None:
        body = json.dumps(obj, separators=(",", ":")).encode()
        assert self.proc.stdin is not None
        self.proc.stdin.write(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
        self.proc.stdin.flush()

    def _read_available(self) -> None:
        assert self.proc.stdout is not None
        fd = self.proc.stdout.fileno()
        while True:
            ready, _, _ = select.select([fd], [], [], 0)
            if not ready:
                return
            chunk = os.read(fd, 65536)
            if not chunk:
                return
            self._buf.extend(chunk)

    def drain(self, seconds: float = 0.4) -> None:
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            self._read_available()
            self._parse_frames()
            time.sleep(0.05)

    def _parse_frames(self) -> None:
        while True:
            header_end = self._buf.find(b"\r\n\r\n")
            if header_end < 0:
                return
            header = self._buf[:header_end].decode("utf-8", "replace")
            length = None
            for line in header.split("\r\n"):
                if line.lower().startswith("content-length:"):
                    length = int(line.split(":", 1)[1].strip())
            if length is None:
                raise RuntimeError(f"missing Content-Length in {header!r}")
            start = header_end + 4
            end = start + length
            if len(self._buf) < end:
                return
            body = bytes(self._buf[start:end])
            del self._buf[:end]
            self.messages.append(json.loads(body))

    def close(self) -> None:
        if self.proc.stdin and not self.proc.stdin.closed:
            self.proc.stdin.close()
        try:
            self.proc.wait(timeout=2)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait(timeout=2)

    def results(self) -> list[dict[str, Any]]:
        return [m for m in self.messages if "result" in m]

    def notifications(self, method: str) -> list[dict[str, Any]]:
        return [m for m in self.messages if m.get("method") == method]


def open_session(bin_path: Path) -> LspSession:
    proc = subprocess.Popen(
        [str(bin_path)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=None,  # inherit — smoke logs stay visible
        cwd=ROOT,
    )
    return LspSession(proc=proc)


def handshake(session: LspSession) -> dict[str, Any]:
    session.send(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"capabilities": {}, "processId": None},
        }
    )
    session.drain(0.5)
    session.send({"jsonrpc": "2.0", "method": "initialized", "params": {}})
    session.drain(0.2)
    results = session.results()
    if not results:
        raise AssertionError("no initialize result")
    return results[0]["result"]


def did_open(session: LspSession, path: Path, *, version: int = 1) -> None:
    uri = path.resolve().as_uri()
    session.send(
        {
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": "tessprek",
                    "version": version,
                    "text": "",
                }
            },
        }
    )
    session.drain(1.0)


def did_change(session: LspSession, path: Path, text: str, *, version: int = 2) -> None:
    uri = path.resolve().as_uri()
    session.send(
        {
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {"uri": uri, "version": version},
                "contentChanges": [{"text": text}],
            },
        }
    )
    session.drain(0.8)


def did_close(session: LspSession, path: Path) -> None:
    uri = path.resolve().as_uri()
    session.send(
        {
            "jsonrpc": "2.0",
            "method": "textDocument/didClose",
            "params": {"textDocument": {"uri": uri}},
        }
    )
    session.drain(0.5)


def case_handshake(bin_path: Path) -> None:
    s = open_session(bin_path)
    try:
        result = handshake(s)
        caps = result.get("capabilities", {})
        sync = caps.get("textDocumentSync") or {}
        info = result.get("serverInfo") or {}
        assert info.get("name") == "tes-lsp", info
        assert sync.get("openClose") is True, sync
        assert sync.get("change") == 1, sync  # Full
        print("ok  handshake")
    finally:
        s.close()


def case_open_clean(bin_path: Path) -> None:
    s = open_session(bin_path)
    try:
        handshake(s)
        did_open(s, CLEAN)
        diags = s.notifications("textDocument/publishDiagnostics")
        assert diags, "expected publishDiagnostics"
        last = diags[-1]["params"]["diagnostics"]
        errors = [d for d in last if d.get("severity") == 1]
        assert not errors, f"unexpected errors on clean fixture: {errors}"
        print("ok  open clean (no error diagnostics)")
    finally:
        s.close()


def case_did_change(bin_path: Path) -> None:
    before = CLEAN.read_bytes()
    s = open_session(bin_path)
    try:
        handshake(s)
        did_open(s, CLEAN)
        did_change(s, CLEAN, "<!-- tessera edited -->\nhello\n")
        logs = s.notifications("window/logMessage")
        assert any("changed" in (m.get("params") or {}).get("message", "") for m in logs), logs
        assert any("(unchanged)" in (m.get("params") or {}).get("message", "") for m in logs), logs
        print("ok  didChange (in-memory, hash unchanged log)")
    finally:
        s.close()
    after = CLEAN.read_bytes()
    assert before == after, "didChange must not mutate .tes on disk"
    print("ok  didChange disk unchanged")


def case_bad_magic_diagnostics(bin_path: Path) -> None:
    s = open_session(bin_path)
    try:
        handshake(s)
        did_open(s, BAD_MAGIC)
        diags = s.notifications("textDocument/publishDiagnostics")
        assert diags, "expected publishDiagnostics for bad_magic"
        last = diags[-1]["params"]["diagnostics"]
        assert last, "expected at least one diagnostic"
        assert any(
            d.get("severity") == 1
            and (
                d.get("code") == "edit-read"
                or "magic" in d.get("message", "").lower()
            )
            for d in last
        ), last
        print("ok  bad_magic publishDiagnostics")
    finally:
        s.close()


def case_close_clears(bin_path: Path) -> None:
    s = open_session(bin_path)
    try:
        handshake(s)
        did_open(s, CLEAN)
        did_close(s, CLEAN)
        diags = s.notifications("textDocument/publishDiagnostics")
        assert diags, "expected diagnostics traffic"
        last = diags[-1]["params"]["diagnostics"]
        assert last == [], f"expected cleared diagnostics on close, got {last}"
        print("ok  didClose clears diagnostics")
    finally:
        s.close()


CASES = [
    ("handshake", case_handshake),
    ("open_clean", case_open_clean),
    ("did_change", case_did_change),
    ("bad_magic", case_bad_magic_diagnostics),
    ("close_clears", case_close_clears),
]


def main(argv: list[str]) -> int:
    selected = set(argv[1:]) if len(argv) > 1 else None
    bin_path = find_tes_lsp()
    print(f"binary: {bin_path}")
    print(f"root:   {ROOT}")

    failed = 0
    for name, fn in CASES:
        if selected is not None and name not in selected:
            continue
        try:
            fn(bin_path)
        except Exception as exc:  # noqa: BLE001 — report and continue
            failed += 1
            print(f"FAIL {name}: {exc}", file=sys.stderr)

    if failed:
        print(f"{failed} case(s) failed", file=sys.stderr)
        return 1
    print("all lsp smoke cases passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
