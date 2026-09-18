# Phase 3L — Guide Category Move

## Objective

Change a Guide's Category by atomically creating the rewritten Guide in the destination Category and removing the original through one recoverable journal.

## Required reasoning level

Orchestrator/high; no extra agent for this bounded slice.

## Prerequisites

Signed Guide organization checkpoint `f315f1b`; typed `ChangeCategory`; accepted mixed-target recovery journals; descriptor-confined managed targets.

## Allowed files or subsystem

Core storage recovery/application/tests, desktop Tauri/React/tests/styles, this packet, and `PROGRESS.md`.

## Relevant specifications and ADRs

Guide path/Category/revision rules in `docs/content-model.md`; ADR 0009; ADR 0014; `docs/threat-model.md`; and `docs/design-direction.md`.

## Exact required behavior

- A same-Category request follows the ordinary atomic details update and does not move the file.
- A changed Category must exist and be active. Rust derives the destination directory and preserves the exact filename; renderer input never contains a path.
- Under one writer lock, reread the source at its expected path/revision, require destination absence, apply typed Category plus title/summary/Tag/Alias mutations, and create one `MultiFile` journal.
- Journal order is destination `Absent -> Object` first, source `Existing -> Absent` second, so interruption never removes the only durable Guide copy. Before-images remain recoverable through the accepted quarantine cleanup.
- Refresh must prove exactly one Guide ID in the requested destination Category and return its new path-derived revision.
- Preserve body, stable ID, verification pair, Attachments, nested fields, unknown metadata, filename, and Attachment directory. Tested or archived-from-tested state becomes stale.
- Reference Desk adds an active-Category select to Guide details with busy/error/live/dirty behavior.

## Important states and edge cases

Same destination, inactive/missing destination, stale source, occupied destination filename, unsafe source/destination, case-sensitive filename, tested/archived status, invalid details, contention, interruption after destination creation or source removal, committed-refresh failure, and external changes.

## Security constraints

No renderer path/filename/YAML/time/operation authority; no overwrite; no recursive move; no Attachment move; no arbitrary filesystem, shell, Git, or MCP authority. Every target remains beneath the retained Workspace capability with no-follow traversal.

## Tests to add or update

Storage create+delete journal/replay/conflict tests; application active/inactive/same/stale/collision/path/body/unknown/status tests; native bounded ID/error tests; UI Category normalization/dirty/select semantics tests.

## Commands to run

Focused Cargo/Vitest/type checks, then one full pnpm/Cargo/Clippy/build/Tauri/bootstrap/whitespace gate.

## Required reviews and artifacts

Fresh current web-interface-guideline static review of the changed form. Security review is combined at the next major filesystem checkpoint unless a new unresolved authority appears.

## Explicit non-goals

Filename tidy/rename, Category creation in the form, Guide deletion, Attachment relocation, marking tested, Git, MCP mutation, dependencies, or automatic conflict resolution.

## Definition of done

Cross-Category move is confined, no-overwrite, revision-checked, interruption-recoverable, content-preserving, accessible, fully gated, and represented by signed feature/progress checkpoints.

## Expected commit boundary

One signed feature commit and one signed progress-only commit; the Orchestrator owns both.

## Conditions requiring escalation

Any need for renderer paths, overwrite, recursive filesystem behavior, Attachment path change, new journal semantics, dependency changes, or ambiguous recovery state.
