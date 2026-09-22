# ADR 0050: Ordinary Markdown copy import boundary

- Status: Accepted for phased implementation
- Date: 2026-09-22

## Context

Rehow identifies ordinary Markdown collections but currently cannot import them. Import must never reinterpret untrusted source frontmatter as authoritative Rehow metadata, mutate the source, follow links, accept renderer paths, or leave an invalid destination after interruption.

## Decision

V1 ships copy import only; in-place adoption remains deferred. Native folder dialogs retain separate descriptor capabilities for the source collection and an empty destination. Renderer IPC receives only bounded preview data and supplies Workspace/Category display names; it never supplies a host path, source bytes, ID, timestamp, status, or destination layout.

The source preview recursively scans at most four directory levels and 20,000 entries. It considers lowercase `.md` files only, accepts at most 10,000 single-link regular files, reads each with `O_NOFOLLOW`, caps each source at 2 MiB minus 4 KiB reserved for generated Guide metadata and the accepted aggregate at 50 MiB, and rejects changed, linked, special, non-UTF-8, empty, NUL/control-bearing, or otherwise non-normalizable documents. Preview reports bounded included paths plus exact excluded-non-Markdown, malformed/unsafe, and oversized counts. Display paths are inert escaped relative labels, not links or filesystem authority.

Every accepted source is snapshotted by relative components, device/inode/mode/size/timestamps, BLAKE3 content digest, and normalized-body digest. Apply reopens and revalidates every source before the destination changes, then retains the normalized bodies in bounded memory so later source changes cannot affect the import. UTF-8 BOM is removed, CRLF/bare CR become LF, and trailing newlines become exactly one; preview identifies normalized entries. The complete normalized source document, including any source frontmatter, becomes inert Guide body text. It is never promoted to Rehow metadata. A bounded `Imported from <escaped relative path>` summary supplies provenance.

The user explicitly maps all previewed files to one newly created Category in one newly created Workspace. Core generates all UUIDv7 IDs/timestamps/filenames and emits only `draft` Guides. Destination writes use existing per-document recovery primitives. The destination is always either unchanged or a valid Rehow Workspace containing a valid prefix of the import. If a later write cannot complete, Rehow opens that valid partial Workspace, reports exact imported/total counts, and preserves the retained plan for retry where possible; it never deletes the partial result or source. No Git repository, Attachment, external opener, shell, or MCP authority is added.

## Consequences

- Implementation is gated as read-only preview, then destination/apply, then UI.
- Source frontmatter cannot spoof IDs, status, timestamps, paths, verification, Categories, Attachments, or links.
- Normalization is explicit and source bytes remain untouched.
- Atomic all-or-nothing bulk import is not promised; every committed prefix is valid, recoverable, and accurately reported.
- Acceptance requires source/destination substitution, symlink/hardlink/special-file, hostile filename, depth/count/size/aggregate, content-race, partial-write, draft-only, and source-unchanged tests plus one final Security Reviewer pass.
