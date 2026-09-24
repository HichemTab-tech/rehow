# ADR 0061: Safe Git author onboarding

- Status: Accepted and implemented
- Date: 2026-09-24

## Context

Managed commits intentionally ignore global Git configuration. That isolation prevents hostile includes and repository-selected behavior, but a repository without local `user.name` and `user.email` previously left users with an error and no safe in-app recovery.

## Decision

Git status reports only whether the repository-local identity is `configured`, `missing`, or `invalid`. Values never cross renderer IPC.

When both local values are absent, Rehow may append one repository-local `[user]` section through the existing bounded config parser, revision-bound plan, native confirmation, atomic exchange, durable before-image, and recovery machinery. The user can enter values manually or explicitly import one complete launch-captured global identity.

Global import reads only stable, owner-controlled regular `~/.gitconfig` and XDG Git config snapshots. Symlinks, unsafe modes/owners/ancestry, oversized or changing files, includes, malformed syntax, partial identity, duplicates, and invalid values make import unavailable. Values remain native/core-only, are redacted from `Debug`, and appear only in the parented native confirmation. Rehow never edits global Git configuration.

If either local key already exists, Rehow does not replace it. A complete pair is already configured; a partial, duplicate, or invalid pair must be repaired with Git outside Rehow. This keeps replacement semantics and ambiguous configuration out of the onboarding operation.

## Consequences

- Rehow can deliberately reuse the safe identity already configured on the machine without weakening managed-commit isolation.
- Repository-local commits remain deterministic and do not change if global configuration changes later.
- Conditional includes and system configuration are intentionally not imported.
- Identity replacement remains deferred to a separately designed operation.
