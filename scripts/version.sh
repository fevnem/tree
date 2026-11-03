#!/usr/bin/env bash
# One source of truth for the version: Cargo.toml. Rewrites every file that has
# to agree, and refuses when a release tag disagrees.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
[ -n "$VERSION" ] || { echo "no version in Cargo.toml" >&2; exit 1; }
IFS=. read -r MAJOR MINOR PATCH <<< "$VERSION"

# app.rc: FILEVERSION a,b,c,0 and FileVersion/ProductVersion strings
sed -i -E "s/^ FILEVERSION .*/ FILEVERSION ${MAJOR},${MINOR},${PATCH},0/" crates/tree-win/res/app.rc
sed -i -E "s/^ PRODUCTVERSION .*/ PRODUCTVERSION ${MAJOR},${MINOR},${PATCH},0/" crates/tree-win/res/app.rc
sed -i -E "s/VALUE \"FileVersion\", \"[^\"]*\"/VALUE \"FileVersion\", \"${VERSION}.0\"/" crates/tree-win/res/app.rc
sed -i -E "s/VALUE \"ProductVersion\", \"[^\"]*\"/VALUE \"ProductVersion\", \"${VERSION}.0\"/" crates/tree-win/res/app.rc

# installer: output name and DisplayVersion
sed -i -E "s/Tree_[0-9.]+_setup\.exe/Tree_${VERSION}_setup.exe/" installer/tree.nsi
sed -i -E "s/(\"DisplayVersion\" \")[^\"]*/\1${VERSION}/" installer/tree.nsi

# CHANGELOG must already mention the version; that is the review gate.
grep -q "^## ${VERSION}" CHANGELOG.md || { echo "CHANGELOG.md has no '## ${VERSION}' section" >&2; exit 1; }

# A release tag that disagrees with the tree is the classic drift bug.
if TAG=$(git describe --tags --exact-match 2>/dev/null); then
  [ "v${VERSION}" = "$TAG" ] || { echo "tag $TAG != version $VERSION" >&2; exit 1; }
fi

echo "version ${VERSION} applied to app.rc and installer"
