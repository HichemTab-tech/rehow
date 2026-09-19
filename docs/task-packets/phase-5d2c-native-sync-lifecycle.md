# Task Packet: Native sync lifecycle

- **Objective:** Expose accepted sync inspection, classification, confirmed remote-fast-forward apply, cancellation, and recovery through pathless native commands while retaining every authority in Rust.
- **Required reasoning level:** high for lifecycle/concurrency and native confirmation.
- **Prerequisites:** signed Phase 5D2B2 checkpoint `983b3da`; ADR 0025; existing clone and commit native lifecycle patterns.
- **Allowed files or subsystem:** `crates/rehow-core/src/git.rs`, `apps/desktop/src-tauri/src/lib.rs`, focused tests/evidence/ADR/progress. No React UI in this packet.
- **Relevant specifications and ADRs:** ADRs 0023–0025, `docs/threat-model.md`, `docs/testing-strategy.md`.
- **Exact required behavior:** A pathless command plans, anonymously fetches, validates, and classifies using the open Workspace; returns only bounded redacted state and an opaque single-use token; supports cancellation; applies only a clean proven loose-branch remote fast-forward after native confirmation; replaces the retained Workspace view only after core success; exposes explicit pathless recovery for incomplete sync intent.
- **Important states and edge cases:** Busy/pending/active/classified/applying, cancellation races, up-to-date/local-ahead/diverged/history-limit/dirty, stale tokens, declined confirmation, ambiguous process exit, recovery-required, retained watcher rebuild, Workspace close/change.
- **Security constraints:** Renderer supplies no path, URL, ref, OID, Git output, credential, or filesystem authority. No push, auth, shell, merge/rebase/reset/checkout, packed publication, scheduling, or MCP. Confirmation text is derived only from bounded validated labels.
- **Tests to add or update:** Closed payload/error mapping; lifecycle token/busy/cancellation helpers; confirmation copy; recovery and Workspace replacement behavior where deterministic; existing native/core suites.
- **Commands to run:** Focused desktop/core tests; Cargo formatting; strict all-target/all-feature Clippy; applicable broad Workspace tests; bootstrap and whitespace checks.
- **Explicit non-goals:** React UI, push, authenticated transport, packed branches, schedule/start sync, remote-tracking refs, reflogs, MCP.
- **Definition of done:** Commands compile, authority cannot be forged through IPC, cancellation/decline/recovery are explicit, errors are stable/content-free, tests and checks pass, docs match reality.
- **Expected commit boundary:** One core/native checkpoint, then progress ledger.
- **Conditions requiring escalation:** Any renderer path/URL/ref/OID input, credential handling, destructive Git action, silent conflict resolution, or inability to retain/revoke authority deterministically.
