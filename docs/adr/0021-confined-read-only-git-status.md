# ADR 0021: Confined read-only Git status

- Status: Accepted for Phase 5A read operations only
- Date: 2026-09-19

## Context

Repository configuration is untrusted and can select hooks, fsmonitor, filters, pagers, editors, helpers, alternates, gitfiles, and external object storage. Rehow needs branch, status, divergence, conflict, and remote-name facts without granting network, credential, or mutation authority.

## Decision

Phase 5A resolves only root-owned `/usr/bin/git` and `/usr/bin/bwrap`, invokes them directly without a shell, clears the environment, disables prompts/pagers/editors/askpass/optional locks, and applies two-second and 1 MiB combined-stream bounds. Repository commands run in a Bubblewrap user/mount/network namespace. The retained Workspace, object store, refs, HEAD, and optional index/packed-ref/shallow files are inherited descriptors mounted read-only into a synthetic Git directory with no local config; object `info` is masked by an empty private mount. Gitfiles, `commondir`, and pre-existing object alternates are rejected, while a post-check alternates race remains hidden by the mount. No URL or raw stderr is returned.

Status uses porcelain v2 `-z` with submodules ignored. Local configuration is read separately from its retained regular-file descriptor with includes disabled; only bounded, control-free remote names and the current branch's remote/merge tracking pair are interpreted. A conservative ref grammar feeds config-free sandboxed `rev-list --left-right --count`; the payload distinguishes known divergence from unavailable counts. The native command accepts no path or repository argument. This slice performs no network operation and exposes no init, clone, remote mutation, staging, commit, reset, checkout, merge, fetch, pull, or push command.

## Consequences

- Missing Git and absent/unsafe repositories are ordinary structured states; content features remain available.
- Root-owned fixed paths intentionally exclude user-managed Git installations, and missing Bubblewrap makes repository status unavailable, until a separately reviewed trusted-executable/sandbox policy exists.
- Every mutation remains blocked behind its own ADR and security gate.
