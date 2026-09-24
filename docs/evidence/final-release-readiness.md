# Final release readiness

Date: 2026-09-24

## Accepted product evidence

- Workspace detection, creation, safe Markdown copy import, Guide/Category/Tag/Attachment workflows, tested-state approval, external-change reconciliation, actionable Workspace issue guidance, search, Git, and MCP are implemented at the signed checkpoints recorded in `PROGRESS.md`.
- Dirty Guide editors now expose explicit Compare, Keep Draft, and confirmed Reload from Disk behavior. The current on-disk snapshot is read through retained core authority without mutating the open view.
- ADR 0058 adds pathless loaded-Guide Open File behavior: the core retains and verifies the exact read-only file descriptor and XDG Desktop Portal forces an application choice. The renderer has no path, URI, application, or writable authority.
- The required shadcn/ui foundation is now source-owned under `apps/desktop/`; reviewed Button/utility source uses exact locked dependencies while retaining the approved Reference Desk styling.
- Attachment preview, absent-only export, and descriptor-only parent reveal complete the approved Phase 3 Attachment surface. Rehow never executes or external-opens Attachment content.
- ADR 0057 closes the retained-memory gap: Guide bodies are revision-checked lazy reads and normalized search records are bounded anonymous disk-backed state. The deterministic 5,000-Guide/52.7 MiB benchmark measured 1.533 s cold indexing, 13.484 ms warm p95, and 10,228 KiB peak RSS.
- The complete repository gate passes after actionable Workspace issue reporting and the memory-safe search change: 489 pre-review core tests with 2 intentional network/performance ignores, all protocol/API/unknown-YAML suites, 33 desktop Rust tests, 92 MCP tests, 48 Vitest tests, strict Clippy/format/type/contract checks, and production Vite/Cargo builds. Post-review adversarial bound corrections pass 13 focused search tests and the final full gate is recorded in `PROGRESS.md`.
- The 2026-09-24 completion-audit deltas additionally pass 52 Vitest tests, all 37 desktop Rust tests, focused external-conflict and Guide-opener core tests, strict core/desktop Clippy, type/lint/contract checks, bootstrap validation with 58 ADRs, and the production Vite build. No earlier accepted MCP/Git/storage surface was changed by these deltas.
- Development `.deb` and `.rpm` artifacts, package contents, independent MCP execution, uninstall preservation, ABI floor, current-host menu resolution, Debian 13 installation, Fedora 44 installation, and Ubuntu 24.04 baseline compatibility have recorded evidence in `phase-7a-linux-bundles.md` and `PROGRESS.md`.
- Current visual evidence covers wide/narrow, light/dark, focus, 200% scaling, search, Git clone/commit/sync, and bounded Attachment actions. Fresh static guideline reviews also cover the external-conflict and Open File controls; current web-interface guideline reviews pass.

## Remaining technical acceptance

1. Run the installed package on a clean graphical Ubuntu 24.04 baseline through both supported Wayland and X11 sessions without a terminal-provided environment, then record WebKitGTK/graphics results. The current host needs the invocation-scoped `WEBKIT_DISABLE_DMABUF_RENDERER=1` workaround for Wayland and therefore cannot close this matrix alone.
2. On that disposable graphical system, complete one install-to-recovery journey: create/open/import, edit/search/copy, Git unavailable and available states, MCP handshake, restart, external conflict, documented recovery, and uninstall with Workspace bytes preserved.
3. Complete the final keyboard-only and screen-reader smoke on the installed native build. Automated semantic/focus/live-region checks and prior screenshot reviews do not replace this manual gate.
4. After those results, perform the final combined security, accessibility, visual, installation, and documentation audit. Do not rerun accepted subsystem reviews unless the release correction changes them.

## Human checkpoints

- Choose the project license. Rehow cannot invent redistribution rights.
- Version `0.1.0`, stable identifier `me.hichemtab-tech.rehow`, and first-release `.deb` plus `.rpm` targets are selected; AppImage remains deferred unless separately qualified.
- Provide explicit authorization and the required credentials only when artifact signing or publication is requested. No push, release, account/repository creation, signing, or publication is authorized by this audit.

## Efficient restart

Do not reopen completed feature phases. Resume with the project-license choice and access to one disposable graphical Ubuntu 24.04 environment. If that environment is unavailable, report release qualification as blocked rather than applying a global graphics workaround without evidence.
