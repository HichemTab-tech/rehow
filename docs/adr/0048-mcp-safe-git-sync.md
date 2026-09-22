# ADR 0048: MCP safe Git synchronization

- Status: Accepted for Phase 6E3B planning; implementation not yet accepted
- Date: 2026-09-22

## Context

Phase 5D already provides the only accepted synchronization machinery: isolated remote inspection with bounded history classification, authenticated HTTPS/SSH through launch-captured opaque resources, exact remote fast-forward materialization/publication with durable recovery, and a server-enforced ordinary push for a proven local-ahead branch. MCP currently cannot use it. A protocol client must not gain URLs, refs, object IDs, credentials, paths, Git arguments, or ambient network/credential authority.

## Decision

Phase 6E3B adds `git_sync_plan`, `git_sync`, `git_sync_recovery_plan`, and `git_sync_recover`. `git_sync_plan` accepts only a Workspace handle, exact process-local `gitState`, and closed `pull` or `push` direction. It requires startup write+Git-write authority, the trusted native helper, private recovery storage, and the shared per-Workspace Git lifecycle reservation.

Because inspection uses the network and may invoke an already configured credential helper or SSH agent, the server first constructs the pathless core plan and obtains native approval for the exact remote name, safe host, branch, transport, direction, and credential/network policy. Denial performs no network operation. Approval is not supplied by MCP. The server then prepares opaque launch-captured authentication resources and runs the accepted inspection/classification off the protocol runtime with JSON-RPC cancellation propagated to the core.

The result exposes only direction, closed ancestry, remote name, branch, transport, local-change and portable-Workspace-match Booleans, action availability, and—only for an allowed action—a short-lived single-use token. It exposes no URL, ref, OID, credential kind/value, identity, path, content, or raw Git output. `pull` is executable only for a clean proven remote fast-forward. `push` is executable only for proven local-ahead state. Up-to-date, opposite-direction, dirty, diverged, and history-limit results are informational and create no executable token.

`git_sync` accepts only handle+token, consumes it, obtains a second native approval for the exact pull or push effect, refreshes portable Workspace identity, and invokes the retained classification. Pull uses only `apply_git_sync_fast_forward`; push uses only `apply_git_push_with_authentication`. No force, reset, checkout, merge, rebase, stash, discard, ref selection, conflict resolution, or arbitrary Git is possible. Once approved publication begins it runs to a core-defined terminal/recovery state rather than being abandoned by protocol cancellation.

Pull recovery uses an opaque core recovery plan bound to the exact pending sync operation. The separately approved recovery call cannot select an operation and refuses replacement by another intent. The shared lifecycle reservation excludes commit, sync, and both recovery planners/executors for the same Workspace from planning through termination.

Authentication provenance is captured only when at least one startup Workspace has Git-write authority. HTTPS uses the accepted fixed credential proxy only when trusted provenance exists and otherwise attempts anonymous access. SSH requires the accepted held agent, host-key aggregate, fixed proxy, and system client. Rehow never requests, returns, logs, or stores a credential.

## Consequences

- MCP gains deliberate pull/push synchronization without a generic Git surface.
- Network/credential access and later mutation are separate native approvals.
- Non-actionable inspection is still useful but cannot be converted into a mutation token.
- Private validated staging may remain as accepted Phase 5D state; cleanup/pruning remains a separately designed operation.
- Scheduled/automatic sync remains deferred because it cannot satisfy this interactive approval contract.
- Acceptance requires isolated core sync/recovery coverage, adapter capability/schema/cancellation/replay/concurrency tests, the complete repository gate, and Security Reviewer PASS.
