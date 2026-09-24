# Task Packet: Git and MCP onboarding polish

- **Objective:** Make missing Git identity, existing remotes, and MCP client setup understandable and safely actionable from the desktop app.
- **Required reasoning level:** xhigh for Git configuration safety; high for UI integration.
- **Prerequisites:** ADRs 0022, 0024, 0026–0049, accepted pathless renderer boundary, packaged `/usr/bin/rehow-mcp`.
- **Allowed files or subsystem:** `crates/rehow-core/src/git.rs`, `crates/rehow-core/src/git/https_credentials.rs`, `apps/desktop/src-tauri/src/lib.rs`, `apps/desktop/src/{App.tsx,app.test.ts,index.css}`, ADR/documentation/progress files for this change.
- **Relevant specifications and ADRs:** ADRs 0022, 0024, 0061, 0062; `docs/mcp-setup.md`; `docs/threat-model.md`; `docs/testing-strategy.md`.
- **Exact required behavior:** Report closed identity state; allow absent-only repository-local identity setup from manual input or safe explicit global import; never expose identity values to renderer IPC; label remotes from the authoritative count; reject duplicate remote names without overwrite; generate Codex and ChatGPT tunnel commands that prompt for path/tunnel data and expose least-authority profiles accurately.
- **Important states and edge cases:** Missing/complete/partial/duplicate/invalid identity; absent/unsafe/changing global config; stale config plans and recovery lock; zero/one/many remotes; duplicate name; invalid MCP alias; clipboard failure; read-only/broad/Git profiles.
- **Security constraints:** No global config mutation or include acceptance, renderer path/identity disclosure, remote replacement, client config editing, tunnel creation, API-key handling, or credential storage. Generated shell commands must never require unescaped path substitution. Retain atomic config and native-confirmation boundaries.
- **Tests to add or update:** Safe global provenance/redaction; atomic local identity setup; partial/stale refusal; closed native/UI payloads; state-aware remote labels; least-authority MCP template generation; desktop typecheck/build and Rust checks.
- **Commands to run:** Focused Vitest/core tests; `pnpm check`; `pnpm test`; `pnpm build`; Cargo format, Clippy, and tests.
- **Required reviews and artifacts:** Current web-interface guideline review; security review of config provenance, IPC redaction, and MCP templates. Screenshot is required only if the Settings geometry cannot be inspected in the normal graphical smoke pass.
- **Explicit non-goals:** Editing an existing remote URL; replacing local identity; using conditional/system Git identity; configuring clients automatically; managing `tunnel-client`; exposing a public MCP endpoint.
- **Definition of done:** All behavior above is implemented, closed-schema tests pass, docs match reality, and `PROGRESS.md` records exact verification and next action.
- **Expected commit boundary:** One coherent Git/MCP onboarding fix owned by the Orchestrator.
- **Conditions requiring escalation:** A need to disclose the Workspace path to the renderer, replace existing Git configuration, store credentials, or introduce a new dependency/service.
