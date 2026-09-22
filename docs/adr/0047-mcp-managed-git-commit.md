# ADR 0047: MCP managed Git commit

- Status: Accepted for Phase 6E3A planning; implementation not yet accepted
- Date: 2026-09-22

## Context

The standalone MCP server must eventually let an AI prepare and create a deliberate Git commit without receiving arbitrary Git, filesystem, credential, or shell authority. Phase 5C already provides a descriptor-confined core plan which constructs an unsigned managed-only candidate, preserves unrelated worktree changes, publishes through a recoverable intent, and never runs hooks, filters, helpers, signing programs, or network operations. Phase 6 has a per-Workspace Git-write capability and a sealed native approval channel, but exposes only `git_status` today.

## Decision

Phase 6E3A adds `git_commit_plan`, `git_commit`, `git_commit_recovery_plan`, and `git_commit_recover` by adapting the accepted Phase 5C core lifecycle. It does not create another Git implementation.

`git_status` will return an opaque process-local `gitState` fingerprint over the portable Workspace identity and its bounded, pathless status result. `git_commit_plan` accepts only the launch handle, that fingerprint, and a commit message. A mismatch produces a state conflict before expensive planning. The server then refreshes the Workspace and builds the authoritative core candidate in blocking work. The core plan remains decisive and binds exact repository, index, ref, Workspace, managed-content, message, identity, and staging facts.

Planning requires startup write+Git-write authority because candidate construction writes owner-private staging objects. Without both capabilities, the tool returns a bounded informational refusal rather than creating staging or an executable token. This is a documented Git-specific exception to the general read-only preview rule. A successful preview returns only the normalized message, safe branch, managed create/update/delete counts, unrelated-change count, unsigned/no-push/no-network facts, expiry, and one MCP plan token. It returns no path, object ID, identity, content, raw Git output, operation ID, or core token.

The MCP token is short-lived, single-use, launch/root/portable-Workspace scoped, and bound to an opaque digest of the exact core plan facts. `git_commit` accepts only the launch handle and MCP token, consumes it, obtains native Git-write approval for the exact bounded summary, refreshes the portable Workspace identity, and gives the retained opaque core plan to `apply_git_commit` in blocking work. Denial or pre-publication protocol cancellation safely quarantines the candidate. Once the recoverable core intent begins, cancellation cannot abort or roll back publication; the response reports success, stale/conflict, or recovery required without leaking the intent identifier.

Pending commit recovery blocks new commit planning. `git_commit_recovery_plan` discovers recovery internally and issues a separate short-lived native-approval plan containing only bounded policy facts. `git_commit_recover` accepts only handle+token, consumes it, obtains Git-write approval, rechecks that recovery is still required, and calls `resume_pending_git_commit_publication`. The client never selects or receives an operation identifier. Recovery is forward-only according to ADR 0024 and never resets, discards, rewrites, or guesses.

At most one live commit candidate exists per Workspace handle. Replacement is not implicit: a new plan while one is live returns busy, avoiding hidden staging churn. Expiry, denial, server shutdown, and safe pre-intent cancellation quarantine only the descriptor-identified candidate. All error and debug surfaces remain path/content/OID/Git-output/credential safe.

## Consequences

- MCP gains deliberate managed-only local commits while reusing the audited core and approval boundaries.
- Commit creation is unsigned, local, networkless, hookless, and never pushes.
- Read-only sessions retain `git_status` but cannot create private candidate objects.
- Git sync, fetch, pull, push, remote changes, signing, amend, reflogs, cleanup, arbitrary branches, arbitrary Git arguments, and scheduled commits remain deferred.
- Implementation acceptance requires focused adversarial tests, one complete repository gate, and a Security Reviewer pass.

