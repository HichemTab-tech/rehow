# Phase 5D2C native sync lifecycle evidence

Date: 2026-09-19

## Accepted boundary

- The renderer receives only a single-use `PlanToken`, validated remote name/transport/host labels, a closed ancestry value, and two booleans. It supplies no path, URL, ref, object ID, Git output, credential, or filesystem authority.
- One native lifecycle owns `active`, `classified`, and `applying` states. A new inspection revokes any prior classification; mismatched tokens do not consume authority; cancellation revokes active or retained authority; confirmed publication cannot be cancelled mid-write.
- Inspection performs the accepted private anonymous-HTTPS fetch, complete Workspace validation, and bounded ancestry proof while the open Workspace capability is retained in Rust.
- Apply accepts only a clean proven `remoteFastForward`, requires an OS-native confirmation, consumes the opaque classification once, calls the durable core publication boundary, replaces the retained Workspace view only after success, and rebuilds the watcher.
- Recovery is pathless, requires a pending durable sync intent plus OS-native confirmation, resumes only the accepted exact-state core state machine, then replaces the view and watcher.
- Workspace switching is refused during active/applying sync and clears a retained classification otherwise. Git status reports either commit or sync recovery as `recoveryRequired`.
- No push, credentials, shell, merge, rebase, reset, checkout, packed-branch publication, scheduling, React UI, or MCP authority was added.

## Verification

- `cargo test -p rehow-desktop --lib`: 24 passed.
- `cargo test -p rehow-core sync_`: 18 passed; covers planning/redaction, incoming validation, ancestry, publication, every Git-metadata interruption seam, conflict preservation, packed-only refusal, and recovery codecs.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed.
- `cargo fmt --all --check`: passed after formatting.
- `cargo test --workspace --all-features`: 412 core tests passed with 2 intentional ignores; 18 error-contract, 13 protocol, public-API/trybuild, 11 unknown-YAML, 24 desktop, MCP, and doc tests passed.
- `python3 scripts/check_bootstrap.py` and `git diff --check`: passed.
- Payload tests prove the native inspection result has exactly seven closed fields and no URL/path/ref/object ID; every `GitSyncError` maps to a stable content-free code/message.
- Lifecycle tests prove active cancellation is idempotent, marks completion for revocation, and refuses cancellation after confirmed apply begins.
- One reused Security Reviewer performed a read-only review of the actual core/native diff and returned `PASS` with no reported blocker, high, or medium finding.

The exact checkpoint and signature are recorded in `PROGRESS.md` after acceptance.
