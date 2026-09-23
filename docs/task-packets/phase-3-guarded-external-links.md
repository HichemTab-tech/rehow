# Task Packet: Guarded external Guide links

- **Objective:** Open only a safe external link present in the exact displayed Guide revision through the desktop portal.
- **Required reasoning level:** high for the native trust boundary; low for frozen reader wiring.
- **Prerequisites:** ADR 0005 safe Markdown AST; ADR 0055; accepted Workspace session/revision boundary; GIO 0.18.4 already pinned.
- **Allowed files or subsystem:** `crates/rehow-core/src/{application.rs,markdown.rs}`, `apps/desktop/src-tauri/src/lib.rs`, `apps/desktop/src/{App.tsx,MarkdownDocument.tsx,app.test.ts,index.css}`, relevant docs/evidence/progress. No dependency or Tauri capability change.
- **Relevant specifications and ADRs:** `docs/product-spec.md`, `docs/architecture.md`, `docs/threat-model.md`, ADRs 0005 and 0055.
- **Exact required behavior:** Accept Workspace session, Guide ID, exact revision, and a maximum-2,048-byte destination; core must prove an exact non-null destination in the safe Markdown AST and reread the confined Guide revision; an opaque plan requires portal `OpenURI` v3+ with `ask=true`; return only `{ requested: true }` after the interactive chooser request is accepted.
- **Important states and edge cases:** missing/stale/replaced Guide, unsafe or absent destination, duplicate link, malformed Markdown, unavailable session bus/portal/handler, Workspace or selection change, duplicate request, stale response.
- **Security constraints:** No WebView navigation, `href`, path/file/custom URI, arbitrary opener/argv, shell, content logging, URL-bearing error/Debug, prefetch, automatic open, mutation, or MCP surface.
- **Tests to add or update:** Core exact-link/stale/unsafe/redaction and mandatory-chooser-option tests; native closed result/error/single-flight tests; React strict response and semantic keyboard button rendering tests.
- **Commands to run:** Focused core/native/Vitest and strict Clippy while editing; one combined repository check/test/build/format/bootstrap/whitespace gate.
- **Required reviews and artifacts:** Fresh Web Interface Guidelines static review and one final Security Reviewer PASS. No screenshot unless reader geometry changes materially.
- **Explicit non-goals:** Internal anchors, relative/file links, media, embedded browsing, link previews, Rehow-owned handler selection, MCP open, or claiming chooser selection/external-handler success.
- **Definition of done:** Only exact current safe links reach the fixed portal, IPC and errors remain closed, stale/raced state fails closed, UI is keyboard-accessible, all gates/reviews pass, and signed feature/ledger checkpoints exist.
- **Expected commit boundary:** ADR/packet, core plan+portal, native adapter, reader UI/tests, threat/design evidence; Orchestrator owns the commit.
- **Conditions requiring escalation:** Any need for WebView navigation, new scheme, file access, renderer-selected process/path, shell, new dependency/capability, or destination disclosure outside the already displayed Guide.
