# ADR 0006: Staged Safe Git Integration

- Status: Partially Accepted — discovery/status, ADR 0022 init/connect, ADR 0023 anonymous clone, and ADR 0024 manual commit implemented; sync remains proposed
- Date: 2026-09-05

## Context

Git is required and must eventually support init/connect/clone/commit/sync, but repositories/configuration can trigger hooks, filters, fsmonitor, textconv, pagers, editors, askpass, SSH helpers, alternates, gitfiles, and network/credential behavior. Merely using a fixed `git` command is insufficient.

## Decision

Accept stage 1, ADR 0022's process-free local initialization/remote connection, ADR 0023's anonymous-HTTPS clone, and ADR 0024's managed-only manual commit. The remaining required product stages must each receive a separate operation ADR/security gate:

1. executable/repository discovery and bounded machine-readable status/branch/remotes/divergence/conflict;
2. safe init/connect/clone plans for unchanged destinations (implemented by ADRs 0022–0023 for local init, metadata-only connect, and anonymous HTTPS);
3. ADR 0024's deliberate managed-only commit using filter-free private plumbing plus direct recoverable metadata CAS rather than porcelain;
4. explicit fetch/validated-incoming-tree/journaled-materialization/push sync with plan/current-state checks and trusted normal credentials.

Every subprocess uses an absolute trusted executable, direct argv (no shell/aliases), sanitized dangerous environment/config, disabled pager/editor/hooks/external diff/fsmonitor where applicable, time/output limits, and machine formats. Repository/worktree configuration cannot select executable helpers. Require repository top-level equal to Workspace root initially; reject unsafe ownership, external gitfiles/object paths, ambiguous conflicts, and arbitrary refspecs.

The commit ADR must limit the tree delta to enumerated Rehow-managed files/Attachments and deletions, refuse an index that differs from HEAD, preserve unrelated unstaged files, and bind a plan to HEAD OID, index checksum/tree, managed paths/revisions, unrelated-worktree status summary, message, and ref CAS. It must prove filters/hooks cannot run.

The sync ADR must prevent Git from writing the active Workspace directly: fetch to isolated refs, validate every incoming tree/path/type/size/schema/symlink, materialize accepted bytes through the core journal, and update refs with expected-OID CAS—or defer worktree integration to external Git. Credential/network code must ignore repository-selected helpers/URL rewrites/commands, allow only audited protocols, and obtain SSH-agent/helper behavior from explicitly trusted system/user provenance outside repository context.

Never expose force/reset/clean/restore/rebase/stash/automatic merge/conflict resolution. Never weaken `safe.directory`, request/store credentials, or auto-network by default. A stage that cannot meet its adversarial gate remains unavailable with guidance to use normal Git tools; later stages remain product requirements, not silently implemented shortcuts.

## Consequences

- Git content-independent read use arrives before risky mutations.
- Commit/sync implementation is more complex and may use libgit2/plumbing rather than ordinary porcelain.
- Standard SSH agent/credential-helper UX is retained only where it can be bounded and errors redacted.

## Rejected

- Shell command construction, `git add/commit/pull` without hostile-config proof, force/history rewriting, embedded credential manager, and automatic conflict resolution.
