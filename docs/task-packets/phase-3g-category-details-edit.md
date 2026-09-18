# Phase 3G — Category details editing

## Objective

Allow a writable Workspace to edit a Category name and optional summary in one revision-checked, recoverable operation.

## Required reasoning level

Architect/xhigh for the optional-field span mutation; Worker/low is not appropriate because safe insertion/removal is a storage decision.

## Prerequisites

- Signed Category rename and archive/restore checkpoints.
- ADR 0011 lossless mutation rules and ADR 0014 content operations.

## Allowed files or subsystem

- `crates/rehow-core/src/domain/content/mutation.rs`
- `crates/rehow-core/src/application.rs`
- `apps/desktop/src-tauri/src/lib.rs`
- `apps/desktop/src/App.tsx`, `apps/desktop/src/index.css`, and focused tests
- This packet and `PROGRESS.md`

## Relevant specifications and ADRs

- `docs/content-model.md`
- `docs/architecture.md`
- `docs/threat-model.md`
- `docs/adr/0011-*`, `docs/adr/0014-*`

## Exact required behavior

- Add a typed `SetSummary(Option<String>)` Category mutation.
- Replace an existing summary, safely insert a missing summary, or remove it when empty.
- Save name and summary through one expected revision and one recoverable document update.
- Preserve ID, order, status, timestamps except `updated_at`, unknown metadata, directory path, and Guides.
- Expose refreshed summary data to the UI and provide a labeled details form.
- Valid no-ops preserve the revision.

## Important states and edge cases

- Summary absent/present/cleared; name-only and summary-only edits; both unchanged.
- Invalid name, invalid summary, stale revision, comments or presentation that make insertion/removal unsafe.
- Narrow-window primary actions retain a visible accessible label.

## Security constraints

- Use only the opened Workspace capability and existing recovery executor.
- Never reconstruct an entire descriptor or drop unknown metadata.
- Refuse ambiguous comment ownership, flow-style insertion/removal, or unproven round trips.

## Tests to add or update

- Domain insertion, replacement, removal, no-op, unknown metadata, and unsafe-comment cases.
- Application atomic details update, stale revision, invalid summary, and stable directory.
- Native payload/error serialization and existing UI helper/static-render coverage.

## Commands to run

- Focused Cargo and Vitest tests while editing.
- One final root `pnpm check && pnpm test && pnpm build` and full Cargo fmt/Clippy/test/Tauri no-bundle gate.

## Explicit non-goals

- Category reorder/removal, Guide moves, MCP mutation, external-change watching, or a new browser test dependency.

## Definition of done

- Required behavior and tests pass, UI is statically reviewed against current guidelines, documentation reflects verified state, and a signed checkpoint exists.

## Expected commit boundary

One feature commit, followed by a progress-only ledger commit if needed.

## Conditions requiring escalation

- The parser cannot prove safe optional-field insertion/removal, or the filesystem/recovery boundary must change.
