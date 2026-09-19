# Phase 5B3C2 Git finalization evidence

Date: 2026-09-19

## Accepted boundary

This slice finalizes an already validated and journal-materialized clone in the marker-bound private `.git` scaffold. It does not add renderer IPC, checkout, ordinary sync, authentication, cleanup, or MCP authority.

The core:

- revalidates the destination capability, every materialized blob ID, and the selected staging commit/tree;
- accepts only bounded canonical `pack-<object-id>.pack`/`.idx` pairs through no-follow, owner-only, single-link descriptors;
- builds the exact worktree index in the staging repository inside the existing Bubblewrap boundary, using a fresh operation-scoped filename so an interrupted `.lock` cannot block retry;
- streams pack, index, shallow boundary, canonical config, local branch, remote-tracking branch, and `HEAD` through exact no-replace-or-verify publication;
- refuses foreign files, unexpected modes, symlinks, hardlinks, malformed names, changed source bytes, and mismatched replay state without overwrite or deletion;
- requires confined Git status to report the expected branch/upstream/origin, a clean worktree, known divergence, and zero ahead/behind; and
- publishes the immutable operation/plan-bound completion seal only after that status check, then re-verifies the exact final inventory.

No destination checkout, merge, reset, hook, filter, alias, editor, pager, shell, credential helper, prompt, or arbitrary path/ref execution is introduced.

## Recovery and adversarial evidence

Focused tests cover:

- successful finalization, clean status, and exact replay;
- interruption after pack, shallow, config, local ref, remote ref, `HEAD`, and completion-seal publication;
- foreign pack collision preservation;
- staging pack mutation after portable-content publication, with no completion seal;
- nested branch creation and upstream correlation; and
- pack/index inventory, digest, size, mode, and stable-read validation.

The full gate also exposed and corrected a pre-existing process-output race: a fast child could exit before the polling loop observed the reader's output-limit flag. The runner now checks the shared limit result after joining both readers and before accepting child status.

## Verification

- `cargo test -p rehow-core clone_finalization --all-features --locked`: 5 passed.
- `cargo test -p rehow-core clone_runner_cancels_process_groups_and_caps_output --all-features --locked`: 1 passed.
- `cargo test -p rehow-core clone_materialization_refuses_destination_and_staging_changes --all-features --locked`: 1 passed.
- `cargo test -p rehow-core git_runner_rechecks_output_limit_after_fast_exit --all-features --locked`: 1 passed.
- `cargo test -p rehow-core --all-features --locked`: 350 passed, 2 intentional ignores; all error-contract, protocol, public-API/trybuild, unknown-YAML, and doc suites passed.
- Core all-target/all-feature Clippy with warnings denied, Cargo formatting, bootstrap validation, and whitespace checks passed.
- The single reused Security Reviewer found an ignored-file sealing race and a second fast-exit output-limit race. Both were corrected with deterministic regressions; the correction verdict is PASS.

## Deferred

The native validated-session completion command and React onboarding controls remain separate follow-up work. Authenticated HTTPS/SSH, ordinary commit/pull/push/sync, staging cleanup, and MCP mutation remain out of scope.
