# Phase 4E search interaction evidence

Date: 2026-09-19

## Behavior and contract

Core now returns each ranked result as a Guide ID, deterministic matched-field reasons, one plain normalized excerpt, its field, and sorted non-overlapping Unicode-scalar highlight ranges. Excerpts are capped at 220 scalar values plus at most two ellipses; highlight count is capped at 32. Ranking/truncation happens before excerpt creation. The Tauri adapter returns no HTML. The renderer rejects unknown fields, extra keys, invalid field names, overlong text/counts, overlapping/out-of-range spans, and then creates only React text and semantic `mark` nodes.

The Reference Desk now exposes Active (default), Tested, Draft, Stale, Archived, and All status scopes. The native adapter validates this closed set and core filters the selected status before ranking and the 100-result cap. A 101-active-match regression proves an Archived-only result cannot be crowded out. Scope changes that exclude the open Guide retain dirty-edit confirmation before reconciling selection to a visible result. Query and non-default scope are restorable URL state. The former decorative “Recently Updated” control is gone. Empty search offers specific Clear Search, Show Active Guides, and Search Archived actions as applicable. Existing Arrow/Enter/Escape result navigation and search shortcuts remain; `/` no longer steals focus from the native scope select.

## Production performance

Command: `cargo run --release -p rehow-core --example search_benchmark --locked --quiet`

Corpus: deterministic 5,000 Guides and 52,665,331 canonical bytes on the Phase 4B reference machine. Three final independent processes against the recorded source hashes:

| Run | Cold rebuild | Warm p50 | Warm p95 | Warm p99 | Limit-1 p95 | Peak RSS |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 1,554.541 ms | 5.965 ms | 7.574 ms | 7.811 ms | 7.620 ms | 145,408 KiB |
| 2 | 1,551.023 ms | 5.784 ms | 7.676 ms | 8.068 ms | 7.563 ms | 145,464 KiB |
| 3 | 1,548.677 ms | 6.063 ms | 7.583 ms | 7.775 ms | 7.630 ms | 145,408 KiB |

Worst results pass the 8 s cold, 50 ms warm p95, and 300 MiB budgets. ADR 0020 records the investigation of the deliberate increase from Phase 4B's ID-only 5.382 ms p95. Limit 1 and limit 100 are effectively equal, so bounded presentation does not scale with the result cap; the new complete-contract p95 remains below 8 ms.

A production Vite build in headless system Chromium measured 40 successive query-to-next-animation-frame samples, including the 20 ms debounce, validated IPC payload, state update, excerpt segmentation, and render: p50 49.975 ms, p95 50.184 ms, p99 50.283 ms. This passes the 100 ms visible budget. The Vitest 5,000-Guide scope plus 100-excerpt transformation gate passes the 50 ms main-thread-task budget.

## Visual and accessibility review

The project-pinned `web-design-guidelines` skill and its current upstream rules were fetched/read on 2026-09-19. The implementation uses a labeled native select, semantic actions, live result counts, visible focus, restorable filter state, system-theme select colors, bounded/wrapping text, `content-visibility` above 50 rows, and no HTML injection or animation. Search field labels communicate meaning beyond color. The visual pass also corrected the existing reader header so a long title keeps useful measure and Guide actions wrap below it rather than crushing it into four lines.

Deterministic synthetic Workspace payloads—never personal content—were captured and inspected:

- `screenshots/search-results-wide-light.png`: 1440×900, light, error/content matches, selection, reader, and corrected header.
- `screenshots/search-results-narrow-dark.png`: 480×800, dark, archived scope and highlighted excerpt with no horizontal overflow.
- `screenshots/search-no-results-720-light.png`: 720×900, actionable no-result state with Clear Search and Search Archived.

No current-guideline blocker was found. The screenshots keep the Reference Desk density and hierarchy; no card nesting, gradient/glow/glass, decorative pills, or marketing treatment was introduced.

## Reproducibility identity

- `crates/rehow-core/src/search.rs`: `33b228edb9981c810329ef40cca735286773e753c96478a9fcc87b9d45a6d7c1`
- `crates/rehow-core/examples/search_benchmark.rs`: `65f1e5b0c0dafe414eb4dbb250aa613bd902cc2a5117bfc91b790043f06350f4`
- `apps/desktop/src-tauri/src/lib.rs`: `2eca0c36e1cbafdb007cfedc19ef24e4dce8db668de85abac6c6a53c4ca86a4b`
- `apps/desktop/src/App.tsx`: `255caa94bf29e4582f2e8fe3cd3ae31609b5396a8c6cfdf13fa8ce35a58e47e2`
- `apps/desktop/src/index.css`: `5bae5525199d92cda25261574f9db57ea0beab6d78c0bf4a4311eb2c2d42f636`
- Screenshot SHA-256 values: wide `428605b2cd6f625d1bdc5acfc9895b137b28a0a08aa04c1a7590fef32786ead9`; narrow `35f579f4c9bd383a8c0b1d2b06f77fb985784739d4790dc3e0ed8eb8c5575e50`; no-results `922209f05b6653a7c46315eb991f51048eaec6ff92f0095c8e6d17092963f57f`.

## Verification

- 306/306 normal core tests pass; the release-only incremental performance gate is intentionally ignored by the debug suite.
- 18 error-contract, 13 protocol, 1 public-API/trybuild, 11 unknown-YAML, 15 desktop Rust, 1 MCP, and 29 Vitest tests pass.
- `pnpm check`, contract/vector checks, Cargo formatting, locked all-target/all-feature Workspace Clippy with warnings denied, production web/Cargo build, optimized Tauri no-bundle build, bootstrap validation, and whitespace checks pass.
- The only build warning is the already documented informational `.app` identifier warning.
- The single checkpoint reviewer initially found post-cap status filtering and stale selection after scope changes; both were corrected and its narrow recheck passed with no blocker/high/medium finding.
