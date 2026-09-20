# Task Packet: Phase 6C7 revision-bound MCP Guide mark-stale

- **Objective:** Add one exact-revision `guide_mark_stale` tool without generic status or tested authority.
- **Required reasoning level:** high; this exposes an existing trust-state mutation through MCP.
- **Prerequisites:** signed Phase 6C6 checkpoint `c0084b6`; ADRs 0009, 0028, 0033, and 0034; accepted typed Guide stale matrix.
- **Allowed files or subsystem:** core public error factories, MCP server/tests, and directly applicable MCP/security/testing/operations docs.
- **Relevant specifications and ADRs:** `docs/mcp-spec.md`, `docs/content-model.md`, `docs/error-contract.md`, `docs/threat-model.md`, ADRs 0009, 0028, 0033, and 0034.
- **Exact required behavior:** Expose `guide_mark_stale` behind the alias write grant, accepting only Guide ID and exact revision. Reuse typed core mutation/recovery. Draft/tested become stale; stale is a no-op; archived remains archived with stale restorable status. Return revision, changed state, declared/effective/restorable status, and accurate committed-refresh state.
- **Important states and edge cases:** read-only alias; invalid/missing/stale Guide; draft/tested/stale/archived matrix; external edit; busy/unsafe storage; committed refresh failure.
- **Security constraints:** No caller status, reason, body, path, Category, timestamp, YAML, shell/process/network, Attachment, Git, delete, broad, destructive, or tested authority. Errors/logs omit paths/content.
- **Tests to add or update:** exact eighteen-tool schemas; read-only denial; complete stale matrix including archived restorable state; stale revision; preservation; real protocol execution; focused/full gates.
- **Commands to run:** focused MCP tests; Rust format and strict workspace Clippy; `pnpm check`; `pnpm test`; `pnpm build`; bootstrap and whitespace checks.
- **Required reviews and artifacts:** one reused Security Reviewer after implementation; ADR, packet, and evidence; no UI/design review because no renderer changes.
- **Explicit non-goals:** stale-reason metadata; generic Guide status; tested approval; Guide delete/move/content changes; Attachment/Git tools.
- **Definition of done:** Mark-stale requires write authority and exact revision; closed matrix and preservation pass; no tested authority exists; full gates and security review pass; docs match code.
- **Expected commit boundary:** error/tool/schema/tests, ADR/packet/evidence, and aligned docs form one signed checkpoint; progress follows separately.
- **Conditions requiring escalation:** caller can select status or assert tested; archived behavior loses restorable state; mutation bypasses recovery; committed state is ambiguous.
