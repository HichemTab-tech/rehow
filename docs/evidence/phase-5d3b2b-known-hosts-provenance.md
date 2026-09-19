# Phase 5D3B2B known-hosts provenance evidence

Date: 2026-09-19

## Implemented boundary

- Native startup inspects only launch-home `.ssh/known_hosts` and fixed system `/etc/ssh/ssh_known_hosts{,2}`.
- Existing sources require expected user/root ownership, one-link regular type, non-writable mode, safe non-symlink ancestry, no-follow read-only open, stable path/descriptor identity, two identical reads, and a 1 MiB per-source cap.
- Missing sources are optional; any unsafe source discards every trusted peer. Retained snapshots cannot change when the source path is later replaced or rewritten.
- Public status/Debug reveal only `notConfigured`, `trusted`, or `unsafe` plus trusted source count. No paths, identities, or bytes cross the boundary.
- No SSH/Git process, agent connection, network, TOFU prompt/update, renderer IPC, or filesystem mutation occurs.

## Verification

- Two focused tests cover trusted non-UTF-8 bytes, Debug redaction, retained-snapshot immutability, deterministic in-place mutation and path-replacement races, exact/one-over size, absent/relative HOME, symlink/hardlink/loose-mode refusal, and all-or-nothing peer failure.
- Full locked Workspace tests pass: core 423 with 2 intentional network/performance ignores; 18 error-contract, 13 protocol, public-API/trybuild, 11 unknown-YAML, 24 desktop, and 1 MCP test.
- Strict core/desktop all-target/all-feature Clippy, Cargo formatting, `pnpm check`, contract drift, TypeScript, Biome, production Vite build, optimized Tauri build, bootstrap validation, and whitespace checks pass.

## Review

No extra agent is used for this non-connecting capture slice. The one Security Reviewer remains deferred to the completed sandbox handoff/invocation boundary where sealed host-key files, exact agent inode, packaged proxy, network child visibility, cancellation, output redaction, and credential canaries can be reviewed together.
