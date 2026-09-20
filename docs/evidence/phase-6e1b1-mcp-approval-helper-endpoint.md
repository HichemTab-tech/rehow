# Phase 6E1B1 MCP approval-helper evidence

Date: 2026-09-20

The separately built `rehow-mcp-approve` endpoint has no arguments and no Workspace or mutation authority. Unit tests reject every closed-kind/framing/length/UTF-8/control/bidi/visual-separator/invisible-format/line/trailing violation, including the complete Unicode 16 Format plus Default-Ignorable set. The dialog uses direct in-process GTK rather than `rfd`'s `PATH`-resolved Zenity fallback, and both approval pipes become close-on-exec before GTK initialization. Real-process tests prove missing private pipes fail silently, a valid request in a headless environment returns no approval byte, stdin/stdout/stderr resolve to `/dev/null`, and an intentionally inherited hostile descriptor is removed so `/proc/<pid>/fd` contains exactly 0–4 while the helper awaits its request. A hostile-`PATH` regression places a fake Zenity first and proves it is never executed and cannot forge a result.

At this checkpoint no server launcher or MCP tool could reach the helper. Phase 6E1B2 subsequently adds a still-internal verified launcher and protocol fixtures; real positive/negative native-button behavior remains a manual display-session gate before the first MCP approval tool is exposed.

Independent Security Reviewer result: **PASS** after two findings were corrected: the first implementation's `PATH`-resolved dialog fallback was replaced with direct GTK, and the Unicode denylist was completed to cover visual separators and invisible/format characters.
