# Phase 5B3B3 native clone validation evidence

Date: 2026-09-19

## Boundary

- Renderer input is one validated `PlanToken`; planning remains URL-in/path-out-free.
- Native confirmation alone receives the retained secret-free URL.
- The asynchronous adapter moves the empty-folder capability and plan into one blocking worker and retains the resulting opaque `GitCloneValidation` natively.
- One native lifecycle lock atomically binds the pending plan, selected-folder capability, active cancellation identity, and validated authority. Folder selection cannot change across confirmation or active validation.
- Cancellation sets one native-held atomic flag. No path, process ID, descriptor, URL, ref, OID, filename, content, stderr, or credential crosses IPC.
- Cancelling onboarding revokes pending or validated clone authority; a cancellation racing with successful validation cannot republish it.
- Core revalidates the selected destination after success and after every pipeline failure. Only a still-proven-empty selection is restored for retry.
- This slice publishes no Workspace file and installs no Git config or ref in the destination.

## Focused evidence

- Core orchestration rejects SSH, pre-cancellation, and destination drift without writes.
- Native serialization tests prove closed plan/apply/cancel payloads and content-free errors.
- Native tests prove mismatched tokens are not consumed, matching tokens are single-use, cancellation is pathless/idempotent, and the opaque validation session is `Send` for the blocking boundary.
- Independent security review found and verified corrections for selection-transition atomicity and cancellation revocation at successful completion.
- Existing Phase 5B3B2 tests cover real isolated Git tree/blob validation, process-group cancellation, output limits, staging limits/types, malformed framing, exact commit type, independently recomputed object IDs, canonical Workspace parsing, and explicit public-HTTPS sandbox smoke behavior.

## Commands

Checkpoint command results are recorded in `PROGRESS.md`. No Node or pnpm installation or version switch is part of this slice.
