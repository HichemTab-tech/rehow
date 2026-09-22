# Phase 2C Markdown import preview evidence

Date: 2026-09-22

## Result

Ordinary Markdown selection now retains a native descriptor capability and produces a bounded, deterministic, read-only import preview. The renderer receives only escaped relative display labels, decimal byte counts, normalization flags, and aggregate counts; it receives no path, descriptor, or source bytes. Cancelling or selecting another folder revokes the retained source.

The scan accepts lowercase `.md` single-link regular files only, stays within four directory levels and 20,000 entries, caps accepted files at 10,000, caps each source below the Guide envelope, caps aggregate source bytes at 50 MiB, and reports malformed/unsafe, oversized, and non-Markdown entries. BOM and line-ending normalization is previewed without modifying source files. Symlinks, hardlinks, FIFOs, invalid UTF-8, controls, and empty files are rejected.

The Reference Desk onboarding state shows the bounded counts and first 100 inert labels, provides cancellation, and keeps destination selection visibly unavailable until the separate apply boundary is implemented.

## Verification

- `cargo test -p rehow-core storage::tests --locked`: 26 storage tests passed.
- `cargo test -p rehow-desktop markdown_import_preview_payload_is_closed_and_pathless --locked`: passed.
- `pnpm --filter @rehow/desktop test -- --run apps/desktop/src/app.test.ts`: 40 tests passed.
- `pnpm check`: passed with the installed Node 26.2.0 and pnpm 11.3.0; neither tool was installed or switched.
- `cargo clippy -p rehow-core -p rehow-desktop --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo fmt --all` followed by formatter and whitespace checks: passed.

Destination writes, Category mapping, Guide creation, retry, and partial-import recovery remain in the next apply checkpoint. The source was never modified in implementation or tests.
