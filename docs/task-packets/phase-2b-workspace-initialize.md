# Task Packet: Empty-folder Workspace initialization

- **Objective:** Create and immediately open a valid Rehow Workspace in a user-selected empty folder.
- **Required reasoning level:** high; first Workspace-creation authority.
- **Prerequisites:** accepted descriptor-confined inspection, canonical manifest emitter, recovery journal, and Reference Desk onboarding state.
- **Allowed files or subsystem:** core protocol/application/storage initialization path, desktop Tauri session command, onboarding UI/styles/tests, this packet, and progress.
- **Relevant specifications and ADRs:** `docs/content-model.md`, `docs/architecture.md`, `docs/threat-model.md`, ADR 0009, and ADR 0014.
- **Exact required behavior:** retain the native folder descriptor after inspection; accept only a bounded Workspace name; revalidate empty state under the writer lock; create a private `categories/` directory and canonical `rehow.yaml`; open and return the new Workspace; never send or accept a path in renderer IPC.
- **Important states and edge cases:** cancelled selection, empty and writable, non-writable, any folder change before Create, invalid name, busy writer, unsupported durability, interrupted initialization residue, and committed-but-refresh-failed.
- **Security constraints:** no renderer path/ID/time/operation/YAML authority; no overwrite or adoption; descriptor-relative no-follow operations; manifest installed no-replace with mode `0600`; directory mode `0700`; content-free errors; Git and MCP unchanged.
- **Tests to add or update:** empty-target retention, changed-folder refusal including an empty directory, symlink/special-entry refusal, canonical initialization/open, fail-closed interrupted residue, request/response shape, and onboarding state helpers.
- **Commands to run:** focused core/desktop/UI tests while iterating; one final format, Clippy, full test, production build, contract, bootstrap, and native Tauri build batch.
- **Explicit non-goals:** nonexistent-path creation, Markdown adoption/import, Git initialization, default Category creation, Guide creation, migration, recent Workspace persistence, and MCP mutation.
- **Definition of done:** an empty selected folder becomes a compatible open Workspace without overwriting any entry, all focused and final gates pass, and durable progress names the next action.
- **Expected commit boundary:** initialization core/Tauri/UI/tests, then progress.
- **Conditions requiring escalation:** safe descriptor retention cannot be maintained, a nonempty folder would need modification, or initialization cannot fail closed on ambiguous committed state.
