# Phase 3K — Guide Tags & Aliases

## Objective

Edit a Guide's Tags and aliases atomically with its title/summary through one revision-checked recoverable write.

## Required reasoning level

Orchestrator/high; no extra agent for this bounded slice.

## Prerequisites

Signed Guide lifecycle checkpoint `34f7fe9`; ADR 0014's typed mutation and tested-to-stale rules; accepted recovery update executor.

## Allowed files or subsystem

ADR 0014's narrow required-string-sequence correction; Guide mutation/application tests; desktop Tauri/React/CSS/tests; this packet; and `PROGRESS.md`.

## Relevant specifications and ADRs

Guide metadata/status rules in `docs/content-model.md`; ADR 0009; ADR 0014; `docs/threat-model.md`; and `docs/design-direction.md`.

## Exact required behavior

- Replace ordered Tags and aliases with independently bounded lists: at most 64 items; Tags at most 64 Unicode scalars each; aliases at most 200.
- Trim UI lines, discard blank lines, retain first-occurrence order, and reject duplicates under the core's full Unicode case-fold comparison; the core revalidates every value.
- Apply title, summary, Tags, and aliases against one expected revision and one recovery operation. A failure changes nothing.
- Preserve path, body, Category, verification pair, archive time, Attachments, nested fields, and unknown metadata. Any actual change stales tested or archived-from-tested state under ADR 0014.
- Required empty collections remain `[]`; nonempty edited collections become canonical block sequences. Existing nonempty flow collections remain read-only.
- Native IPC accepts only bounded strings/lists and retained Guide identity/revision. The Reference Desk presents labeled multiline inputs, limits/help, busy/error/live states, and predictable focus restoration.

## Important states and edge cases

No-op, empty lists, exact limits/one-over, whitespace-only entries, case-fold duplicates, distinct normalization forms, hostile YAML characters, canonical empty flow, nonempty flow refusal, owned comments, tested/archived Guides, stale revision, malformed/read-only Guide, contention, and committed-refresh failure.

## Security constraints

No caller YAML, paths, timestamps, operation IDs, status authority, shell/Git/MCP writes, or HTML rendering. All YAML is emitted by the core; unsafe presentation fails closed without partial writes.

## Tests to add or update

Planner validation/presentation/no-op/stale/preservation tests; application atomicity/revision/content tests; native payload/error bounds; UI normalization and explicit form-label tests supported by the current harness.

## Commands to run

Focused Cargo/Vitest/type checks, then one full pnpm/Cargo/Clippy/build/Tauri/bootstrap/whitespace gate.

## Required reviews and artifacts

Fresh current web-interface-guideline static review of the changed form. Screenshots/browser interaction remain in the documented combined UI gate.

## Explicit non-goals

Tag taxonomy/renaming, autocomplete, Guide move/path rename/delete, marking tested, Attachments, Git, MCP mutation, dependencies, or a new browser test harness.

## Definition of done

All four detail fields commit atomically; Tag/Alias constraints and unsafe-presentation refusal are proven; required gates pass; and signed feature/progress checkpoints exist.

## Expected commit boundary

One signed feature commit and one signed progress-only commit; the Orchestrator owns both.

## Conditions requiring escalation

Any need for generic YAML/status/path authority, weaker duplicate semantics, unbounded IPC, new dependencies, or storage behavior outside the accepted update executor.
