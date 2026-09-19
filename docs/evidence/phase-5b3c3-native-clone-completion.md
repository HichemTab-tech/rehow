# Phase 5B3C3 native clone completion evidence

Date: 2026-09-19

## Boundary

The native adapter now exposes one pathless `git_clone_complete_selected_workspace` command. It accepts no renderer payload. Before mutation it presents a native confirmation containing the exact already validated secret-free URL and an explicit description of the write boundary. The command then consumes only the opaque `GitCloneValidation` and retained empty-folder capability already held by Rust, runs the accepted core finalizer on a blocking worker, and installs the returned `WorkspaceView` as the active watched desktop session.

The lifecycle rejects competing plans, validation, completion, Workspace creation, and folder switching. Declining or cancelling the native confirmation restores the validated session without mutation. Cancellation remains available during network validation and the completion dialog but returns false after finalization starts because publication cannot be safely rolled back. Normal finalization errors restore the exact validated session for retry. Panic inside the core call is contained and also restores the session. Public errors are closed, bounded, and contain no path, URL, ref, OID, Git output, or guide content.

The review found two authority-loss paths: ordinary Workspace initialization could clear a pending validated clone before failing, and worker-join or Workspace-session installation failure could drop retry authority. Initialization now returns `completionPending` before mutation. A reclaimable native holder retains the exact pending session across the blocking worker, and every non-installation result restores it for replay.

## Verification

- `cargo test -p rehow-desktop --all-features --locked`: 21 passed.
- `cargo test -p rehow-core -p rehow-desktop --all-features --locked`: core 350 passed with 2 intentional ignores plus all supporting suites; desktop 21 passed.
- `cargo clippy -p rehow-core -p rehow-desktop --all-targets --all-features --locked -- -D warnings`: passed.
- Cargo formatting and whitespace checks passed.
- The single reused Security Reviewer passed after the native confirmation and retry-authority corrections.
- Full relevant gate and checkpoint verification are recorded in `PROGRESS.md` after completion.

## Deferred

The React onboarding action and user-visible progress/error flow remain the next slice. Authenticated transports, ordinary Git synchronization, cleanup, and MCP mutation remain out of scope.
