#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
target="$repo_root/packages/contracts/src/generated.ts"
temporary=$(mktemp "$repo_root/packages/contracts/src/.generated.ts.XXXXXX")
trap 'rm -f "$temporary"' EXIT HUP INT TERM

cd "$repo_root"
cargo run --quiet --locked -p rehow-core --example export_contracts > "$temporary"
mv "$temporary" "$target"
trap - EXIT HUP INT TERM
