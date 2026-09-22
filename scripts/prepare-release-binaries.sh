#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repository_root"

scripts/prepare-git-ssh-helper.sh release
cargo build -p rehow-mcp --release --bin rehow-mcp-approve --locked
approval_digest="$(node scripts/blake3-file.mjs target/release/rehow-mcp-approve)"
REHOW_MCP_APPROVAL_HELPER_BLAKE3="$approval_digest" \
  cargo build -p rehow-mcp --release --bin rehow-mcp --locked

for binary in rehow-git-ssh rehow-git-credential rehow-mcp rehow-mcp-approve; do
  test -x "target/release/$binary"
done
