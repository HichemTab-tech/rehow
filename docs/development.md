# Development Guide

Status: active development through Phase 6D2
Last updated: 2026-09-20

## Prerequisites

- Git 2.x.
- Node 26.2.0, using the version already available on the project host by explicit user direction.
- pnpm 11.3.0, using the version currently available on the project host without installing or switching it, and pinned through the root `packageManager`.
- Current stable Rust toolchain pinned by `rust-toolchain.toml` once scaffolded. Available bootstrap host: Cargo/Rust 1.98.0.
- Linux Tauri/WebKitGTK build dependencies for the distribution.

Current stack baseline:

- Tauri and official plugins: major 2.
- React/ReactDOM: major 19.
- TypeScript: major 6.
- Vite: major 8; `@vitejs/plugin-react` compatible major.
- Vitest: major 5.
- Tailwind CSS and `@tailwindcss/vite`: major 4.
- shadcn CLI: major 4; generated component source is reviewed/committed.

Exact packages are chosen by Phase 1 packets and captured in lockfiles/Cargo.lock. Do not install global Tauri/shadcn CLIs or use floating `latest` in reproducible scripts.

Primary references checked 2026-09-05:

- [Tauri architecture](https://v2.tauri.app/concept/architecture/), [capabilities](https://v2.tauri.app/reference/acl/capability/), [CSP](https://v2.tauri.app/security/csp/), [dialog](https://v2.tauri.app/plugin/dialog/), [filesystem](https://v2.tauri.app/plugin/file-system/), [shell](https://v2.tauri.app/plugin/shell/), [Linux prerequisites](https://v2.tauri.app/start/prerequisites/).
- [Vite 8 announcement](https://vite.dev/blog/announcing-vite8), [Vitest guide](https://vitest.dev/guide/), [React versions](https://react.dev/versions), [TypeScript 6 notes](https://www.typescriptlang.org/docs/handbook/release-notes/typescript-6-0.html).
- [Tailwind Vite installation](https://tailwindcss.com/docs/installation/using-vite), [Tailwind compatibility](https://tailwindcss.com/docs/compatibility), [shadcn Vite installation](https://ui.shadcn.com/docs/installation/vite).
- [official Codex custom-agent configuration](https://learn.chatgpt.com/docs/agent-configuration/subagents), [AGENTS.md discovery](https://learn.chatgpt.com/docs/agent-configuration/agents-md).
- [Official MCP Rust SDK (`rmcp`)](https://github.com/modelcontextprotocol/rust-sdk) for the separately usable stdio server.
- [notify crate](https://docs.rs/notify/latest/notify/) for watch limitations.
- [Git status porcelain](https://git-scm.com/docs/git-status) and [Git configuration](https://git-scm.com/docs/git-config).

## Linux packages

Tauri's current Debian/Ubuntu prerequisites include WebKitGTK 4.1 headers, build tools, curl/wget/file, SSL headers, appindicator, librsvg, and libxdo packages. Use the official page for distribution-specific commands because package names change. The Arch developer environment must install equivalents explicitly; no project script runs the system package manager.

## Bootstrap workflow

Until Phase 1 creates scripts:

1. Read `AGENTS.md`, `PROGRESS.md`, current packet, and ADRs.
2. Verify `git status --short --branch` and recent history.
3. Confirm `node --version`, `pnpm --version`, `cargo --version`, and `rustc --version` match pins.
4. Run the exact commands from the packet.
5. Record actual results in `PROGRESS.md`.

After scaffolding, use root scripts documented in `AGENTS.md`. Never point a dev/test command at a personal Workspace.

## Project-agent setup

`.codex/config.toml` enables bounded project subagents. `.codex/agents/*.toml` are standalone project-scoped configurations following current official Codex syntax; every file sets `name`, `description`, `developer_instructions`, `model = "gpt-5.6-sol"`, and the role's effort. Do not modify `~/.codex` for this project. Run `python3 scripts/check_bootstrap.py` after changing durable planning or project-agent configuration.

`scripts/prepare-git-ssh-helper.sh dev|release` prepares both fixed Git subprocess siblings, `rehow-git-ssh` and `rehow-git-credential`, with one-link executable files beside the desktop binary. The historical script name is retained to avoid unnecessary command churn; it does not install or configure user credentials.

The vendored `.codex/skills/web-design-guidelines` skill is used for design reviews. Before each review, it requires fetching the current upstream guideline file; the repository copy describes that procedure.

## Native development

The Tauri CLI is a pinned project dependency invoked through pnpm. `pnpm tauri:dev` starts Vite as Tauri's development frontend only; the shipped application uses bundled assets and no persistent local server. Keep production CSP/capabilities exercised in native smoke tests because browser-only development does not prove the security boundary.

GUI apps may not inherit shell startup `PATH`; Git discovery and smoke tests must run from an application-menu-like environment. Wayland lacks reliable global window coordinates and some effects/always-on-top support, so developers must not encode those expectations.

## Test hygiene

- Rust `tempfile`/fixture helpers create unique Workspaces and Git repos under test temporary directories.
- The authenticated-SSH integration fixture requires the system OpenSSH client tools and `sshd`; it creates only disposable loopback keys, agents, repositories, and trust files.
- UI tests mock the typed IPC boundary.
- E2E passes a generated temporary Workspace through test-only launch configuration that cannot ship enabled.
- Failure injection uses explicit test seams, not sleeps or real power loss.
- Test Git config sets local identity in the temp repo and never reads/writes developer repositories.
- Secret canaries are fake and asserted absent from diagnostics.

For the standalone MCP server, use `cargo test -p rehow-mcp --all-features` for the focused gate. Tests exercise both supported protocol lifecycles through the bounded in-memory stdio transport and only temporary Workspace/state roots. The development binary requires one or more explicit `--workspace alias=/absolute/path` arguments and never defaults to the current directory. Add `--allow-write alias` only when deliberately testing the Phase 6C revision/snapshot-bound Category or Guide mutations; it is not implied by `--workspace`.

## Dependency changes

Workers do not add dependencies. A Planner packet and review must record reason, alternatives, maintenance, license, security/parser/native surface, binary size, and pinned major. Run relevant audits available in the repository; do not silently update unrelated packages.

## Web design review

For implemented UI:

1. Read `.codex/skills/web-design-guidelines/SKILL.md` fully.
2. Fetch its current upstream guideline source.
3. Review the requested files and report the mandated terse `file:line` findings.
4. Capture/inspect the matrix in `docs/design-direction.md`.
5. Record resolved/open design and accessibility findings in `PROGRESS.md`.

## Commit workflow

The Orchestrator inspects actual changes, runs packet gates, obtains independent review, commits one coherent unit, and updates `PROGRESS.md`. Do not push. Signed commits are used only if already configured and functional without new credentials or prompts.
