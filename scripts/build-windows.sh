#!/usr/bin/env bash
# Cross-build tree.exe on Linux (cargo-xwin supplies the MSVC SDK/CRT).
set -euo pipefail
cd "$(dirname "$0")/.."

TARGET=x86_64-pc-windows-msvc

if ! command -v cargo-xwin >/dev/null; then
  echo "cargo-xwin is missing: cargo install --locked cargo-xwin" >&2
  exit 1
fi

cargo xwin build --release --target "$TARGET" -p tree-win

mkdir -p dist
cp "target/$TARGET/release/tree.exe" dist/tree.exe

size=$(stat -c%s dist/tree.exe)
sha=$(sha256sum dist/tree.exe | cut -d' ' -f1)
printf 'tree.exe  %s bytes\nsha256    %s\n' "$size" "$sha"
file dist/tree.exe
