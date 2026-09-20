# Phase 6A MCP protocol foundation evidence

Date: 2026-09-20

Implemented the separately runnable, read-only stdio MCP foundation selected by ADR 0026. The server accepts 1–16 explicit aliased compatible Workspace roots, rejects canonical duplicates/overlap, retains descriptor-confined core views after identity recheck, assigns random launch handles, and returns no absolute paths. The only tools are `workspaces_list` and bounded-summary `workspace_get`.

Protocol dispatch uses exact-pinned official `rmcp` 3.4.0. Rehow owns newline framing because the SDK convenience reader is unbounded: input is limited to 1 MiB, malformed/overlong/unterminated frames fail closed, and stdout is transport-only. Tests cover current discovery, legacy initialization, exact tool surface, tool invocation, CLI limits, compatible/incompatible roots, overlap, path redaction, exact/over-limit frames, CRLF, malformed JSON, and EOF fragments. Security review found and closed a root-selection race: the core now opens and retains the selected root descriptor before canonicalization, binds the canonical identity to that descriptor, and inspection consumes the exact retained selection. A deterministic pathname-replacement test proves the replacement is not adopted.

Focused verification:

- `cargo check -p rehow-mcp --all-targets`
- `cargo test -p rehow-mcp --all-features` — 11 unit tests and 1 binary integration test passed
- `pnpm check` — passed
- `pnpm test` — 39 Vitest; 441 core passed/2 ignored; public API/error/protocol/unknown-YAML suites; 25 desktop; 11 MCP unit and 1 MCP binary integration test passed
- `pnpm build` — production web and full Cargo workspace build passed
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` — passed
- `cargo fmt --all --check`, bootstrap validation, and `git diff --check` — passed

The single reused Security Reviewer returned PASS after the root-race correction and final root-symlink compatibility delta.
