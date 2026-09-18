# Task Packet: Phase 2A Reference Desk shell

- **Objective:** Replace the inert smoke page with the approved responsive Reference Desk application shell and prove the main search/read interaction visually.
- **Required reasoning level:** high for interaction, responsive behavior, and accessibility; implementation remains single-agent.
- **Prerequisites:** `docs/design-direction.md`, the current upstream Web Interface Guidelines, and the existing React/Vite/Tailwind shell.
- **Allowed files or subsystem:** `apps/desktop/src/`, `apps/desktop/index.html`, focused tests, screenshots, and evidence/progress documents.
- **Relevant specifications and ADRs:** product/design specifications; no storage or IPC authority is added.
- **Exact required behavior:** wide three-pane layout, medium two-pane layout, narrow result/reader stack, incremental fixture search, selectable results, command copy with polite announcement, `Ctrl/Cmd+K` and `/` search focus, dark/light system theme, and explicit empty results.
- **Important states and edge cases:** long commands scroll locally, long result text truncates, no-results recovery, narrow back navigation/focus restoration, copy failure guidance, and 200% text resilience.
- **Security constraints:** fixture content only; no HTML injection, command execution, filesystem access, Tauri IPC, external links, or logging.
- **Tests to add or update:** search terms/empty behavior, TypeScript, Biome, production build, wide/narrow screenshots, current-guideline source audit.
- **Commands to run:** desktop Biome/typecheck/Vitest/build; root `pnpm check`; native Tauri no-bundle build when accepting the shell.
- **Explicit non-goals:** real Workspace data, editor/save actions, routing persistence, production search ranking, dialogs, Git behavior, and final empty/error/conflict screens.
- **Definition of done:** checks pass, screenshots are inspected, current design guideline findings are corrected or recorded, and the shell is a signed recoverable checkpoint.
- **Expected commit boundary:** one UI/evidence commit and one progress-ledger commit.
- **Conditions requiring escalation:** new dependencies, renderer filesystem authority, product navigation conflict, or an accessibility problem without a safe correction.
