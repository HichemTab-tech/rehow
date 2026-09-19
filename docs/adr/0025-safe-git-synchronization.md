# ADR 0025: Safe Git synchronization

- Status: Partially Accepted — isolated inspection and bounded ancestry implemented; integration and push pending
- Date: 2026-09-19

## Context

Synchronization combines untrusted network data, repository history, credentials, portable Workspace mutation, and ref publication. `git pull` writes the active repository and worktree before Rehow can validate the incoming snapshot; `git push` may publish stale or unintended history. Repository configuration may also select hooks, helpers, proxies, filters, transports, or commands.

## Decision

Synchronization is a sequence of separately accepted gates. Every gate binds the current Workspace, direct local branch, exact Git metadata/configuration, configured upstream, and prior gate output. No gate runs pull, merge, rebase, checkout, reset, clean, stash, or a shell.

### 5D1 — isolated incoming inspection

Core reads the current branch's exact `branch.<name>.remote` and `branch.<name>.merge` plus that remote's single canonical URL from the held config with includes disabled. Duplicate, missing, local-dot, non-branch, noncanonical, credential-bearing, or otherwise ambiguous configuration is refused. Planning is single-use, redacted, and bound to the current Workspace fingerprint, manifest revision, root capability, Git directory identity, repository snapshots, direct branch, object format, managed inventory, and clean/dirty status. A pending recoverable commit blocks planning.

Apply currently accepts anonymous canonical HTTPS only. It sends the canonical URL and exact upstream branch explicitly to the same config-free, credential-disabled Bubblewrap network boundary accepted for clone. Discovery accepts exactly one full-width OID/ref record. Fetch writes only a random owner-private XDG staging repository, requests exactly the advertised OID with depth one, retains packs, disables hooks/helpers/prompts/proxies/tags/submodules/maintenance, and enforces the existing time/output/file/aggregate limits.

The complete fetched commit, tree, paths, object identities, blob bytes, Workspace schemas, relationships, and Attachments pass the accepted clone validator. The incoming Workspace ID must equal the open Workspace ID. Core repeats the full local plan binding after validation. The result exposes only `upToDate` or `remoteChanged`, whether the active worktree has local changes, and whether the portable Workspace snapshots match. URL, paths, refs, OIDs, content, Git output, credentials, and staging authority remain private. This gate never changes the active Workspace, object database, index, refs, config, reflogs, or remote-tracking refs.

### 5D2A — bounded ancestry classification

Core consumes the opaque validated inspection, honors cancellation before every successful return, and revalidates the complete local plan before and after classification. Equal object IDs are `upToDate` without another network operation. Otherwise, core deepens only the exact already-advertised object in the same private staging repository to a 4,096-commit cap, rechecks that its validated tree is unchanged, and reads both local and staged histories through config-free, hook-disabled, read-only sandboxes. The deepening child receives only a conservative fraction of the verified remaining aggregate staging budget, with fixed metadata headroom; the complete staging tree is rescanned afterward.

History output is capped and accepts only full-width object IDs headed by the expected commit. Exact shallow-file content and metadata are plan-bound and checked around local reads; staged shallow state and repository metadata are checked around staged reads. Proven containment yields `remoteFastForward` or `localAhead`; non-containment yields `diverged` only when both histories are complete. A shallow boundary or cap yields `historyLimit`, never a guessed relationship. The result retains integration authority opaquely and exposes only the ancestry class and prior bounded facts. No active content, object, index, ref, config, or reflog is changed.

### Pending gates

5D2B must materialize proven remote-fast-forward content through the Workspace recovery journal and update objects, index, and the direct branch ref by expected-OID/state compare-and-swap. Ambiguous, history-limited, or locally dirty states require explanation, not automatic resolution.

5D3 may push only an exact reviewed local commit from the expected remote OID using an explicit refspec and native confirmation. Non-fast-forward, changed remote, auth, host-key, and transport failures are preserved and reported. No force option exists.

Authenticated HTTPS/SSH requires the separately audited credential/agent provenance described by ADR 0023. Scheduled or start-time sync remains opt-in and cannot precede acceptance of the same gates used by manual sync.

## Consequences

- The first accepted slice can securely answer whether the configured upstream snapshot is valid and changed, but cannot yet modify or publish anything.
- A changed remote is called remote-fast-forward, local-ahead, or diverged only when bounded history proves that result; otherwise it is history-limited.
- Private staging is retained as exact evidence/input for the next gate; identity-safe retention/pruning remains required.
- Unsupported authentication degrades to an actionable unavailable state rather than a token prompt.
