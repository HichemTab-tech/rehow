# Task Packet: Phase 6C9 revision-bound MCP Guide Category change

- **Objective:** Add a typed exact-revision `guide_change_category` tool with lock-scoped destination-Category concurrency protection and no caller path authority.
- **Required reasoning level:** high; this exposes a journaled two-path filesystem mutation through MCP.
- **Prerequisites:** signed Phase 6C8 checkpoint `620c117`; ADRs 0009, 0028, and 0036; accepted Phase 3L Guide move semantics.
- **Allowed files or subsystem:** core move storage/application/error factories, MCP server/tests, and directly applicable MCP/security/testing/operations docs.
- **Relevant specifications and ADRs:** `docs/mcp-spec.md`, `docs/content-model.md`, `docs/error-contract.md`, `docs/threat-model.md`, ADRs 0009, 0028, and 0036.
- **Exact required behavior:** Accept exact Guide and destination Category revisions, derive paths/filename in core, require the destination remain active, verify the descriptor prerequisite under the move writer lock, create destination before removing source, refresh and prove one resulting Guide, and return the new revision/status. Same-Category is a lock-verified no-op.
- **Important states and edge cases:** read-only alias; invalid/missing/stale Guide or Category; same Category; archived or concurrently changed destination; occupied filename; external source edit; tested/archived-tested trust reduction; busy/unsafe storage; interrupted/committed refresh.
- **Security constraints:** No caller path/filename/title/summary/body/Tags/aliases/status/time/YAML/operation ID, tested approval, shell/process/network, Attachment, Git, delete, broad, or destructive authority. Never overwrite destination; errors omit paths/content.
- **Tests to add or update:** exact twenty-one-tool schema; capability-before-parse; same-Category verified no-op; successful move/preservation; tested-to-stale; stale Guide/Category and concurrent destination archive; occupied destination; real protocol execution; focused/full gates.
- **Commands to run:** focused core/MCP tests; Rust format and strict workspace Clippy; `pnpm check`; `pnpm test`; `pnpm build`; bootstrap and whitespace checks.
- **Required reviews and artifacts:** one reused Security Reviewer after implementation; ADR, packet, and evidence; no UI/design review because no renderer changes.
- **Explicit non-goals:** filename tidy, generic details update, Category creation, Guide deletion, Attachment relocation, tested approval, Git tools.
- **Definition of done:** Guide and destination revisions are lock-checked; path is core-derived; move/no-op and recovery semantics pass; unrelated content is preserved; full gates and security review pass; docs match code.
- **Expected commit boundary:** storage/core/tool/schema/tests, ADR/packet/evidence, and aligned docs form one signed checkpoint; progress follows separately.
- **Conditions requiring escalation:** destination prerequisite cannot share the writer lock; overwrite or caller path authority appears; recovery becomes ambiguous; MCP can assert tested.
