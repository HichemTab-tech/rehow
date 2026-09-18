# Task Packet: Ranked full-content search slice

- **Objective:** Replace metadata-only filtering with deterministic backend ranking across every required Guide field.
- **Required reasoning level:** medium.
- **Prerequisites:** compatible read-only Workspace view, bound Guide model, safe Markdown AST, Reference Desk results UI.
- **Allowed files or subsystem:** core `search`; desktop Tauri search command; React search state/tests; this packet and progress.
- **Relevant specifications and ADRs:** product spec Search, architecture Search/index design, testing strategy Search relevance.
- **Exact required behavior:** normalize query text; preserve quoted phrases; require all terms; rank by the documented 100/95/75/70/55/45/40/35/25/10 policy; use status only as a close tie; exclude archived by default; cap query bytes, term count, and results; search title, aliases, command lines, affected/inline paths, error lines, Tags, Category, summary, headings, and body.
- **Important states and edge cases:** empty query stays local/unranked; unknown returned IDs are ignored; rapid queries cancel stale UI updates; phrases, Unicode/case, long input, archived scope, equal scores, and malformed isolated Guides are deterministic.
- **Security constraints:** read-only; no paths from the renderer, filesystem/process/network/write authority, raw content in errors/logs, or arbitrary regex. Guide commands remain data.
- **Tests to add or update:** relevance fixture proving title over body repetition; explicit alias/command/path/error fields; cross-field terms; quoted phrases; archived scope; caps; UI rank mapping.
- **Commands to run:** format, locked core search tests/Clippy, desktop Rust tests, pnpm check/Vitest/build, diff/bootstrap checks.
- **Required reviews and artifacts:** no extra agent for this bounded read-only checkpoint. The later index/cache and production benchmark gate remains mandatory.
- **Explicit non-goals:** persistent/disposable index, watcher-driven reindex, snippets/highlights, typo/fuzzy search, archived UI scope, performance acceptance.
- **Definition of done:** UI calls the bounded core-backed command after a short debounce, backend order drives results, ranking tests pass, and docs state that production budgets are not yet claimed.
- **Expected commit boundary:** core ranking + thin command + UI wiring/tests/packet.
- **Conditions requiring escalation:** a cache format or dependency becomes necessary, ranking policy must change, or production limits are claimed without the benchmark corpus.
