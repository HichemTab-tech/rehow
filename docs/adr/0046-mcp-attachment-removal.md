# ADR 0046: MCP Attachment removal

- Status: Accepted for Phase 6E2D2
- Date: 2026-09-21

## Decision

`attachment_remove_plan` is a read-only preview for one declared managed Attachment at an exact Guide revision. The preview returns bounded untrusted display metadata and an informational effect fingerprint. An executable 90-second token is issued only with startup write+broad authority, trusted sealed native approval, available recovery storage, and a successful blocking core preview of the exact Guide candidate plus streamed Attachment device/inode, identity/hash/size/mode/path revision.

The private plan binds the launch handle, portable Workspace ID, Guide/Attachment IDs, exact Guide revision, managed path, and an opaque core hash of every approved Attachment-removal preview fact. `attachment_remove` accepts only Workspace handle and token, consumes it once, obtains Destructive native approval, then refreshes and executes the opaque core approval in blocking work so protocol cancellation remains dispatchable.

The core rechecks the exact Guide/file under the recovery writer lock, streams the before-image into the private recovery store with bounded memory, checks cancellation immediately before journal creation, and uses the accepted metadata-first plus recoverable-trash journal. The journal binds the approved source device/inode. After atomic displacement, both held identity and bytes are verified before success. A mismatch preserves both files and a post-first-target conflict returns `RECOVERY_REQUIRED` with the operation ID and bounded Guide/Attachment paths. Once a journal begins, recovery semantics own completion. Denial, cancellation, replay, expiry, cross-Workspace use, drift, archived/missing targets, and unsafe files never begin the journal.

## Consequences

- MCP cannot provide a host path, file bytes, hash, size, mode, metadata patch, trash path, operation ID, effect set, or consent assertion.
- Planning remains useful in read-only/no-broad sessions but returns no executable token.
- Removal never permanently deletes the file, recursively removes directories, or affects another Attachment.
- Tested Guides become Stale through the existing typed metadata-removal rule; unknown remaining metadata stays preserved.
- Raw Attachment transfer, metadata add, bulk removal, rename, preview/open/export/reveal, trash pruning, Git, and shell authority remain deferred.
