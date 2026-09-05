#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
temporary=$(mktemp "$repo_root/packages/contracts/src/.generated-check.ts.XXXXXX")
trap 'rm -f "$temporary"' EXIT HUP INT TERM

cd "$repo_root"
cargo run --quiet --locked -p rehow-core --example export_contracts > "$temporary"
cmp "$repo_root/packages/contracts/src/generated.ts" "$temporary"
