# Task Packet: Native Attachment removal

- **Objective:** Remove one identified Attachment from one Guide through exact native confirmation and a recoverable metadata-plus-trash operation.
- **Required reasoning level:** xhigh for storage implementation and final security review.
- **Prerequisites:** Accepted ADRs 0009, 0014, 0016, and 0017; native import checkpoint `2469a9a`.
- **Allowed files or subsystem:** ADR/task/progress docs; `crates/rehow-core`; desktop Tauri adapter and existing Guide UI/tests.
- **Relevant specifications and ADRs:** `docs/content-model.md` Attachment/revision sections; `docs/threat-model.md`; ADRs 0009, 0014, 0016, 0017.
- **Exact required behavior:** ID-only IPC; read-only streamed preview; exact effect-bound native confirmation; candidate/source revalidation; durable streamed before-image; metadata-first removal; held-inode isolation plus exact no-replace trash materialization; retained view refresh; per-Attachment Remove action.
- **Important states and edge cases:** cancel, missing/stale/archived Guide, missing Attachment ID/file, changed/hash/size/path/mode conflict, symlink/hardlink/special/oversized file, unsafe trash hierarchy, comment/round-trip refusal, interruption after either target, committed refresh failure.
- **Security constraints:** No renderer path/shell; descriptor confinement and no-follow; fixed-memory streaming; independently recoverable approved bytes; held-open inode writes cannot alter canonical trash; no recursive/permanent delete; no replacement; no stale approval; no content/path logging.
- **Tests to add or update:** Exact metadata removal/preservation/stale transition; preview/execution drift; hostile file/path types; two-target replay and conflict; native error/IPC shape; UI cancel/success/error/focus semantics.
- **Commands to run:** Focused Cargo/Vitest while editing; acceptance uses formatting, Workspace Clippy/tests, `pnpm check/test/build`, optimized native no-bundle build, bootstrap/contracts/diff checks.
- **Required reviews and artifacts:** One Security Reviewer/xhigh after focused tests pass. No screenshot gate: this adds a named action to the existing Attachment list without changing navigation or visual direction.
- **Explicit non-goals:** Permanent deletion, directory cleanup, rename, opening/preview/export, MIME inference, bulk removal, MCP/Git behavior.
- **Definition of done:** Required and hostile behaviors pass; security review has no blocker/high/medium finding; signed feature and progress checkpoints exist.
- **Expected commit boundary:** ADR/packet, core mutation/storage/application, native/UI wiring, and tests form one signed feature commit; verified progress follows.
- **Conditions requiring escalation:** Recovery would require permanent deletion or whole-file allocation, confirmation cannot bind the exact effect, or a dependency/schema migration becomes necessary.
