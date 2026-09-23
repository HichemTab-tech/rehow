# Phase 3 guarded external-link evidence

Date: 2026-09-23

## Result

Accepted.

Rehow renders retained safe Markdown links as inline semantic action buttons. A request is bound to the open Workspace session, Guide ID, exact revision, and exact URL retained by the reparsed safe AST. Core rereads the confined Guide before creating a URL-redacting plan. Execution accepts only the fixed XDG OpenURI portal, refuses interface versions below 3, and privately fixes `ask=true`; the renderer cannot bypass the native application chooser or select a process. Native success means only that the chooser request was accepted.

Unsafe/absent/file/custom links, changed Guides, stale sessions, duplicate requests, WebView navigation, arbitrary path/process/argv input, shell execution, URL-bearing native results, and MCP authority remain unavailable.

## Reviews

- Security review initially blocked `ask=false` because an untrusted renderer could trigger retained links without user intent. After the v3+ and mandatory-chooser correction, the same independent Security Reviewer returned PASS with no remaining blocker/high/medium finding.
- The project-pinned Web Interface Guidelines skill was reread and its current upstream rules were fetched on 2026-09-23. Static review passed: the control is a button because it invokes a native action rather than Web navigation; keyboard activation, inherited visible focus, hover, busy/disabled state, polite live feedback, and actionable failure focus are present. No screenshot rerun was required because this is inline text styling with no material reader geometry change.

## Verification

Final combined gate passed on 2026-09-23:

- `pnpm check`
- `pnpm test`: UI 47 passed; core 487 passed and 2 explicitly ignored; desktop 33 passed; MCP 92 passed; all remaining integration/contract tests passed
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- `cargo fmt --all --check`
- `pnpm build`
- `python3 scripts/check_bootstrap.py`: 17 required files, 55 ADRs, 8 pinned agents
- `git diff --check`
