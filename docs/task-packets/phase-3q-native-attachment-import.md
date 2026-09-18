# Task Packet: Native Attachment import

- **Objective:** Import one native-selected regular file into one Guide as a recoverable managed Attachment.
- **Required reasoning level:** xhigh for storage implementation; xhigh security review.
- **Prerequisites:** ADRs 0009 and 0016; accepted managed Attachment inventory at `843cc51`.
- **Allowed files or subsystem:** ADR/task/progress docs; `crates/rehow-core`; desktop Tauri adapter and existing Guide UI/tests; generated contracts only when required.
- **Relevant specifications and ADRs:** `docs/content-model.md`, `docs/threat-model.md`, ADR 0009, ADR 0016.
- **Exact required behavior:** Native select and confirm; exact Guide revision; stable streamed source copy; core-generated ID/path/hash/size; collision-safe `0600` destination; one recoverable file-plus-metadata journal; refreshed retained view; accessible UI action and errors.
- **Important states and edge cases:** cancel, missing/stale/archived Guide, 256 Attachments, duplicate content, invalid/hostile filename, collision, symlink/hardlink/special/changed/oversized source, unsafe parents, interrupted journal, committed refresh failure.
- **Security constraints:** No renderer path, no shell, no symlink traversal, no whole-file allocation, no overwrite, no inferred execution, no tested assertion, no source path persistence/logging.
- **Tests to add or update:** Filename normalization/collision; metadata append/preservation/stale transition; source type/size/change; stream copy; atomic replay; IPC shape; UI cancel/success/error and accessible naming.
- **Commands to run:** Focused Cargo/Vitest during work; at acceptance run formatting, locked Workspace tests/Clippy, `pnpm check test build`, native no-bundle build, bootstrap/contract/diff checks.
- **Required reviews and artifacts:** One Security Reviewer/xhigh after focused tests pass. No screenshot gate because this adds an existing-toolbar action and dialog, not a new visual direction.
- **Explicit non-goals:** Removal, rename, preview/open, MIME inference, drag/drop, directory import, MCP mutation, Git behavior.
- **Definition of done:** Required behavior and hostile tests pass; review has no blocker/high/medium finding; docs reflect verified state; signed coherent checkpoint exists.
- **Expected commit boundary:** ADR, core storage/domain/application, native/UI wiring, tests, and generated contracts form one feature commit; verified progress may follow.
- **Conditions requiring escalation:** A required recovery invariant cannot be expressed by the existing journal, filesystem lacks required atomic primitives, or product behavior would require renderer path authority.
