# Phase 6B MCP content-read evidence

Date: 2026-09-20

Phase 6B expands the standalone server from two setup tools to an exact nine-tool read-only surface. Startup-granted Workspace handles now support bounded Category and Guide listing, Category/Guide lookup, core-ranked Guide search, current diagnostics, and descriptor-confined refresh. No runtime path, mutation, Git, process, network, raw Attachment, or tested-status authority was added.

List, search, and validation responses default to 25 entries and cap at 100. Versioned cursors bind to the offset, tool, filters, launch handle, and retained view with a per-launch keyed BLAKE3 MAC. Guide bodies default to 256 KiB, cap at the canonical 2 MiB limit, truncate at UTF-8 boundaries, and disclose truncation; fenced code extraction is separately limited to 128 blocks and 128 KiB. Search filtering occurs inside core before its ranking cap, and duplicate portable Workspace IDs are recomputed coherently after refresh. All content is described as untrusted data.

Verification:

- `cargo test -p rehow-mcp` — 15 unit tests and 1 binary integration test passed.
- `pnpm check` — engine, Biome, TypeScript, and generated-contract checks passed.
- `pnpm test` — 39 Vitest tests, 441 core tests passed/2 ignored plus supporting suites, 25 desktop tests, and 16 MCP tests passed.
- `pnpm build` — production desktop web build and full Cargo workspace build passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — passed.
- `cargo fmt --all --check` and `git diff --check` — passed.

The focused protocol fixture calls Category listing, ranked error-message search, a truncated Guide read with separately extracted code, and validation through the legacy lifecycle. Current discovery, exact tool enumeration, framing, stdout purity, root authority, core search behavior, refresh recovery, and filesystem confinement remain covered by the shared suites.

The single reused Security Reviewer found three medium issues: editable cursor offsets, Category/Tag filters applied after the search cap, and duplicate portable IDs cached across refresh. The implementation now authenticates cursor offsets with a per-launch BLAKE3 key, filters inside core before ranking truncation, and derives duplicate state from one coherent all-Workspace view snapshot. Regression tests cover all three; the correction re-review returned PASS.
