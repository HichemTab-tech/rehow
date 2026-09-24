#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repository_root"

deb_bundle="${1:-target/release/bundle/deb/Rehow_0.1.0_amd64.deb}"
rpm_bundle="${2:-target/release/bundle/rpm/Rehow-0.1.0-1.x86_64.rpm}"
test -f "$deb_bundle"
test -f "$rpm_bundle"

smoke_root="$(mktemp -d)"
trap 'rm -rf -- "$smoke_root"' EXIT
mkdir "$smoke_root/deb" "$smoke_root/rpm"
ar p "$deb_bundle" data.tar.gz | tar -xzf - -C "$smoke_root/deb"
bsdtar -xf "$rpm_bundle" -C "$smoke_root/rpm"

for package_root in "$smoke_root/deb" "$smoke_root/rpm"; do
  for binary in rehow rehow-git-ssh rehow-git-credential rehow-mcp rehow-mcp-approve; do
    binary_path="$package_root/usr/bin/$binary"
    test -x "$binary_path"
    test "$(stat -c '%h' "$binary_path")" = "1"
    test "$(stat -c '%a' "$binary_path")" = "755"
  done

  scripts/check-linux-abi-floor.sh \
    "$package_root/usr/bin/rehow" \
    "$package_root/usr/bin/rehow-git-ssh" \
    "$package_root/usr/bin/rehow-git-credential" \
    "$package_root/usr/bin/rehow-mcp" \
    "$package_root/usr/bin/rehow-mcp-approve"

  desktop_file="$package_root/usr/share/applications/Rehow.desktop"
  grep -Fxq 'Type=Application' "$desktop_file"
  grep -Fxq 'Name=Rehow' "$desktop_file"
  grep -Fxq 'Exec=rehow' "$desktop_file"
  grep -Fxq 'Icon=rehow' "$desktop_file"
  grep -Fxq 'Terminal=false' "$desktop_file"
  grep -Fxq 'Categories=Utility;' "$desktop_file"

  for size in 32 128 256 512; do
    icon="$package_root/usr/share/icons/hicolor/${size}x${size}/apps/rehow.png"
    test -f "$icon"
    file "$icon" | grep -Fq "PNG image data, ${size} x ${size},"
  done

  approval_digest="$(node scripts/blake3-file.mjs "$package_root/usr/bin/rehow-mcp-approve")"
  grep -aFq "$approval_digest" "$package_root/usr/bin/rehow-mcp"
  if "$package_root/usr/bin/rehow-mcp" >/dev/null 2>&1; then
    echo "rehow-mcp unexpectedly accepted an empty configuration" >&2
    exit 1
  fi
done
