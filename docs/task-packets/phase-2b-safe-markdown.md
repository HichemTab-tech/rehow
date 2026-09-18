# Task Packet: Safe Markdown reader

- **Objective:** Render opened Guide bodies as useful CommonMark/GFM without executable HTML, navigation, or remote media.
- **Required reasoning level:** high (security boundary and one dependency); implementation otherwise bounded.
- **Prerequisites:** ADR 0005, strict Reference URL validation, read-only Workspace/Guide IPC, Reference Desk shell.
- **Allowed files or subsystem:** `rehow-core` Markdown and URL-policy visibility; generated contracts; desktop Tauri read command; React reader/CSS/tests; locks; this packet/ADR/content-model/progress.
- **Relevant specifications and ADRs:** ADRs 0001/0005, `docs/threat-model.md`, and `docs/content-model.md` limits.
- **Exact required behavior:** Rust emits version-1 typed balanced events for CommonMark plus tables, strikethrough, and tasks. React maps known nodes; headings nest below the Guide title. Code blocks have a user-gesture copy button. Raw HTML is literal, images have no source, and links have no navigation until a guarded native opener exists.
- **Important states and edge cases:** empty source; malformed event tolerance in React; HTML blocks/inline HTML; blocked schemes and relative URLs; remote images; long/deep documents; unsupported AST version; oversized ordered-list starts.
- **Security constraints:** no `dangerouslySetInnerHTML`, `href`, image `src`, opener, file/network/process capability, logging, or command execution. Reuse the strict canonical URL policy. Cap 100,000 semantic nodes and 128 container frames.
- **Tests to add or update:** Rust node coverage, HTML literal, URL allow/block corpus, no image-source serialization, one-over node/depth limits; React escaping, absent `href`/`src`, unsupported version, and code-copy control.
- **Commands to run:** format; generated-contract check; desktop typecheck/Vitest/build; locked core tests/Clippy; desktop Rust check; diff/bootstrap checks.
- **Required reviews and artifacts:** self-audit now; one independent Security Reviewer at the next major checkpoint because quota prevented the existing review. Visual capture can share the next real-data UI gate.
- **Dependency rationale:** `pulldown-cmark 0.13.4` is the maintained Rust CommonMark parser, MIT licensed, Rust 1.71.1 floor, no runtime I/O, and forbids unsafe code with selected features. Default HTML-renderer/CLI/SIMD features are disabled; reachable additions are `unicase` plus already-used `bitflags`/`memchr`. Its no-op build script cannot generate files without disabled `gen-tests`. Source size is under 1 MiB. A custom parser is higher security/maintenance risk; a JS parser violates the Rust-authoritative AST boundary.
- **Explicit non-goals:** editing, syntax highlighting, opening links/attachments, Mermaid/MDX/math/footnotes, sanitizing HTML into live DOM, lazy rereads/watchers.
- **Definition of done:** typed contracts are generated, Guide IPC returns AST not raw Markdown, focused/full relevant gates pass, docs match behavior, and the commit excludes unrelated `x`.
- **Expected commit boundary:** parser + contract + thin adapter + fixed React renderer + tests/docs in one checkpoint.
- **Conditions requiring escalation:** parser needs filesystem/network/unsafe authority; limits or URL policy must loosen; CSP/opener scope changes; a required gate fails outside this packet.
