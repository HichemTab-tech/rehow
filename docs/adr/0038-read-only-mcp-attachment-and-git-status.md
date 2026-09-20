# ADR 0038: Read-only MCP Attachment metadata and Git status

- Status: Accepted for Phase 6D2
- Date: 2026-09-20

## Decision

Add `attachments_list({workspaceHandle, guideId})` and `git_status({workspaceHandle})` as read-only MCP tools.

Attachment listing returns only the validated Guide revision and its bounded declared metadata (maximum 256 entries). Managed relative paths remain untrusted content; no Attachment bytes, host paths, file handles, or mutation authority are exposed.

Git status delegates exclusively to the accepted Phase 5 core reader: fixed root-owned Git/Bubblewrap executables, descriptor-confined repository mount, cleared/config-free environment, networkless sandbox, bounded time/output, and strict porcelain/config parsing. MCP maps this to a closed status object containing branch/upstream, counts, divergence, and remote names. It never returns remote URLs, paths, raw output, environment, or credentials.

## Consequences

- The MCP surface grows from twenty-four to twenty-six tools without adding write or network authority.
- Unsafe/unavailable/non-repository/failure states are explicit and content-free.
- Attachment bytes and Git mutation remain separate later gates.
