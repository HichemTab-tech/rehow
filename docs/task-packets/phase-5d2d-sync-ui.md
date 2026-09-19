# Task Packet: Reference Desk sync UI

- **Objective:** Add a compact, accessible manual-sync and recovery surface over the accepted pathless native lifecycle.
- **Required reasoning level:** medium/high for async lifecycle and state presentation.
- **Prerequisites:** signed Phase 5D2C checkpoint `4f984fd`; ADR 0025; Reference Desk direction; current Web Interface Guidelines.
- **Allowed files or subsystem:** `apps/desktop/src/App.tsx`, `apps/desktop/src/index.css`, `apps/desktop/src/app.test.ts`, focused docs/evidence/progress; a narrow native correction only if recovery presentation requires it.
- **Relevant specifications and ADRs:** ADR 0025, `docs/design-direction.md`, `docs/threat-model.md`, `docs/testing-strategy.md`.
- **Exact required behavior:** A ready Workspace with an upstream exposes Sync; one compact panel shows checking/cancellation and the closed ancestry result; only a clean remote-fast-forward offers Apply; all other states explain the safe next step; apply/recovery consume closed payloads and install returned Workspace state; recovery handles commit or sync intent without guessing; focus and announcements are deterministic.
- **Important states and edge cases:** inspecting/cancelling/applying/recovering, invalid payload, cancellation race, declined native confirmation, up-to-date, local-ahead, dirty fast-forward, diverged, history-limit, ambiguous invocation failure, Workspace switch, conflicting forms, narrow windows.
- **Security constraints:** No renderer path, URL, ref, OID, Git output, credential, staging authority, merge/rebase/reset/checkout, push, scheduling, packed publication, or MCP authority. React treats all native data as bounded plain text and validates exact payload shapes.
- **Tests to add or update:** Closed payload guards; presentation copy/action policy for every ancestry/dirty state; recovery result guards; existing interaction helpers and production build.
- **Commands to run:** `pnpm --filter @rehow/desktop test`; `pnpm check`; `pnpm build`; Cargo formatting/check if native code changes; bootstrap and whitespace checks.
- **Explicit non-goals:** Push, authentication, automatic sync, conflict resolution, Git command output, repository details, broader shell redesign.
- **Definition of done:** Manual sync states are understandable and keyboard-accessible at wide/narrow sizes; only accepted native actions are reachable; tests/build/guideline review pass; docs match reality.
- **Expected commit boundary:** One UI checkpoint, then progress ledger.
- **Conditions requiring escalation:** Any need for renderer authority, credential handling, automatic conflict resolution, destructive Git action, or a native contract that cannot identify recovery safely.
