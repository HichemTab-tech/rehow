# Task Packet: Phase 6C6 revision-bound MCP Guide archive/restore

- **Objective:** Add separate exact-revision reversible Guide archive and restore tools without generic status or tested authority.
- **Required reasoning level:** high; this exposes an existing typed lifecycle mutation through MCP.
- **Prerequisites:** signed Phase 6C5 checkpoint `2b48223`; ADRs 0009, 0028, and 0033; accepted typed Guide lifecycle matrix.
- **Allowed files or subsystem:** core public error factories, MCP server/tests, and directly applicable MCP/security/testing/operations docs.
- **Relevant specifications and ADRs:** `docs/mcp-spec.md`, `docs/content-model.md`, `docs/error-contract.md`, `docs/threat-model.md`, ADR 0009, ADR 0028, and ADR 0033.
- **Exact required behavior:** Expose separate `guide_archive` and `guide_restore` tools behind the alias write grant, each accepting only Guide ID and exact revision. Reuse typed core lifecycle/recovery. Return revision, changed state, declared/effective status, and accurate committed-refresh state. Refresh and return correlated conflicts.
- **Important states and edge cases:** read-only alias; invalid/missing/stale Guide; active/archive transition mismatch; draft/stale/tested/legacy restore matrix; external edit before lock; busy/unsafe storage; committed refresh failure.
- **Security constraints:** No caller status Boolean, body, path, Category, timestamp, YAML, operation ID, shell/process/network, Attachment, Git, delete, broad, destructive, or tested authority. Errors/logs omit paths/content.
- **Tests to add or update:** exact seventeen-tool schemas; read-only denial; archive/restore preservation; status outputs; stale conflict shared coverage; real protocol archive; core tested/stale lifecycle suites; focused/full gates.
- **Commands to run:** focused MCP tests; Rust format and strict workspace Clippy; `pnpm check`; `pnpm test`; `pnpm build`; bootstrap and whitespace checks.
- **Required reviews and artifacts:** one reused Security Reviewer after implementation; ADR, packet, and evidence; no UI/design review because no renderer changes.
- **Explicit non-goals:** Guide delete/move/details changes; mark tested; Category mutations; Attachment/Git tools; client configuration changes.
- **Definition of done:** Lifecycle requires write authority and exact revision; path/body/Category remain fixed; restore cannot assert tested; full gates and security review pass; docs match code.
- **Expected commit boundary:** lifecycle errors/tools/schemas/tests, ADR/packet/evidence, and aligned docs form one signed checkpoint; progress follows separately.
- **Conditions requiring escalation:** restore can assert tested; caller status/path/body authority expands; mutation bypasses recovery; committed state is ambiguous.
