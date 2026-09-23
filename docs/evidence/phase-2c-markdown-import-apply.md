# Phase 2C Markdown import apply evidence

Date: 2026-09-23

## Result

Rehow can now copy a reviewed ordinary Markdown collection into a newly created Workspace and Category. Native code retains both folder capabilities; renderer IPC carries no filesystem path or source bytes. Apply repeats the bounded source scan, verifies exact identity and content, retains normalized bodies before destination mutation, rechecks that the destination is outside the source hierarchy, and creates only draft Guides. Source frontmatter remains inert Guide body text and source bytes are unchanged.

Workspace initialization now journals the private `categories/` scaffold and manifest as one recoverable plan. A per-operation marker distinguishes replay from a foreign-directory race, then is atomically retired into recovery trash with post-move verification; a concurrent substitute is preserved and reported. Tests cover interruption after directory publication, same-user foreign-directory insertion, marker substitution, destination changes, destination movement beneath the source during preparation, source replacement by symlink/hardlink/socket, content drift, draft-only trust, hostile filename controls, and an exact valid prefix after a later Guide failure.

The desktop flow provides native destination selection, labeled Workspace/Category inputs, inline validation, busy/error/partial/success announcements, cancellation, and a bounded dense preview. A native generation-owned lifecycle blocks overlapping imports, Workspace switching, cancellation, and stale completion from replacing a newer session.

## Verification

- `cargo test --workspace --all-features`: core passed 476 library tests with 2 intentional network/performance ignores; error-contract, protocol, public-API/trybuild, unknown-YAML, desktop, MCP, integration, and documentation suites passed. Three trybuild baselines were refreshed only for semantically unchanged Rust 1.98.1 diagnostic qualification.
- `cargo test -p rehow-desktop`: 28 tests passed.
- `pnpm --filter @rehow/desktop test -- --run`: 41 tests passed.
- `pnpm check`: passed with installed Node 26.2.0 and pnpm 11.20.0; neither tool was installed or switched.
- `pnpm build`: contracts, TypeScript/Vite, and all Cargo Workspace artifacts passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed.
- `cargo fmt --all --check` and `git diff --check`: passed.

The final Security Reviewer verdict is PASS with no remaining actionable finding in this boundary. `PROGRESS.md` records the accepted checkpoint.
