#!/usr/bin/env bash
set -euo pipefail

profile="${1:-dev}"
case "$profile" in
  dev)
    cargo_args=()
    output_dir="target/debug"
    ;;
  release)
    cargo_args=(--release)
    output_dir="target/release"
    ;;
  *)
    echo "usage: $0 [dev|release]" >&2
    exit 2
    ;;
esac

cargo build -p rehow-core --bin rehow-git-ssh --locked "${cargo_args[@]}"
temporary="$(mktemp "$output_dir/.rehow-git-ssh.XXXXXX")"
trap 'rm -f "$temporary"' EXIT
install -m 0755 "$output_dir/rehow-git-ssh" "$temporary"
mv -f "$temporary" "$output_dir/rehow-git-ssh"
trap - EXIT
