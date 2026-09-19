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

cargo build -p rehow-core --bin rehow-git-ssh --bin rehow-git-credential --locked "${cargo_args[@]}"
for helper in rehow-git-ssh rehow-git-credential; do
  temporary="$(mktemp "$output_dir/.${helper}.XXXXXX")"
  trap 'rm -f "$temporary"' EXIT
  install -m 0755 "$output_dir/$helper" "$temporary"
  mv -f "$temporary" "$output_dir/$helper"
  trap - EXIT
done
