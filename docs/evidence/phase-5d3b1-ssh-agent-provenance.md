# Phase 5D3B1 SSH-agent provenance evidence

Date: 2026-09-19

## Implemented boundary

- Native application state reads the launch-inherited `SSH_AUTH_SOCK` once and retains a closed `notConfigured`, `unsafe`, or `trusted` result.
- Trusted means an absolute bounded user-owned Unix socket with one link, private permissions, safe non-symlink ancestry, matching pre-open/path/held identities, and an `O_PATH | O_NOFOLLOW | O_CLOEXEC` held inode.
- Public status and `Debug` reveal no path, environment value, inode, or descriptor.
- This slice makes no connection, invokes no process or network, reads no key/config/host data, adds no renderer command, and mutates no filesystem state.

## Verification

- Focused core tests: 2 passed, covering trusted capture, path-redacted `Debug`, retained-inode behavior after replacement, and missing/relative/oversized/regular/symlink/parent-traversal/loose-socket/loose-parent refusal.
- Full locked Workspace tests: core 418 passed with 2 intentional network/performance ignores; 18 error-contract, 13 protocol, public-API/trybuild, 11 unknown-YAML, 24 desktop, and 1 MCP test passed.
- Core/desktop strict all-target/all-feature Clippy passes with warnings denied. Cargo formatting, `pnpm check`, contract drift, TypeScript, Biome, production Vite/Cargo builds, optimized Tauri build, bootstrap validation, and whitespace checks pass.

## Review

No extra review agent is used for this non-connecting foundation. The single Security Reviewer is intentionally deferred to the completed authority-bearing SSH invocation gate, where proxy grammar, host keys, descriptor handoff, credential canaries, and sandbox mounts can be reviewed together.
