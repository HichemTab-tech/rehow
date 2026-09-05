# ADR 0003: Capability Storage, Revisions, and Recovery

- Status: Accepted
- Date: 2026-09-05

## Context

Selected Workspaces and imported Git repositories may contain traversal paths, symlinks, hostile filenames, special files, and concurrent edits. Path canonicalization followed by ordinary path APIs is vulnerable to time-of-check/time-of-use races; rename atomicity alone does not prevent stale overwrite or recover a multi-file operation.

## Decision

Open the user-selected root once as a retained directory capability. On Linux, prefer `openat2` beneath/no-magic/no-follow resolution with a component-walk fallback. All managed operations use IDs/validated relative paths, reject symlinks, non-regular files, and multiply linked managed files, and revalidate every operation.

Every mutation requires an opaque BLAKE3 revision containing the relative path and exact bytes (or a Workspace snapshot for broad operations). Same-revision competing Rehow writers are serialized by in-process and cross-process advisory locks; stale writes fail.

Single-file writes use same-directory exclusive temp → write/fsync → recheck → atomic no-replace/exchange → displaced-byte verification → parent fsync. Exact prior/new bytes and multi-file intent live in fsynced, mode-0700 XDG-state recovery journals/objects outside the untrusted Workspace. Startup classifies old/new/neither hashes and never guesses. Migrations/broad operations require verified snapshots. ADR 0009 defines the normative state machine.

## Consequences

- Strong confinement and recoverability on ordinary local Linux filesystems.
- More platform-specific code and fault/race testing.
- Advisory locks cannot stop an uncooperative same-user editor; atomic exchange preserves displaced bytes on supported Linux filesystems, but an editor holding an old inode can still write. The limitation is surfaced honestly.
- Network/removable filesystem guarantees may be weaker and must be detected/reported.
- Atomic replacement may not preserve unusual ACL/xattr behavior; hard-linked managed files are refused and other unsupported metadata is detected/refused rather than silently changed.

## Rejected

- String-prefix path checks.
- Symlink following within managed roots.
- Timestamp-only revisions or last-writer-wins.
- Direct overwrite, cross-filesystem implicit copy/delete, and Git-as-backup.
