# ADR 0045: MCP Attachment title update

- Status: Accepted for Phase 6E2D1
- Date: 2026-09-21

## Decision

Phase 6E2D1 adds one non-destructive `attachment_metadata_update` tool. It replaces only the required display title of one already-declared managed Attachment, identified by stable Attachment ID inside one exact Guide revision. Startup write authority is required; broad authority and native approval are not because no file, path, identity, hash, size, or media-type claim changes.

The MCP schema accepts only Workspace handle, Guide ID, expected Guide revision, Attachment ID, and the replacement title. The core validates the same 1–200-scalar display-text rule as parsing, requires an unarchived Guide and round-trip-proven block presentation, replaces only the existing `title` scalar, preserves all unknown metadata, and journals the exact-revision Guide update. A Tested Guide becomes Stale. An identical title is a byte-identical no-op.

## Consequences

- The client cannot add an Attachment, select a host path, upload/download bytes, change managed path/hash/size/media type/ID, delete a file, or assert Tested status.
- Missing Attachment IDs, invalid titles, archived Guides, stale revisions, unsafe presentation, and storage failures fail without mutation.
- Attachment metadata add remains deferred until a safe already-managed-file discovery contract exists. Metadata/file removal remains a broad, dry-run, native-approved recovery operation.
- This bounded slice can be reviewed with the later Attachment mutation batch rather than spawning a separate reviewer.
