# Task Packet: Actionable Workspace issues

- **Objective:** Turn the existing Workspace issue count into bounded, specific, safe recovery guidance.
- **Required reasoning level:** high for disclosure/recovery policy; low for frozen payload and UI wiring.
- **Prerequisites:** Existing confined scanner issue taxonomy, pathless Workspace session, manual refresh, ADR 0056.
- **Allowed files or subsystem:** desktop native payload, `App.tsx`, `app.test.ts`, `index.css`, and aligned docs/progress. No storage mutation or dependency change.
- **Relevant specifications and ADRs:** product validation/recovery requirements, architecture/threat model, ADRs 0009 and 0056.
- **Exact required behavior:** Return complete issue count, at most 100 closed issue records, and exact truncation state; validate them strictly; show escaped relative locations, issue-specific explanation, safe next steps, and refresh/close actions with focus restoration.
- **Important states and edge cases:** zero issues, one/many issues, truncated list, hostile display text, Workspace replacement, issue disappearance after refresh, narrow window, keyboard dismissal.
- **Security constraints:** No absolute path, file bytes, automatic repair/delete, generic path input, opener, shell, mutation, or MCP expansion. Unknown kinds and unsafe display scalars fail closed.
- **Tests to add or update:** Native closed payload mapping; strict renderer schema/truncation validation; inert recovery-panel rendering; focused type/style/UI tests.
- **Commands to run:** Focused desktop Rust/Vitest/type/Biome checks while editing; one combined repository gate before acceptance.
- **Required reviews and artifacts:** Current Web Interface Guidelines static review and recorded evidence. No specialist unless a new security-sensitive mutation is introduced.
- **Explicit non-goals:** Automatic journal replay, cleanup, file editing, opening the Workspace folder, migration, or changing scan classifications.
- **Definition of done:** Every reported issue has a bounded safe location and actionable explanation, ambiguous bytes remain untouched, validation/focus/narrow behavior pass, full gates pass, and a signed checkpoint exists.
- **Expected commit boundary:** Payload, panel, styles/tests, ADR/packet/evidence, then progress ledger.
- **Conditions requiring escalation:** Any need to infer user intent, delete/move/overwrite content, expose an absolute path, accept a renderer path, or add a dependency.
