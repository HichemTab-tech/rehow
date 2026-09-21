# Task Packet: Phase 6E2D1 MCP Attachment title update

- **Objective:** Add exact-revision MCP replacement of one declared managed Attachment's display title without exposing file or path authority.
- **Required reasoning level:** high implementation; batch security review with the later destructive Attachment slice.
- **Prerequisites:** Accepted Attachment schema/import/removal work, Phase 6 write capability, ADRs 0016 and 0045.
- **Allowed files or subsystem:** Core Guide mutation/application code, MCP server, Phase 6 docs/evidence, and progress after commit.
- **Relevant specifications and ADRs:** `docs/content-model.md` Attachments, `docs/mcp-spec.md`, `docs/threat-model.md`, ADRs 0016 and 0045.
- **Exact required behavior:** `attachment_metadata_update` accepts only handle, Guide ID/revision, Attachment ID, and title; requires startup write; atomically replaces only the existing title; identical input is a no-op; Tested becomes Stale.
- **Important states and edge cases:** Missing Guide/Attachment, stale revision, invalid/overlong title, archived Guide, read-only Workspace, unknown nested metadata, comments/unsafe presentation, busy/conflict, refresh-required, no-op.
- **Security constraints:** No path, bytes, hash, size, media type, ID generation, status, shell, Git, network, broad, or consent input. Preserve unknown metadata and reject ambiguous presentation.
- **Tests to add or update:** Core narrow-mutation/preservation/stale/no-op/invalid cases; MCP write-gate, exact schema, successful update, invariant fields, revision/no-op behavior.
- **Commands to run:** Focused core/MCP tests and strict Clippy while iterating; one full repository gate before checkpoint.
- **Required reviews and artifacts:** Evidence document now; one reused Security Reviewer for the combined Attachment mutation batch before that batch is accepted.
- **Explicit non-goals:** Metadata add, media-type editing, raw transfer, host paths, file rename/replacement/removal, Attachment rendering/export/reveal, Git tools.
- **Definition of done:** Narrow schema, typed core mutation, preservation tests, focused/full gates, truthful evidence/progress, signed checkpoint.
- **Expected commit boundary:** One implementation/docs checkpoint, then a progress-only ledger checkpoint if useful.
- **Conditions requiring escalation:** Any need for client path/byte/hash authority, unknown-metadata loss, file mutation, new dependency, or unresolved destructive semantics.
