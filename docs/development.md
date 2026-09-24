# Development Guide

Status: active development through completion audit
Last updated: 2026-09-24

## Prerequisites

- Git 2.x.
- Node 26.2.0, using the version already available on the project host by explicit user direction.
- pnpm 11.20.0, using the version currently available on the project host without installing or switching it, and pinned through the root `packageManager`.
- Current stable Rust toolchain pinned by `rust-toolchain.toml` once scaffolded. Available bootstrap host: Cargo/Rust 1.98.0.
- Linux Tauri/WebKitGTK build dependencies for the distribution.

Current stack baseline:

- Tauri and official plugins: major 2.
- React/ReactDOM: major 19.
- TypeScript: major 6.
- Vite: major 8; `@vitejs/plugin-react` compatible major.
- Vitest: major 5.
- Tailwind CSS and `@tailwindcss/vite`: major 4.
- shadcn/ui: project-owned `components.json`, reviewed component source, and exact locked utility dependencies; no global CLI installation.

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

## Hosted CI and releases

`.github/workflows/ci.yml` runs the full Linux check, test, production package, and bundle-smoke gate for pull requests, pushes to `master`, and manual dispatch. Ubuntu 24.04 hosted runners restrict unprivileged user-namespace capabilities through AppArmor by default, so the workflow enables the documented ephemeral runner controls and requires a real networkless Bubblewrap preflight before any project gate. The preflight and production sandboxes preserve `/usr/lib64` so Ubuntu and Arch dynamic loaders remain reachable. OpenSSH client/server packages are explicit test dependencies; the loopback fixture accepts only Ubuntu's fixed `/usr/sbin/sshd` or Arch's fixed `/usr/bin/sshd`. No sandbox test is skipped. `.github/workflows/release.yml` rebuilds the exact matching version tag and uploads `.deb`, `.rpm`, and checksums only after a GitHub Release is published. The build job is read-only and isolated from the separate write-authorized upload job. Exact maintainer steps, retry behavior, platform limits, and signing exclusions are in `docs/release.md` and ADR 0059.

## Test hygiene

- Rust `tempfile`/fixture helpers create unique Workspaces and Git repos under test temporary directories.
- The authenticated-SSH integration fixture requires the system OpenSSH client tools and `sshd`; it creates only disposable loopback keys, agents, repositories, and trust files.
- UI tests mock the typed IPC boundary.
- E2E passes a generated temporary Workspace through test-only launch configuration that cannot ship enabled.
- Failure injection uses explicit test seams, not sleeps or real power loss.
- Test Git config sets local identity in the temp repo and never reads/writes developer repositories.
- Secret canaries are fake and asserted absent from diagnostics.

For the standalone MCP server, use `cargo test -p rehow-mcp --all-features` for the focused gate. Tests exercise both supported protocol lifecycles through the bounded in-memory stdio transport and only temporary Workspace/state roots. The development binary requires one or more explicit `--workspace alias=/absolute/path` arguments and never defaults to the current directory. Add `--allow-write alias` only for deliberate mutations; it is not implied by `--workspace`. Managed Git commit and safe sync/recovery additionally require `--allow-git-write alias`, a trusted packaged approval helper, a writable private state root, and native confirmation. Sync requires separate approval before remote inspection and before mutation; it uses only the already configured upstream and launch-captured normal Git/SSH credential facilities. It never accepts credentials, paths, remotes, refs, Git arguments, force, reset, merge, rebase, or conflict-resolution authority from MCP.

Ordinary development builds deliberately report the native MCP approval helper as unconfigured. Release packaging must first build `rehow-mcp-approve`, compute its exact lowercase BLAKE3 digest, then compile only `rehow-mcp` with `REHOW_MCP_APPROVAL_HELPER_BLAKE3` set to that digest; the unchanged helper and server are installed as one-link siblings. Do not smoke directly from Cargo's hard-linked `target/release` artifacts because the provenance gate intentionally rejects link counts other than one. Phase 7 packaging automation must reproduce and smoke-test this ordering rather than accepting a runtime digest or path.

Phase 6E2A planning remains available in an ordinary development build, but executable Category reorder tokens require that digest-bound sibling. Unit and JSON-RPC tests use a test-only in-memory approval outcome; production code has no such override.

Phase 6E2B Category-removal preview is likewise available without mutation authority. Executable removal tokens require startup write+broad grants and the same packaged helper; the operation is limited to one empty, non-default Category and uses recoverable trash.

Phase 6E2C Guide-tested preview is available in read-only sessions. Executable Tested tokens require only the alias's startup write grant plus the trusted helper; broad authority is deliberately irrelevant. The execution call accepts only the handle/token and the core recalculates readiness, verification hash, and timestamps from the exact current bytes.

Phase 6E2D1 adds `attachment_metadata_update` for the title of an already-declared managed Attachment. It requires `--allow-write`; it does not require or consume broad authority and cannot transfer or select files.

Phase 6E2D2 adds `attachment_remove_plan` and `attachment_remove`. Planning works without mutation authority; executable tokens require both `--allow-write <alias>` and `--allow-broad <alias>` plus the trusted packaged approval helper and retained recovery directories.

Current client configuration and the exact target AI workflow are documented in `docs/mcp-setup.md`. Keep those examples synchronized with the strict CLI parser and real tool names.

After building the helper and digest-bound server, run `node scripts/smoke-mcp-category-reorder.mjs` in a graphical session. The script copies both binaries into a temporary one-link package layout, creates only temporary Workspace/state roots, and requires one explicit **Approve** followed by one explicit **Cancel**; it proves the approved reorder and byte-identical denial path, then removes its temporary data.

For Phase 6E2B run `node scripts/smoke-mcp-category-remove.mjs` with the same package build. It requires one **Approve** and one **Cancel**, proves recoverable removal plus byte-identical denial, and deletes its temporary fixture.

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
