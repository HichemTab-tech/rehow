# Task Packet: Phase 6C3 snapshot-bound MCP Category creation

- **Objective:** Add one capability-gated tool that creates a canonical active Category only against an exact complete Category inventory snapshot.
- **Required reasoning level:** high; this adds external create authority and a new cross-inventory concurrency token.
- **Prerequisites:** signed Phase 6C2 checkpoint `9c9078c`; ADRs 0009, 0028, and 0030; accepted recovery-journaled Category creation.
- **Allowed files or subsystem:** core application/protocol/storage/recovery, MCP server/tests, and directly applicable MCP/security/testing/operations docs.
- **Relevant specifications and ADRs:** `docs/mcp-spec.md`, `docs/content-model.md`, `docs/error-contract.md`, `docs/threat-model.md`, ADR 0009, ADR 0028, and ADR 0030.
- **Exact required behavior:** Return one opaque complete-inventory snapshot from Category listing. Expose `category_create(workspaceHandle, expectedCategorySnapshot, name, summary?)` behind the alias write grant. Core owns ID/time/path/order/active state. Under the writer lock, enforce the raw 1,000-entry cap, freshly inspect the compatible Workspace, compare the complete snapshot, and commit one absent-only canonical descriptor. Return the created ID/revision and refreshed snapshot, or accurate committed-refresh-required output.
- **Important states and edge cases:** read-only alias; malformed snapshot; invalid name/summary; stale retained view; external Category edit/create/residue; raw entry limit; busy writer; target collision; incompatible current Workspace; interruption; committed refresh failure.
- **Security constraints:** No caller path, ID, order, status, timestamp, YAML, operation ID, shell/process/network, Attachment, Git, broad, lifecycle, or destructive authority. Errors and logs omit paths/content.
- **Tests to add or update:** exact twelve-tool/schema surface; snapshot returned by list; read-only denial without recovery state; successful core-owned creation; complete-inventory external-edit conflict with no directory; stale-view raw-entry-limit refusal under the lock; focused/full gates.
- **Commands to run:** focused MCP/core tests; Rust format and strict workspace Clippy; `pnpm check`; `pnpm test`; `pnpm build`; bootstrap and whitespace checks.
- **Required reviews and artifacts:** one reused Security Reviewer after implementation; ADR, packet, and evidence; no UI/design review because no renderer changes.
- **Explicit non-goals:** Category update/reorder/archive/remove; caller order/path/ID; Guide changes; tested approval; Attachment/Git tools; client configuration changes.
- **Definition of done:** Creation requires the write grant and exact full inventory snapshot; authoritative fields are core-owned; stale/limit cases write nothing; recovery remains bounded; full gates and security review pass; docs match code.
- **Expected commit boundary:** inventory snapshot, lock-scoped create checks, MCP tool/schema/tests, ADR/packet/evidence, and aligned docs form one signed checkpoint; progress follows separately.
- **Conditions requiring escalation:** the complete inventory cannot be revalidated under the writer lock; malformed entries can bypass the hard cap; caller-controlled authority expands; create can overwrite; committed state is reported ambiguously.
