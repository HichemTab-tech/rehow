# Phase 4D external-change watching evidence

Date: 2026-09-19

## Accepted behavior

Rehow watches only retained, no-follow directory capabilities for the Workspace root and managed Category/Attachment directories. Linux watch paths are descriptor aliases under `/proc/self/fd`; watches are non-recursive, event paths never cross core or IPC, and every accepted event merely requests the existing argument-free canonical refresh. The refreshed view and disposable index replace retained state only after the complete confined scan succeeds.

Bursts are coalesced with a bounded one-slot wake channel. Watch failures become a visible degraded state while manual refresh and window-refocus reconciliation remain available. A clean renderer refreshes automatically. Any dirty form or active Workspace mutation defers the refresh and announces that files changed, without discarding edits or stealing focus. Workspace ID plus a native per-open session generation rejects stale events after switching or reopening.

## Dependency and boundary audit

`notify 8.2.0` is exact-pinned with default features disabled in `rehow-core`. The inspected source declares CC0-1.0 and Rust 1.77; resolved `notify-types 2.1.0` declares MIT OR Apache-2.0 and Rust 1.85. The Linux-only additions are `notify-types`, `inotify`, and `inotify-sys`; existing graph entries supply `mio` and `walkdir`. Rehow uses no shell, recursive caller path, event filename, content logging, or additional write authority.

Tests cover burst coalescing, ignored nested Git activity, newly created Category and Attachment watch sets, error priority, managed-directory symlink replacement refusal, the path-free three-field IPC payload, stale native-session rejection, and every dirty/busy deferral state.

## Design and accessibility review

The project-pinned `web-design-guidelines` skill and its current upstream rules were read on 2026-09-19. The change keeps the existing semantic refresh button, accessible name/tooltip, visible focus treatment, and polite live announcement; automatic refresh does not move focus. Status text is concise, truncates safely, and hides at narrow width while the named refresh action remains available. No new animation, color-only state, modal interruption, unsafe link/media, or nested-card treatment was introduced. Existing representative Reference Desk screenshots remain applicable to the unchanged shell geometry; the complete Phase 4 search-interaction screenshot matrix remains the Phase 4 gate.

## Verification

Final reviewed source identities:

- `crates/rehow-core/src/storage/watch.rs`: `2345005126fd179430fc0d47ff152c5525bec5f860f96c6452981b78d40d200d`
- `apps/desktop/src-tauri/src/lib.rs`: `22cb370aded77234d8b52165582d418cbe6ffd57da1cc831e01d30221ffca237`
- `apps/desktop/src/App.tsx`: `b06805cea808d28dbfc3bc538be4de0d3eb1a1e3d2205c00480caa01b7d86e87`

- `cargo test -p rehow-core storage::watch --all-features --locked`
- `cargo test -p rehow-desktop --all-features --locked`
- `pnpm --filter @rehow/desktop test`
- `pnpm check`
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- `pnpm test`
- `pnpm build`
- `pnpm tauri:build`
- `python3 scripts/check_bootstrap.py`
- `git diff --check -- . ':(exclude)x'`

The full pre-review run passed with 301 normal core tests and one intentional release-only performance test ignored, plus 18 error-contract, 13 protocol, 1 public-API/trybuild, 11 unknown-YAML, 13 desktop Rust, 1 MCP, and 24 Vitest tests. Production web, Cargo Workspace, and optimized Tauri no-bundle builds passed. The only build warning is the already documented informational `.app` identifier warning. Final post-review command results are recorded in `PROGRESS.md`.

The single Security Reviewer found and closed three acceptance issues: an in-flight edit/apply race, a post-swap coalescer lost wake, and incomplete/overbroad managed-directory selection. The corrected focused gate passes 7 watcher tests, 13 desktop Rust tests, and 25 Vitest tests; Workspace Clippy with warnings denied, formatting, type/contracts, bootstrap, and whitespace checks also pass. The final verdict has no blocker, high, or medium finding.
