#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repository_root"

version="$(node -p "require('./apps/desktop/src-tauri/tauri.conf.json').version")"
deb_bundle="${1:-target/release/bundle/deb/Rehow_${version}_amd64.deb}"
rpm_bundle="${2:-target/release/bundle/rpm/Rehow-${version}-1.x86_64.rpm}"
arch_bundle="${3:-target/release/bundle/arch/rehow-${version}-1-x86_64.pkg.tar.zst}"
test -f "$deb_bundle"
test -f "$rpm_bundle"
test -f "$arch_bundle"

smoke_root="$(mktemp -d)"
trap 'rm -rf -- "$smoke_root"' EXIT
mkdir "$smoke_root/deb" "$smoke_root/rpm" "$smoke_root/arch"
ar p "$deb_bundle" data.tar.gz | tar -xzf - -C "$smoke_root/deb"
bsdtar -xf "$rpm_bundle" -C "$smoke_root/rpm"
bsdtar -xf "$arch_bundle" -C "$smoke_root/arch"

for package_root in "$smoke_root/deb" "$smoke_root/rpm" "$smoke_root/arch"; do
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

  license_file="$package_root/usr/lib/Rehow/LICENSE"
  test -f "$license_file"
  cmp LICENSE "$license_file"

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

test -f "$smoke_root/arch/.BUILDINFO"
test -f "$smoke_root/arch/.MTREE"
grep -Fxq 'pkgname = rehow' "$smoke_root/arch/.PKGINFO"
grep -Fxq "pkgver = ${version}-1" "$smoke_root/arch/.PKGINFO"
grep -Fxq 'arch = x86_64' "$smoke_root/arch/.PKGINFO"
grep -Fxq 'depend = gtk3' "$smoke_root/arch/.PKGINFO"
grep -Fxq 'depend = webkit2gtk-4.1' "$smoke_root/arch/.PKGINFO"
diff --recursive --brief "$smoke_root/deb/usr" "$smoke_root/arch/usr"

arch_image="archlinux@sha256:8745817f349ed24373341ddb92776209eeec3f0364ea48f7f645ac5800d30a50"
arch_bundle_absolute="$(realpath "$arch_bundle")"
docker run --rm --network none \
  --volume "$arch_bundle_absolute:/tmp/rehow.pkg.tar.zst:ro" \
  "$arch_image" \
  bash -ceu '
    pacman -U --noconfirm \
      --assume-installed gtk3 \
      --assume-installed webkit2gtk-4.1 \
      /tmp/rehow.pkg.tar.zst
    pacman -Q rehow
    test -x /usr/bin/rehow
    test -f /usr/share/applications/Rehow.desktop
    pacman -R --noconfirm rehow
    test ! -e /usr/bin/rehow
  '
