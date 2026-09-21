# Phase 6E2B MCP Category removal evidence

Date: 2026-09-21

`category_remove_plan` binds one issue-free, empty, non-default Category at its exact descriptor revision and complete inventory snapshot. Read-only sessions receive the preview and no token. Executable plans require startup write+broad authority and the trusted sealed helper. The private binding also includes the portable Workspace identity and resolved relative directory; neither paths nor effects can be supplied to execution.

`category_remove` accepts only the launch handle and canonical single-use token. Native denial preserves bytes. Approval is followed by a fresh full refresh and exact state checks. The core then repeats issue-free snapshot validation under the recovery writer lock and at the journal boundary, with cancellation checks, before moving the descriptor to recoverable trash and removing only the verified-empty directory.

A final uncooperative-writer race cannot be made impossible with filesystem primitives: content may arrive after the last scan but after journal execution begins. The recovery executor refuses nonrecursive directory quarantine, preserves the foreign content and exact trashed descriptor, persists a conflict, and now surfaces `RECOVERY_REQUIRED` with the operation ID and affected descriptor path. A deterministic regression injects that race after the final scan and proves all bytes remain recoverable.

Focused tests pass for core cancellation/final manifest-race refusal, post-scan recovery-required reporting, recoverable removal, exact 30-tool schemas, approved execution, replay refusal, read-only preview, denial with byte identity, nonempty/default refusal, post-plan external change, and real JSON-RPC cancellation without a write.

The independent Security Reviewer found two issues in its first pass: the new cancellation variant was not mapped by the desktop adapter, and a post-final-scan foreign-entry race was incorrectly reported as a no-write stale plan after the descriptor had already moved. Both were corrected. Desktop now has explicit cancellation/recovery codes, messages, and tests. The core now distinguishes a persisted post-progress conflict, preserves both foreign content and the exact trashed descriptor, and returns an operation-bound recovery error that MCP serializes as `RECOVERY_REQUIRED` with the affected path. The reviewer’s requested final correction pass could not run because that agent’s workspace credit ended; this limitation is recorded rather than represented as a reviewer PASS. The Orchestrator inspected the correction diff and ran the focused and full gates below.

The release helper was rebuilt and hashed as BLAKE3 `36b4d77902213c84e972e54e2bd0039dc6caa8b04730fcd100e4a26e6699cc6d`, then embedded into the release server build. `node scripts/smoke-mcp-category-remove.mjs` copied both artifacts to a one-link sibling package layout and passed the real graphical **Approve** then **Cancel** flow: approval removed the exact empty Category through recoverable trash, while cancellation preserved the remaining descriptor byte-for-byte.

Final local acceptance gate:

- `cargo test --workspace --all-targets --all-features --locked`: pass; core 448 passed/2 intentional network-performance ignores, desktop 25 passed, MCP 62 passed, helper integration 3 passed, stdio integration 1 passed, and all contract/API suites passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all --check`, and `git diff --check`: pass.
- Biome checked 57 files; direct installed `tsc` checks for desktop/contracts, generated-contract comparison, 39 Vitest tests, verification vectors, Git helper/credential smoke, Vite production build, Cargo workspace build, and bootstrap validation all pass.
- Bootstrap validation reports 17 required files, 43 ADRs, and all 8 project agents pinned.
- The installed Node is 26.2.0. The machine exposes pnpm 11.20.0 while the repository pins 11.3.0, so the exact-version wrapper refuses as designed; per user instruction, no Node/pnpm installation or switch occurred and the already-installed local binaries ran the JavaScript gate directly.
