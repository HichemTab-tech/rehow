# Task Packet: Phase 6C8 revision-bound MCP Guide Tags/Aliases

- **Objective:** Add separate exact-revision complete-replacement tools for Guide Tags and aliases.
- **Required reasoning level:** high; these are typed content-model mutations exposed through MCP.
- **Prerequisites:** signed Phase 6C7 checkpoint `aa876d9`; ADRs 0009, 0028, and 0035; accepted Guide collection validation.
- **Allowed files or subsystem:** core Guide edit variants/public error factories, MCP server/tests, and directly applicable MCP/security/testing/operations docs.
- **Relevant specifications and ADRs:** `docs/mcp-spec.md`, `docs/content-model.md`, `docs/error-contract.md`, `docs/threat-model.md`, ADRs 0009, 0028, and 0035.
- **Exact required behavior:** Expose `guide_set_tags` and `guide_set_aliases` behind the alias write grant. Each accepts its exact Guide revision and one complete ordered array, delegates to typed core recovery, reports no-op/change and current statuses, and returns correlated conflicts.
- **Important states and edge cases:** read-only alias; invalid/missing/stale Guide; empty arrays; exact count/scalar limits; Unicode-fold duplicates; unchanged arrays; tested and archived-tested trust reduction; external edit; busy/unsafe storage; committed refresh failure.
- **Security constraints:** No other collection field, title, summary, body, path, Category, timestamp, YAML, generic status, tested approval, shell/process/network, Attachment, Git, delete, broad, or destructive authority. Errors/logs omit content and paths.
- **Tests to add or update:** exact twenty-tool schemas; capability-before-parse; successful paired replacements and clears; validation/refusal byte identity; no-op revision stability; preservation; tested-to-stale; stale conflict; real protocol execution; focused/full gates.
- **Commands to run:** focused MCP/core tests; Rust format and strict workspace Clippy; `pnpm check`; `pnpm test`; `pnpm build`; bootstrap and whitespace checks.
- **Required reviews and artifacts:** one reused Security Reviewer after implementation; ADR, packet, and evidence; no UI/design review because no renderer changes.
- **Explicit non-goals:** partial add/remove semantics; normalization/reordering; body/details/Category/status changes; tested approval; Attachment/Git tools.
- **Definition of done:** Both tools require write authority and exact revision; core validation and trust reduction apply; unrelated content is preserved; full gates and security review pass; docs match code.
- **Expected commit boundary:** typed edits/errors/tools/schemas/tests, ADR/packet/evidence, and aligned docs form one signed checkpoint; progress follows separately.
- **Conditions requiring escalation:** mutation bypasses core validation/recovery; unrelated fields change; MCP can assert tested; committed state is ambiguous.
