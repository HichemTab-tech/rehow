# Phase 3D — Guide Details Editing

## Objective

Edit a Guide title and summary atomically through the existing revision-checked recovery boundary.

## Required reasoning level

Orchestrator/high; no extra agent for this bounded slice.

## Prerequisites

Signed Guide-create checkpoint `94d2f4e`; accepted Guide mutation planner; retained native Workspace session.

## Allowed files or subsystem

Core application tests, desktop Tauri/React/CSS/tests, this packet, and `PROGRESS.md`.

## Relevant specifications and ADRs

Guide mutation sections of `docs/content-model.md`, ADR 0009, ADR 0014, threat model, and Reference Desk design direction.

## Exact required behavior

- Apply title and summary as one revision-checked document update; preserve body, unknown metadata, and filename.
- A no-op preserves bytes/revision; an actual edit updates time and changes tested content to stale under ADR 0014.
- Accept only Guide ID, expected revision, title, and summary over bounded native IPC.
- Provide labeled inline fields, focused errors, discard protection, and refreshed reader/search state.

## Important states and edge cases

Invalid fields, no-op, stale revision, external edit, malformed/read-only Guide, unsafe round trip, writer contention, interrupted/committed write, tested-to-stale, and narrow layout.

## Security constraints

No generic YAML/path/status/time/operation authority. Never overwrite a stale revision. Preserve unknown metadata and do not rename files.

## Tests to add or update

Core atomic metadata/no-op/body/unknown preservation; native stable errors; UI normalization/static semantics.

## Commands to run

Focused Cargo/Vitest/type checks, then one full `pnpm check/test/build`, Workspace Clippy, Tauri build, bootstrap, and whitespace gate.

## Explicit non-goals

Tags, aliases, Category change, filename tidy, tested approval, archive/delete, Git, MCP mutation, and dependencies.

## Definition of done

Details save atomically with conflict safety and refreshed UI; applicable gates pass; signed checkpoint and durable restart state exist.

## Expected commit boundary

One signed feature commit and one signed progress-only commit.

## Conditions requiring escalation

Any need for generic mutation, partial writes, status elevation, dependency addition, or a new storage policy.
