# Task Packet: Phase 6E1A MCP startup capabilities

- **Objective:** Add explicit per-Workspace broad and Git-write startup grants without exposing a new operation.
- **Required reasoning level:** high; security review required.
- **Prerequisites:** signed Phase 6D2 checkpoint `2477afd`; ADRs 0026 and 0039.
- **Allowed files or subsystem:** MCP startup parser/server tests and directly applicable documentation.
- **Relevant specifications and ADRs:** `docs/mcp-spec.md`, `docs/threat-model.md`, ADRs 0026 and 0039.
- **Exact required behavior:** Parse repeatable `--allow-broad <alias>` and `--allow-git-write <alias>`; bind them to one configured alias; require that alias's `--allow-write`; report the four effective capabilities.
- **Important states and edge cases:** arbitrary flag order; multiple Workspaces; duplicate, unknown, malformed, and missing-write grants; read-only defaults.
- **Security constraints:** Fail the whole launch on ambiguity; no ambient/path/client-granted authority; no new tool, mutation, process, network, approval, or tested-status route.
- **Tests to add or update:** parser acceptance/rejection; server capability output; existing exact tool set and full gates remain green.
- **Commands to run:** MCP tests; pnpm check/test/build; strict Workspace Clippy; Cargo format; bootstrap and whitespace checks.
- **Required reviews and artifacts:** one independent Security Reviewer and `docs/evidence/phase-6e1a-mcp-startup-capabilities.md`.
- **Explicit non-goals:** plan tokens, approval helper, destructive/broad/Git mutation, Attachment changes, UI.
- **Definition of done:** effective capabilities are explicit and correctly reported, invalid combinations fail closed, no operation consumes the new grants, tests/review/docs pass.
- **Expected commit boundary:** implementation/tests/ADR/packet/evidence/aligned docs; progress follows separately.
- **Conditions requiring escalation:** any client-supplied consent, implicit authority, helper execution, new dependency, or mutation added to this slice.
