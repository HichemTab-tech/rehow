# Phase 3N — Manual Workspace Refresh

- **Objective:** Reload the open Workspace through its retained native capability so external file changes become visible without restarting Rehow.
- **Required reasoning level:** high; no extra agent for this read-only bounded slice.
- **Prerequisites:** Signed checkpoint `a767ccf`; retained `WorkspaceView` capability; existing bounded scan, issue isolation, revision calculation, and dirty-editor prompts.
- **Allowed files or subsystem:** Core application/tests, desktop Tauri/React/tests/styles, this packet, and `PROGRESS.md`.
- **Relevant specifications and ADRs:** `docs/architecture.md` external-change/session sections; `docs/content-model.md` revision/concurrency sections; `docs/testing-strategy.md`; `docs/design-direction.md`.
- **Exact required behavior:** A pathless native command duplicates the retained root capability and performs the same bounded compatible-Workspace scan used at open. Success atomically replaces the in-memory view and returns a fresh Workspace payload. The UI exposes a named toolbar refresh control, confirms before discarding dirty Guide/body/detail/Category/create drafts, preserves the selected Guide ID when it still exists, otherwise selects the first remaining Guide, reloads its body, and announces Guide/issue counts or removal.
- **Important states and edge cases:** No Workspace, clean/dirty forms, busy mutation, selected Guide changed/removed, Guide added, malformed Guide isolation, empty library, root reclassified/missing, permission/unsafe/resource/changed-during-read errors, repeated refresh, narrow window, and keyboard focus.
- **Security constraints:** No renderer path, selection handle, filesystem mutation, recovery write, shell, Git, MCP, or watcher authority. Use only the retained root capability; hostile/malformed files remain isolated by the existing bounded scanner.
- **Tests to add or update:** Core refresh after external add/change/remove and malformed isolation; native stable content-free errors and payload; UI selection preservation/fallback plus named control/dirty confirmation behavior where practical.
- **Commands to run:** Focused Cargo/Vitest/type checks, then one full pnpm/Cargo/Clippy/build/Tauri/bootstrap/whitespace gate.
- **Required reviews and artifacts:** Fresh current web-interface-guideline static review of the toolbar control and refresh/error/focus states.
- **Explicit non-goals:** Automatic watcher, polling schedule, conflict merge UI, recovery execution, Workspace fact reprobe, opening a new path, search index persistence, Git, or MCP mutation.
- **Definition of done:** External content changes are safely visible on demand; dirty work is never silently discarded; selection/focus/errors are predictable; all gates pass; signed feature/progress checkpoints exist.
- **Expected commit boundary:** One signed feature commit and one signed progress-only commit; the Orchestrator owns both.
- **Conditions requiring escalation:** Any need for a renderer path, silent dirty-state loss, filesystem mutation, watcher dependency, recovery side effect, or expanded compatibility policy.
