# ADR 0049: MCP managed Attachment discovery and metadata add

- Status: Accepted for Phase 6E2D3 implementation
- Date: 2026-09-22

## Context

MCP can list, retitle, and remove declared Attachments but cannot register a file that already exists inside a Workspace. Accepting a caller-supplied host or relative path would create an arbitrary-filesystem/confused-deputy boundary. Copying bytes through MCP is separately deferred. The existing Workspace scanner already treats undeclared regular files beneath `attachments/<GuideId>/` as orphan Attachment candidates.

## Decision

Add `attachment_candidates_list` and `attachment_metadata_add` as the only managed-file registration path.

Discovery accepts a Workspace handle, Guide ID, cursor, and limit. The Rust core enumerates only the exact descriptor-confined `attachments/<GuideId>/` directory, never another Guide directory or ambient path. It accepts only canonical UTF-8/NFC single-link regular files within existing Attachment count/file/aggregate limits, streams selected candidates, and excludes declared, linked, special, unstable, malformed, and oversized entries. Results contain an untrusted filename, decimal size, and an opaque process-local candidate token; they contain no host path, bytes, descriptor, inode, hash, or reusable filesystem authority. Pagination is bounded and cursor-bound to the retained Workspace view.

The core owns each candidate's private relative component, root/Workspace/Guide identity, Guide revision, file identity/mode/timestamps, streamed SHA-256/content digest, size, and random token. The MCP server retains at most 256 candidates for five minutes. Tokens are single-use, scoped to one launch handle/Guide/revision, and consumed on an add attempt. Expired, replayed, evicted, mismatched, or changed candidates fail closed.

`attachment_metadata_add` accepts only Workspace handle, Guide ID, exact Guide revision, candidate token, and bounded display title. It requires startup write capability, not broad or Git capability. Under the cooperating writer lock, core refreshes the exact Guide, reopens the candidate no-follow beneath its Guide directory, requires the same single-link regular-file identity/content/size and an undeclared canonical path, rechecks immediately before the recoverable Guide update, and derives every authoritative field: UUIDv7 Attachment ID, managed relative path, lowercase SHA-256, size, and trusted update time. It changes no Attachment bytes or filename. Existing unknown metadata is preserved; duplicate content, archived/missing/stale Guides, the 256-entry limit, unsafe round trips, and any candidate drift are refused. Tested content becomes stale. A post-commit external race is surfaced by canonical refresh/validation as committed-with-refresh-required; Rehow never deletes or replaces the candidate.

No native confirmation is required because the operation is a narrow, revision-checked metadata write over a file already inside the explicitly granted Workspace and does not copy, move, rename, open, execute, or delete it. Read-only startup can discover candidates but cannot register one.

## Consequences

- The MCP tool surface grows from 43 to 45 tools.
- “Add this to Rehow” can register externally/Git-created managed Attachment files without granting path or byte input.
- Candidate filenames are untrusted user content, not instructions.
- Raw upload/download, moving files between Guides, importing host paths, MIME editing, rendering, reveal/export/open, and bulk adoption remain deferred.
- Acceptance requires hostile filename/link/type/size/race/token/capability tests, a complete repository gate, and Security Reviewer PASS.
