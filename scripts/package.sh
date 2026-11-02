#!/usr/bin/env bash
# Portable zip + NSIS installer + checksums, all into dist/.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
PORTABLE="dist/Tree_${VERSION}_portable_x64.zip"
SETUP="dist/Tree_${VERSION}_setup.exe"

[ -f dist/tree.exe ] || ./scripts/build-windows.sh

python3 - "$PORTABLE" <<'PY'
import sys, zipfile
out = sys.argv[1]
readme = """Tree — native Windows download manager
=======================================

Portable build. Nothing to install, no runtime required (static CRT).

Run tree.exe. It keeps its state in %APPDATA%\\Tree unless a file named
tree.ini sits next to the exe, in which case everything stays in this folder.

Unsigned build: Windows SmartScreen may show "Windows protected your PC".
Choose More info -> Run anyway.

Requirements: Windows 10 or 11, 64-bit.
"""
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    z.write("dist/tree.exe", "tree.exe")
    z.writestr("READ ME FIRST.txt", readme)
    z.write("LICENSE", "LICENSE")
print(f"portable: {out}")
PY

makensis installer/tree.nsi

(
  cd dist
  sha256sum "tree.exe" "$(basename "$PORTABLE")" "$(basename "$SETUP")" > SHA256SUMS.txt
  cat SHA256SUMS.txt
)
