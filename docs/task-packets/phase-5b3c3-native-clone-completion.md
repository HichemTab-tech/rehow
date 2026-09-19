# Task Packet: Native validated-clone completion

- **Objective:** Connect the accepted core clone finalizer to the native-held validated session through one pathless, single-use Tauri command and open the completed Workspace.
- **Required reasoning level:** high.
- **Prerequisites:** Signed core finalization checkpoint `e9d5e58`; ADR 0023; native clone lifecycle from `2812c49`.
- **Allowed files or subsystem:** Core Git visibility only, desktop native lifecycle/commands/tests, this packet, focused evidence, ADR/progress bookkeeping. No React, MCP, SSH, or credential changes.
- **Relevant specifications and ADRs:** ADR 0023; Phase 5B3B3/C1/C2 packets and evidence; architecture, threat model, and testing strategy Git/native boundaries.
- **Exact required behavior:** A pathless command consumes the one opaque validated session; presents a native confirmation bound to the already validated secret-free URL; rejects missing, competing, or duplicate completion; performs core finalization off the UI thread; on success installs the returned Workspace as the active native session with watcher and correct Git facts; on decline or recoverable failure restores the exact validated session for retry; and exposes only closed bounded error codes/messages.
- **Important states and edge cases:** Missing validation, validation in progress, completion in progress, duplicate invocation, completion success, recoverable failure/retry, panic containment, folder-change/cancel attempts during completion, watcher degradation, and Workspace-session lock failure.
- **Security constraints:** Renderer supplies no path, URL, OID, ref, plan hash, or content. A second native confirmation is required because the earlier validation confirmation promised no destination mutation. Completion is cancellable while that dialog is open but not after writes begin. Selection and competing operations cannot revoke or replace the held capability during finalization. Errors expose no path, URL, object ID, Git output, or content.
- **Tests to add or update:** Closed materialization error mapping; finalization cancellation refusal; native command registration/type checking; full desktop Rust tests; strict core/desktop Clippy.
- **Commands to run:** Cargo formatting; desktop tests; core/desktop all-target/all-feature Clippy with warnings denied; relevant full test gate; bootstrap and whitespace checks.
- **Explicit non-goals:** React onboarding controls, progress UI, SSH, credential helpers, commit/pull/push/sync, staging cleanup, or MCP.
- **Definition of done:** The native process can safely turn its held validated session into the active Workspace without renderer authority, retryable failure loses no normal in-memory session, gates pass, and a signed checkpoint exists.
- **Expected commit boundary:** Core visibility plus one native completion command, lifecycle rules, tests, packet/evidence, and progress checkpoint.
- **Conditions requiring escalation:** Need to expose native paths/URLs/OIDs, cancel after publication begins, overwrite unknown destination state, or silently discard recoverable validated authority.
