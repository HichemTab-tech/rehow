# Task Packet: First Guide body editor vertical slice

- **Objective:** Let the native desktop app edit and safely save the body of an existing Guide.
- **Required reasoning level:** high; native mutation and interaction boundary.
- **Prerequisites:** signed private update checkpoint `e28cf48`, Reference Desk shell, safe Markdown reader, ADR 0009.
- **Allowed files or subsystem:** bounded public core application facade, desktop Tauri state/command, React Reference Desk/editor styles/tests, this packet, and progress.
- **Relevant specifications and ADRs:** product/content/design/threat/testing docs; ADRs 0003/0009/0014; current web-interface guidelines.
- **Exact required behavior:** native core generates operation ID/time; body read returns raw Markdown plus revision; writable compatible Workspace offers Edit; Save sends Guide ID/revision/body only; core performs the accepted update; response refreshes Workspace and reader; Cancel/navigation warns before discarding dirty text.
- **Important states and edge cases:** loading, clean edit, dirty edit, saving, no-op, invalid/oversize body, stale revision/external edit, tested-to-stale, committed-refresh-required, read-only Workspace, save failure, narrow window.
- **Security constraints:** no caller path, YAML, status, timestamp, operation ID, or tested authority; bounded body; no HTML execution; no MCP mutation; content-free actionable errors; XDG recovery paths stay native-owned.
- **Tests to add or update:** native request/response shape, public facade generation path, editor semantics/copy, dirty-navigation guard, loading/error/saving copy, status refresh, type/build/native tests.
- **Commands to run:** format; core/desktop Rust tests and Clippy; pnpm check/test/build; native Tauri build; contracts/bootstrap/diff checks.
- **Required reviews and artifacts:** apply the current web-design guidelines to final UI; one combined security/design review after implementation.
- **Explicit non-goals:** title/summary editor, Guide creation/delete/move, Category/Attachment lifecycle, tested approval, Git, MCP writes.
- **Definition of done:** an existing writable Guide can be edited and saved from Rehow with atomic recovery, stale conflict refusal, accessible dirty-state behavior, refreshed reading/search state, and green gates.
- **Expected commit boundary:** core facade, Tauri command, editor UI/tests/docs, then progress.
- **Conditions requiring escalation:** native-owned operation/time/state authority cannot remain bounded, committed outcome is ambiguous, or UI must accept raw paths/YAML/status to proceed.
