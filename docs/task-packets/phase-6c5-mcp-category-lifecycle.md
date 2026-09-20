# Task Packet: Phase 6C5 snapshot-guarded MCP Category archive/restore

- **Objective:** Add exact-revision reversible Category archive and restore tools while proving the default-Category safeguard under the writer lock.
- **Required reasoning level:** high; this exposes lifecycle mutation and strengthens shared Category concurrency.
- **Prerequisites:** signed Phase 6C4 checkpoint `9cfab21`; ADRs 0009, 0028, and 0032; accepted typed Category lifecycle mutation.
- **Allowed files or subsystem:** core application/protocol/storage/recovery, MCP server/tests, and directly applicable MCP/security/testing/operations docs.
- **Relevant specifications and ADRs:** `docs/mcp-spec.md`, `docs/content-model.md`, `docs/error-contract.md`, `docs/threat-model.md`, ADR 0009, ADR 0028, and ADR 0032.
- **Exact required behavior:** Expose separate `category_archive` and `category_restore` tools behind the alias write grant, each accepting only Category ID and exact revision. Reuse typed lifecycle/recovery. For all Category descriptor updates, freshly inspect and compare the complete inventory snapshot under the writer lock and bind it into the plan. Refuse default-Category archive. Return revision, changed state, refreshed snapshot, and accurate committed-refresh state.
- **Important states and edge cases:** read-only alias; invalid/missing/stale Category; active/archive transition mismatch; default Category; external manifest/default change before lock; external Category edit; busy/unsafe storage; committed refresh failure.
- **Security constraints:** No caller status Boolean, path, order, timestamp, YAML, operation ID, shell/process/network, Attachment, Git, reorder, remove, broad, or destructive authority. Errors/logs omit paths/content.
- **Tests to add or update:** exact fifteen-tool schemas; read-only denial; archive/restore preservation; default refusal; external default change under lock; real protocol archive; existing details-update regressions; focused/full gates.
- **Commands to run:** focused core/MCP tests; Rust format and strict workspace Clippy; `pnpm check`; `pnpm test`; `pnpm build`; bootstrap and whitespace checks.
- **Required reviews and artifacts:** one reused Security Reviewer after implementation; ADR, packet, and evidence; no UI/design review because no renderer changes.
- **Explicit non-goals:** Category reorder/remove; Guide lifecycle; tested approval; Attachment/Git tools; client configuration changes.
- **Definition of done:** Lifecycle requires write authority and exact revision; default changes are caught under lock; paths/Guides remain fixed; full gates and security review pass; docs match code.
- **Expected commit boundary:** lock-scoped Category snapshot prerequisite, lifecycle errors/tools/schemas/tests, ADR/packet/evidence, and aligned docs form one signed checkpoint; progress follows separately.
- **Conditions requiring escalation:** manifest/default state cannot be checked under the writer lock; caller lifecycle authority becomes generic; paths/Guides move; committed state is ambiguous.
