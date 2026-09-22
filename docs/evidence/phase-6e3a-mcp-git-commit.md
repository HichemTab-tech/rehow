# Phase 6E3A evidence: MCP managed Git commit

Status: accepted; complete repository gate and correction-only Security Reviewer PASS on 2026-09-22

## Implemented boundary

- `git_status` returns a keyed process-local `gitState` without adding paths, URLs, raw output, credentials, or object IDs.
- `git_commit_plan` requires startup write+Git-write authority, exact `gitState`, retained recovery storage, and the trusted native helper. It constructs the accepted Phase 5C core candidate off the protocol runtime and returns only normalized message, safe branch, bounded counts, policy facts, expiry, effect fingerprint, and a single-use MCP token.
- `git_commit` accepts only handle+token, obtains sealed native Git-write approval, rechecks the portable Workspace identity, and consumes the opaque core plan. It never pushes, uses the network, signs, invokes hooks/helpers/filters, or commits unrelated worktree files.
- `git_commit_recovery_plan` captures an opaque core recovery plan for the exact pending publication. `git_commit_recover` refuses a replaced intent; no operation identifier or Git selector crosses MCP.
- One atomic per-Workspace reservation covers normal and recovery planning before expensive core work begins and remains held through native approval and apply/recovery termination. Expiry, denial, replay, scope mismatch, server shutdown, stale state, and pre-publication cancellation consume/quarantine exact private state.
- Protocol cancellation is checked by the core at the last boundary before durable intent installation. Candidate preparation may leave harmless append-only Git objects, but cancellation publishes no index, ref, or recovery intent.

## Focused verification so far

- `cargo test -p rehow-mcp --all-features`: 80 unit tests, 3 approval-helper integration tests, and 1 stdio integration test pass.
- The managed-commit tests prove state mismatch refusal, read-only refusal, normalized plan facts, native-approved publication, preservation of an unrelated untracked file, exact commit message, non-disclosure, replay refusal, no-recovery refusal, atomic planning exclusion across an active execution, and real JSON-RPC cancellation before durable publication in isolated temporary repositories.
- `cargo test -p rehow-core commit_recovery_plan_is_bound_to_the_exact_interrupted_operation`: the opaque recovery plan for interrupted operation A refuses after operation B replaces it, without consuming B.
- `cargo clippy -p rehow-core -p rehow-mcp --all-targets --all-features -- -D warnings` passes.
- Final consolidated gate passes: `pnpm check`, `pnpm test`, `pnpm build`, `cargo fmt --all --check`, strict workspace Clippy, bootstrap validation, and `git diff --check`. Results include 39 Vitest tests; 452 core tests passed with 2 intentional ignores; 25 desktop tests; 80 MCP unit tests; 3 MCP approval integration tests; and 1 MCP stdio integration test.
- The correction-only Security Reviewer verdict is PASS after exact recovery selection, final-boundary cancellation, atomic pre-planning reservation, and full-lifecycle reservation findings were closed.

No Phase 6E3A security blocker remains.
