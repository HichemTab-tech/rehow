# ADR 0017: Native Attachment removal

- Status: Accepted
- Date: 2026-09-18

## Context

Rehow can import and validate Attachments but cannot remove one. Removal changes both Guide metadata and a potentially large managed file, so confirmation must identify the exact effect and recovery must not allocate the whole Attachment or permanently delete it.

## Decision

- Renderer IPC supplies only stable Guide and Attachment IDs plus the exact Guide revision. Rust resolves the managed path from validated Guide metadata.
- Before confirmation, the core acquires the Workspace writer lock, prepares the exact Guide candidate, and streams the Attachment to verify its stable descriptor state, declared SHA-256/size, BLAKE3 object identity, mode, and path-sensitive revision. This preview writes nothing.
- Native confirmation identifies the sanitized Guide title, Category, stable Guide and Attachment IDs, exact managed path, and size. Approval is bound to the previewed Guide candidate and Attachment state.
- Execution reacquires the lock and requires every preview fact to match. Before installing the journal it streams the exact Attachment into the private content-addressed XDG recovery store, using fixed memory and rechecking descriptor stability. One recovery journal updates Guide metadata first and then removes the managed Attachment without replacement. Neither target is permanently deleted.
- Attachment trash replay first renames the original inode to a deterministic held-source path under the operation trash tree, then streams the immutable recovery object to `.rehow/trash/<operation-id>/<managed-path>`. This prevents a process holding the original inode open from corrupting the canonical trash copy; later writes remain preserved on the held inode. Preflight and replay require the recovery object and verify bounded hashes, mode, and path-sensitive revision.
- Removing Attachment metadata makes a tested Guide stale under the existing lifecycle rule and preserves every remaining Attachment plus unknown metadata. Archived Guides must be restored before removal.
- Rehow does not remove the now-empty `attachments/<guide-id>/` directory automatically.

## Consequences

The recovery executor gains a streamed special case for managed Attachment trash targets while its canonical journal schema remains unchanged. Recovery storage temporarily duplicates removed Attachment bytes until a future retention policy safely prunes operation artifacts. Rename, preview/open, MIME behavior, bulk removal, permanent trash pruning, and MCP mutation remain deferred.
