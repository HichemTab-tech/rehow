# Phase 6C1 MCP Guide-update evidence

Date: 2026-09-20

Phase 6C1 introduces Rehow's first MCP mutation without broadening the root or process boundary. Read remains the default. The launcher must separately pass `--allow-write <alias>` for a configured Workspace; repeated/unknown grants fail. The granted Workspace must be compatible and writable, and write capability still excludes creation, deletion, lifecycle changes, tested approval, Attachments, broad operations, Git, arbitrary paths, shell, and network.

The exact ten-tool surface adds only `guide_update`. It requires a launch handle, stable Guide ID, exact revision, and a complete title/summary/Tag/alias/body replacement. Rust composes the five typed mutations into one candidate and uses the existing cooperating lock, fresh target reread, revision check, atomic exchange, before-image, journal, and refresh path. Unknown metadata is preserved. The request contains no status or authoritative ID/time/path fields. Body input is capped at 768 KiB under the 1 MiB frame bound.

Valid domain failures use MCP `isError: true` with the shared serialized `ErrorDto`; stale errors include exact expected and actual revisions without content or paths. A committed write followed by refresh failure reports the committed revision and `refreshRequired: true` instead of claiming rollback.

Verification:

- `cargo test -p rehow-mcp` — 17 unit tests and 1 binary integration test passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — passed.
- `pnpm check` — engine, Biome, TypeScript, and generated-contract checks passed.
- `pnpm test` — 39 Vitest tests; 441 core passed/2 ignored plus contract/API suites; 25 desktop Rust tests; 17 MCP unit and 1 binary integration test passed.
- `pnpm build` — production desktop web build and full Cargo workspace build passed.
- `cargo fmt --all --check`, bootstrap validation, and `git diff --check` — passed.

Focused evidence covers the CLI grant matrix, exact tool/schema surface, absent path/status inputs, read-only byte identity with no recovery-state creation, successful direct and real-protocol writes in temporary Workspace/state roots, complete-field reread, stale-revision byte identity with correlated `REVISION_CONFLICT`, over-limit refusal, stdout purity, and refusal when either retained recovery root overlaps a read-only or writable configured Workspace. Recovery directories are retained by descriptor before serving requests, so later pathname replacement cannot redirect writes.

The full core suite supplies the established recovery interruption, external race, unknown-metadata, presentation-refusal, and tested-to-stale evidence used by this adapter.
