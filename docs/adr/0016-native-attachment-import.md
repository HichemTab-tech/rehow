# ADR 0016: Native Attachment import

- Status: Accepted
- Date: 2026-09-18

## Context

Rehow can validate managed Attachment files but cannot yet add one. The renderer must not gain arbitrary path authority, Attachment bytes must not be loaded wholly into memory, and a crash must not leave metadata silently claiming a missing file.

## Decision

- Import begins with a core-owned native file dialog. IPC supplies only the Guide ID and exact expected revision; a selected source path never crosses IPC or persists in Workspace metadata.
- Before confirmation, the core holds the selected descriptor, captures its identity and metadata, hashes it in 64 KiB chunks, and creates a read-only import preview under the Workspace writer lock. The preview validates the exact Guide candidate and collision-resolved destination without creating Workspace content or recovery objects.
- A second core-owned confirmation identifies the sanitized Guide title, Category, stable Guide ID, exact destination path, and byte size. Approval is bound to that candidate revision and destination; cancellation is a no-op.
- The source must be a stable, single-link regular file opened without following symlinks. Import is capped at 32 MiB. Execution rechecks the selected descriptor before and after its streamed copy and requires the identity, metadata, SHA-256, BLAKE3 hash, and size approved by the preview.
- Imported bytes are inert. This phase stores no inferred media type and provides no preview, open, or execution action.
- The core creates a UUIDv7 Attachment ID, NFC destination filename, lowercase SHA-256, and exact size. Invalid filename scalars become `-`; an empty result becomes `attachment`. Existing destinations receive deterministic `-2`, `-3`, … suffixes; no file is replaced.
- The destination is `attachments/<GuideId>/<filename>`, mode `0600`. Exact private parent directories may be created descriptor-relatively with mode `0700`; an empty directory for an existing Guide is harmless recovery residue.
- One recovery journal creates the Attachment file first and then revision-checks/updates the Guide. Execution revalidates the exact preview before publishing the streamed recovery object; a newly created object is removed if execution fails before any journal exists. Tested content becomes stale under the existing lifecycle rule. Unknown existing metadata and existing Attachment entries are preserved.
- Duplicate Attachment content, limits, and invalid Guide candidates are refused before confirmation and mutation.

## Consequences

The recovery object store and Workspace installer gain streaming object verification/copy support for Attachment targets. Existing small-document behavior remains unchanged. Attachment removal, rename, external opening, MIME handling, drag/drop, broad imports, and MCP mutation are deferred.
