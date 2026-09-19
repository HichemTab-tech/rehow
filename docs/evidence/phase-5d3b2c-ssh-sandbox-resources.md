# Phase 5D3B2C sealed SSH sandbox resources evidence

Date: 2026-09-19

## Implemented boundary

- Native startup captures only an exact sibling proxy after stable owner/mode/type/link/ancestry checks; missing/unsafe status never blocks normal startup.
- Resource preparation requires trusted agent, host-key, and proxy provenance. Host-key snapshots are aggregate-bounded and copied into `0400` memfds carrying write/grow/shrink/further-seal seals.
- Bubblewrap uses `--ro-bind-fd` for the exact held socket/proxy inodes and `--ro-bind-data` for sealed key files at fixed compiled paths. No source path is reopened and no Workspace mount is supplied.
- Resources remain crate-private and unreachable from clone, sync, or push. No public network or credential operation exists in this slice.

## Verification

- Three focused tests cover trusted/redacted proxy capture; writable/symlink refusal; missing agent/key/proxy and aggregate-overflow mapping; required memfd seals; and a networkless Bubblewrap child that sees only fixed readable socket/key/executable mounts, no `/workspace`, and emits no output.
- Full locked Workspace tests pass: core 426 with 2 intentional network/performance ignores; 18 error-contract, 13 protocol, public-API/trybuild, 11 unknown-YAML, 24 desktop, and 1 MCP test.
- Strict core/desktop all-target/all-feature Clippy, Cargo formatting, `pnpm check`, contract drift, TypeScript, Biome, production Vite build, optimized Tauri build, bootstrap validation, and whitespace checks pass.
- The first combined run exposed filesystem-dependent timestamp changes on a sparse exact-limit host-key fixture. Timestamp equality was removed from authority; stable device/inode/owner/type/mode/link/size plus two identical full reads remain, and a same-size deterministic mid-read rewrite still fails closed. The repeated full gate passes.

## Review

The one Security Reviewer remains deferred to the first complete product SSH invocation, where packaging identity, network sandboxing, cancellation, output/error classification, child descriptor closure, host-key/auth failures, and credential canaries can be assessed together.
