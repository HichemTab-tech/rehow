# Phase 3F — Category Archive & Restore

## Objective

Archive and restore Categories without moving or deleting their Guides or directories.

## Required reasoning level

Orchestrator/high; no extra agent for this bounded slice.

## Prerequisites

Signed Category-rename checkpoint `959a239`; typed Category status planner; revision-checked recovery executor.

## Allowed files or subsystem

Core application tests, desktop Tauri/React/CSS/tests, this packet, and `PROGRESS.md`.

## Relevant specifications and ADRs

Category lifecycle/default-Category rules in `docs/content-model.md`, ADR 0009, ADR 0014, threat model, and Reference Desk design direction.

## Exact required behavior

- Apply typed Archive/Restore to the expected descriptor revision; preserve name, summary, order, unknown metadata, directory, and Guides.
- Refuse archiving `settings.default_category_id` until the user chooses a replacement.
- Archived Categories refuse new Guides; restored Categories accept them.
- Refresh native state and show explicit Archive/Restore controls, busy state, status, and focused actionable errors.

## Important states and edge cases

No-op, stale revision, missing/default Category, malformed/read-only descriptor, unsafe round trip, contention, interrupted/committed write, existing Guides, and narrow rail.

## Security constraints

No move/delete, generic YAML/path/status string, renderer time/operation authority, Git, or MCP write. Boolean intent maps to a closed Rust transition.

## Tests to add or update

Core archive/restore/content/path/default/create-block tests; native stable error tests; UI status control/static semantics.

## Commands to run

Focused Cargo/Vitest/type checks, then one full pnpm/Cargo/Clippy/build/Tauri/bootstrap/whitespace gate.

## Explicit non-goals

Default replacement, reorder, removal, Guide archive, Category summary editing, Git, MCP mutation, or dependencies.

## Definition of done

Archive/restore is reversible and revision-safe, content remains in place, applicable gates pass, and signed durable checkpoints exist.

## Expected commit boundary

One signed feature commit and one signed progress-only commit.

## Conditions requiring escalation

Any need to move/delete content, silently clear the default, add generic mutation, or introduce new storage semantics/dependencies.
