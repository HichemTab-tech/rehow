# Phase 3M — Recoverable Guide Deletion

- **Objective:** Delete one Attachment-free Guide by moving its exact current file to Rehow Trash after explicit user confirmation.
- **Required reasoning level:** high; one Security Reviewer/xhigh at the combined storage checkpoint before acceptance.
- **Prerequisites:** Signed checkpoint `2b49223`; ADR 0009 trash replay; retained Workspace capability and Guide revisions.
- **Allowed files or subsystem:** Core storage/application/tests, desktop Tauri/React/tests/styles, this packet, and `PROGRESS.md`.
- **Relevant specifications and ADRs:** `docs/content-model.md` Guide/Attachment/concurrency sections; ADR 0009; `docs/threat-model.md`; `docs/design-direction.md`.
- **Exact required behavior:** Locate by Guide ID; require the current revision; refuse Guides with Attachment metadata; move the exact confined Guide file to `.rehow/trash/<operation-id>/<guide-path>` through one durable delete journal; refresh and prove the Guide ID absent. The UI requires a named confirmation and returns focus/selection to the remaining library.
- **Important states and edge cases:** Missing Guide, stale revision, archived/tested/draft Guide, Attachment metadata, unsafe/symlink/hardlink target, busy writer, interrupted trash move, committed-refresh failure, empty remaining library, read-only Workspace, and native/UI errors.
- **Security constraints:** No renderer path, recursive deletion, Attachment deletion, arbitrary target, shell, Git, or MCP mutation. Never delete an Attachment directory or unlisted file. Exact before-image bytes remain recoverable in operation-bound trash.
- **Tests to add or update:** Storage trash/replay/conflict; application successful deletion plus stale/Attachment refusal; native bounded identifiers/stable errors; UI confirmation/result/error state and selection normalization where practical.
- **Commands to run:** Focused Cargo/Vitest/type checks, then one full pnpm/Cargo/Clippy/build/Tauri/bootstrap/whitespace gate.
- **Required reviews and artifacts:** Static current web-interface-guideline review; one Security Reviewer/xhigh over the combined Guide move/delete storage boundary before acceptance.
- **Explicit non-goals:** Attachment removal, recursive directory cleanup, permanent erase, restore UI, bulk delete, filename tidy, Git, or MCP mutation.
- **Definition of done:** Confirmed deletion is revision-checked, confined, recoverable, nonrecursive, Attachment-safe, accessible, fully gated, security-reviewed, and represented by signed feature/progress checkpoints.
- **Expected commit boundary:** One signed feature commit and one signed progress-only commit; the Orchestrator owns both.
- **Conditions requiring escalation:** Any need to remove an Attachment or directory, accept a path from IPC, bypass revision/confirmation, add a dependency, or weaken ADR 0009 replay.
