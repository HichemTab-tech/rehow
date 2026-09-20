# Task Packet: Phase 6C4 revision-bound MCP Category details update

- **Objective:** Add one capability-gated tool that atomically replaces a Category's name and optional summary at an exact revision.
- **Required reasoning level:** high; this exposes an existing native mutation through MCP.
- **Prerequisites:** signed Phase 6C3 checkpoint `7a30335`; ADRs 0009, 0028, and 0031; accepted typed Category-details mutation.
- **Allowed files or subsystem:** core public error factories, MCP server/tests, and directly applicable MCP/security/testing/operations docs.
- **Relevant specifications and ADRs:** `docs/mcp-spec.md`, `docs/content-model.md`, `docs/error-contract.md`, `docs/threat-model.md`, ADR 0009, ADR 0028, and ADR 0031.
- **Exact required behavior:** Expose `category_update(workspaceHandle, categoryId, expectedRevision, name, summary?)` behind the alias write grant. Reuse the typed core mutation and recovery journal. Return revision, changed state, refreshed Category snapshot, and accurate committed-refresh state. Return correlated not-found/revision errors after conflict refresh.
- **Important states and edge cases:** read-only alias; invalid ID/revision/name/summary; missing or stale Category; safe no-op; unsafe presentation; busy/unsafe storage; external edit before the writer lock; committed refresh failure.
- **Security constraints:** No caller path, order, status, timestamp, YAML, operation ID, shell/process/network, Attachment, Git, lifecycle, broad, or destructive authority. Errors/logs omit paths and content.
- **Tests to add or update:** exact thirteen-tool/schema surface; read-only denial without recovery state; successful update/no authority expansion; external stale-revision refusal preserving external bytes; real protocol call; focused/full gates.
- **Commands to run:** focused MCP tests; Rust format and strict workspace Clippy; `pnpm check`; `pnpm test`; `pnpm build`; bootstrap and whitespace checks.
- **Required reviews and artifacts:** one reused Security Reviewer after implementation; ADR, packet, and evidence; no UI/design review because no renderer changes.
- **Explicit non-goals:** path tidy/rename; ordering; archive/restore/remove; Guide mutation; tested approval; Attachment/Git tools; client configuration changes.
- **Definition of done:** Update requires write authority and exact revision; only typed details change; stale/read-only/unsafe cases write nothing; full gates and security review pass; docs match code.
- **Expected commit boundary:** public structured errors, MCP tool/schema/tests, ADR/packet/evidence, and aligned docs form one signed checkpoint; progress follows separately.
- **Conditions requiring escalation:** the core typed mutation cannot be reused; caller authority expands; concurrency is not revision-bound; committed state is ambiguous; shared structured errors cannot be maintained.
