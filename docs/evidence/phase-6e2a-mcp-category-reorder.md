# Phase 6E2A MCP Category reorder evidence

Date: 2026-09-21

`category_reorder_plan` validates a complete issue-free Category inventory at an exact snapshot, resolves all descriptor revisions, derives canonical order values, and exposes no mutation through its input. In sessions lacking write+broad authority or the trusted packaged helper it remains a read-only preview with an explicit warning and no token. Executable plans bind the launch handle, portable Workspace identity, snapshot, exact ordered IDs/revisions, operation, summary, and 90-second expiry.

`category_reorder` accepts only the Workspace handle and canonical plan token. It consumes the private plan, accepts consent only from the sealed native helper, then refreshes and rechecks the exact snapshot, complete inventory, issues, and every revision before calling the already accepted recoverable multi-document core operation. The core repeats the complete snapshot check after acquiring its writer lock and again after candidate/object preparation immediately before journal execution, with cancellation checks on both sides of that final scan. Denial makes no change; replay, expiry, cross-Workspace substitution, cancellation, external changes, and unsafe storage fail closed.

Focused Rust tests cover approved reorder, canonical token shape, exact output, replay refusal, post-plan external changes, a deterministic manifest identity change after the first locked scan and preparation but before the final commit-boundary scan, pre-write core cancellation, the exact 28-tool schema, real JSON-RPC execution, and a real JSON-RPC cancellation while approval is active with byte-for-byte no-write proof. The inherited Phase 6E1B suites cover helper provenance, immutable execution, private pipes/environment, framing, denial/failure, expiry, cancellation, and capacity.

Focused correction gate: `cargo test -p rehow-core category_reorder --all-features`, `cargo test -p rehow-mcp category_reorder --all-features`, strict Clippy for both crates, Cargo format, and whitespace all pass. The reused Security Reviewer reports **PASS** after the cancellation and final commit-boundary snapshot corrections.

The release helper was rebuilt, hashed as BLAKE3 `f2aa932d3b1cee3955afadd52df58e90e3594c359d98dd0e9b5a9627e6c4c5fa`, and embedded into the release server build. `node scripts/smoke-mcp-category-reorder.mjs` copied both Cargo artifacts into a one-link sibling package layout and passed the real graphical **Approve** then **Cancel** flow. Approve performed the exact reorder; Cancel returned `denied` and preserved both Category descriptors byte-for-byte. Generic Yes/No buttons discovered during the smoke were replaced by explicit Approve/Cancel controls, Cancel is the safe default, and a unit test proves that only `GTK_RESPONSE_ACCEPT` authorizes.

Final acceptance gate:

- `cargo test --workspace --all-targets --all-features --locked`: pass; core 446 passed/2 intentional network-performance ignores, desktop 25 passed, MCP 57 passed, approval integration 3 passed, stdio integration 1 passed, and supporting contract/API suites passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all --check`, and `git diff --check`: pass.
- Biome checked all 56 files; direct installed `tsc` checks for desktop/contracts, contract generation comparison, 39 Vitest tests, verification vectors, Git credential-proxy smoke, Vite production build, Cargo workspace build, and bootstrap validation all pass.
- The machine exposes pnpm 11.20.0 while this checkpoint still pins pnpm 11.3.0, so pnpm correctly refused the root wrapper scripts. Per the user instruction not to install or switch Node/pnpm, the final JavaScript gate invoked the already-installed local binaries directly. The same root `pnpm check`, `pnpm test`, and `pnpm build` gate passed before the final Rust/docs/smoke correction delta.
- Reused Security Reviewer final verdict after all correction and explicit-button deltas: **PASS**.
