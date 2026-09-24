#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repository_root"

version="$(node -p "require('./apps/desktop/src-tauri/tauri.conf.json').version")"
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]]; then
  echo "Invalid application version: $version" >&2
  exit 1
fi

deb_bundle="${1:-target/release/bundle/deb/Rehow_${version}_amd64.deb}"
output_directory="${2:-target/release/bundle/arch}"
template="packaging/arch/PKGBUILD.template"
arch_image="archlinux@sha256:8745817f349ed24373341ddb92776209eeec3f0364ea48f7f645ac5800d30a50"

for command in ar bsdtar docker install node sed sha256sum tar; do
  command -v "$command" >/dev/null || {
    echo "Required command is unavailable: $command" >&2
    exit 1
  }
done
test -f "$deb_bundle"
test -f "$template"

build_root="$(mktemp -d)"
trap 'rm -rf -- "$build_root"' EXIT
mkdir -p "$build_root/payload" "$build_root/makepkg"

data_member="$(ar t "$deb_bundle" | awk '/^data\.tar(\.|$)/ { print }')"
if [[ -z "$data_member" || "$data_member" == *$'\n'* ]]; then
  echo "The Debian package must contain exactly one data archive." >&2
  exit 1
fi
ar p "$deb_bundle" "$data_member" > "$build_root/data-archive"
bsdtar -xf "$build_root/data-archive" -C "$build_root/payload"
test -x "$build_root/payload/usr/bin/rehow"

tar \
  --sort=name \
  --mtime='UTC 1970-01-01' \
  --owner=0 \
  --group=0 \
  --numeric-owner \
  -C "$build_root/payload" \
  -cf "$build_root/makepkg/rehow-payload.tar" \
  usr

payload_sha256="$(sha256sum "$build_root/makepkg/rehow-payload.tar" | awk '{ print $1 }')"
sed \
  -e "s/@VERSION@/$version/g" \
  -e "s/@PAYLOAD_SHA256@/$payload_sha256/g" \
  "$template" > "$build_root/makepkg/PKGBUILD"

docker run --rm --network none \
  --env BUILD_GID="$(id -g)" \
  --env BUILD_UID="$(id -u)" \
  --volume "$build_root/makepkg:/build" \
  --workdir /build \
  "$arch_image" \
  bash -ceu '
    group_name="$(getent group "$BUILD_GID" | cut -d: -f1 || true)"
    if [[ -z "$group_name" ]]; then
      group_name=rehow-build
      groupadd --gid "$BUILD_GID" "$group_name"
    fi
    user_name="$(getent passwd "$BUILD_UID" | cut -d: -f1 || true)"
    if [[ -z "$user_name" ]]; then
      user_name=rehow-build
      useradd --uid "$BUILD_UID" --gid "$group_name" --home-dir /build --no-create-home "$user_name"
    fi
    runuser -u "$user_name" -- env HOME=/build makepkg --nodeps --nocheck --cleanbuild --force
  '

package_name="rehow-${version}-1-x86_64.pkg.tar.zst"
test -f "$build_root/makepkg/$package_name"
mkdir -p "$output_directory"
install -m 0644 "$build_root/makepkg/$package_name" "$output_directory/$package_name"
printf '%s\n' "$output_directory/$package_name"
