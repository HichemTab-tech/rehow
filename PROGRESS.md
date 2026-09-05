# Rehow Progress

Last updated: 2026-09-06
State authority: actual Git/worktree first; this file summarizes verified facts.

## Current phase

Phase 1 — Core foundation and format proof.

## Current task

Checkpoint the final Phase 1A ledger amendment, then plan the Phase 1B domain/schema slice.

## Overall status

In progress. Phase 0 and Phase 1A are complete in signed, independently reviewed checkpoints. The monorepo scaffold builds natively with the user-approved installed Node 26.2.0, pnpm 11.20.0, and Rust 1.98.0. Phase 1B domain/schema planning is next; the current dirty tree contains only the final packet correction and this ledger update.

## Completed work

- Inspected repository/Git history/status: no initial files/commits/remotes.
- Inspected toolchain: Git 2.55.0, Node 26.2.0 host, pnpm 11.20.0, Rust/Cargo 1.98.0; no standalone global Tauri CLI (project-local CLI is planned).
- Checked current official Codex custom-agent and AGENTS.md configuration documentation.
- Installed the required Vercel web-design-guidelines skill project-locally and fetched/applied its current upstream planning rules.
- Researched current Tauri/React/Vite/Tailwind/shadcn/Linux/Wayland constraints and recorded sources.
- Selected Rust shared-core/thin-adapter architecture, versioned Markdown Workspace format, capability storage, revisions/recovery, derived field-aware search, safe Markdown AST, staged Git safety, standalone stdio MCP, and Reference Desk UI direction.
- Created the required specifications, threat/testing/install/development docs, task-packet template, orchestration protocol, and ADRs.
- Created 8 project-scoped roles in `.codex/agents/`; every role explicitly pins `gpt-5.6-sol` and its required effort.
- Parsed all 9 project TOML config files successfully with Python `tomllib`.
- Ran Architect/xhigh, Reviewer/high, Design Reviewer/high, and Security Reviewer/xhigh planning reviews. Corrected renderer authority, revision/inventory semantics, recovery placement and crash state machine, category/path behavior, tested-state authority, safe Git staging/sync, MCP capability/approval behavior, Attachment handling, parser/scan limits, and public error parity.
- Added canonical CBOR journal generations, strict primitive-dependent write refusal, fixed Attachment inventory caps, actual approval-helper descriptor confinement, and CommonMark-compatible fence validation in response to the second review round.
- Created signed Phase 0 checkpoint `da646a9760f349074838151e0b66f78f5196fe6b`; its configured GPG signature verifies successfully.
- Planner/medium created `docs/task-packets/phase-1a-scaffold.md` with frozen dependencies, exclusive paths, inert Tauri/MCP boundaries, generated-contract workflow, tests, commands, non-goals, and escalation conditions.
- Reviewer/high approved the Phase 1A packet for Worker/low with no blocker, high, or medium finding.
- Implemented the exact pnpm/Cargo topology, pinned manifests and lockfiles, deterministic Rust-to-TypeScript contract generation, inert React/Tailwind/Vite shell, empty standalone MCP boundary, and zero-command Tauri shell.
- Corrected the required Tauri compile placeholder, native dev-script path, pnpm 11 fail-on-switch configuration placement, and Biome preset syntax; both implementation closure reviews pass.
- Created signed Phase 1A implementation checkpoint `b8410d9afee5865b696df074955a9efe78cabc64`; its configured GPG signature verifies successfully.

## Work in progress

- Finalize and commit the Phase 1A packet/ledger correction that records pnpm 11's workspace-level configuration.

## Remaining work

- Phase 1B: implement domain IDs, shared `ErrorDto`, and bounded Workspace/Category/Guide schema parsing without mutation.
- Phase 1C onward: prove concrete-syntax preservation, confinement/revisions, and atomic recovery.
- Phase 2: native shell and safe Workspace onboarding/import/migration.
- Phase 3: Category/Guide/Attachment lifecycle and edit conflicts.
- Phase 4: search/ranking/index performance.
- Phase 5: staged safe Git workflows.
- Phase 6: standalone full MCP contract.
- Phase 7: Linux packaging, desktop integration, cross-environment hardening.

## Known bugs

None known in the Phase 1A scaffold. The shell is intentionally inert and is not yet product functionality.

## Design and accessibility findings

- Planning review selected Reference Desk and incorporated the current guideline rules.
- No implemented UI exists, so no source-level or screenshot pass has occurred.
- Phase 1A intentionally renders only the semantic `Rehow`/`Scaffold ready.` smoke state; formal UI design review remains a Phase 2 gate.
- First mandatory implementation review: after the complete Phase 2 application shell, with 4 target sizes, light/dark, keyboard focus, error/empty states, 200% scaling, and reduced motion.

## Security findings

- Highest risks: symlink/TOCTOU escape, frontmatter loss, concurrent lost updates, Git-triggered execution, Markdown/WebView injection/file disclosure, crash consistency, watcher/index staleness, duplicate IDs, MCP prompt injection/data overexposure.
- Baseline controls/required adversarial gates are documented; none has implementation evidence yet.
- Git commit/sync remain required but must follow read-only primitives and a dedicated malicious-repository gate. Unsafe shortcut implementation is explicitly blocked.
- Both security review rounds found specification gaps. All reported gaps are corrected; the Security Reviewer/xhigh closure pass reports no blocker, high, or medium finding.
- Phase 1A Security Reviewer/xhigh passes the actual implementation: exact dependencies/lockfiles, empty command manifest, one zero-permission capability, restrictive CSP, deterministic contract scripts, and zero-output MCP boundary.

## Blockers and human checkpoints

No human blocker. Writing `.codex/` required and received a scoped environment permission; no global Codex configuration was changed.

## Latest verified commit

`b8410d9afee5865b696df074955a9efe78cabc64` — signed, verified Phase 1A monorepo scaffold. The final packet/ledger wording in the current worktree follows it.

## Tests last run and results

- 2026-09-05: `python3` `tomllib` parsed `.codex/config.toml` plus all 8 `.codex/agents/*.toml`: pass (9 files).
- 2026-09-05: `python3 scripts/check_bootstrap.py`: pass — 17 required files, 10 ADRs, 8 pinned agents.
- 2026-09-05: Architect/xhigh closure review: pass — no blocker/high architecture finding.
- 2026-09-05: Reviewer/high Phase 0 content gate: pass — no blocker/high/medium finding.
- 2026-09-05: Security Reviewer/xhigh closure review: pass — no blocker/high/medium security finding.
- 2026-09-05: `git diff --cached --check` before Phase 0 commit: pass; exact 36-file staged snapshot inspected.
- 2026-09-05: `git verify-commit da646a9760f349074838151e0b66f78f5196fe6b`: pass — good signature from the configured project user key.
- 2026-09-05: post-commit `python3 scripts/check_bootstrap.py`: pass; Git worktree was clean before this progress-only update.
- 2026-09-06: host prerequisite check: `mise 2026.8.14` available; Rust 1.98.0 exact; active Node 26.2.0 and pnpm 11.20.0 do not satisfy the approved packet.
- 2026-09-06: Reviewer/high Phase 1A packet gate: pass — implementation-ready for Worker/low with no blocker/high/medium finding.
- 2026-09-06: `git verify-commit c0a2d4d5cbb92b7127a4dba82f3a8094229068d9`: pass — good signature from the configured project user key.
- 2026-09-06: user toolchain decision: use installed Node 26.2.0 and pnpm 11.20.0; do not install or switch runtimes. The interrupted `mise install` produced no active tool change.
- 2026-09-06: initial Worker/low run: frozen pnpm install, contract generation, `pnpm check`, and desktop Vitest 1/1 passed. Rust/Tauri compilation stopped at the missing default Unix icon; no out-of-scope file was added.
- 2026-09-06: resumed Worker/low run and Orchestrator verification: frozen install, contract checks, `pnpm check`, `pnpm test`, integration, production build, Cargo format/Clippy/tests, native Tauri release build, alternate-CWD generation, bootstrap check, and zero-byte MCP stdout all pass.
- 2026-09-06: Security Reviewer/xhigh implementation gate: pass — no blocker/high/medium finding.
- 2026-09-06: Reviewer/high implementation gate: changes required — missing desktop `dev` script and pnpm auto-switch refusal are medium; Biome config deprecation is low; Tauri `.app` suffix advisory is informational/Linux-nonblocking.
- 2026-09-06: correction verification: desktop Vite dev server reached ready state; `pnpm config list` reports `engineStrict: true` and `pmOnFail: error`; an isolated pnpm 11.20.1 mismatch was refused by pnpm 11.20.0 without download/switch; Biome check is warning-free; both lockfiles remained unchanged.
- 2026-09-06: Reviewer/high closure: pass — no blocker/high/medium finding.
- 2026-09-06: Security Reviewer/xhigh closure: pass — no blocker/high/medium security finding.
- 2026-09-06: Orchestrator full gate: `pnpm install --frozen-lockfile`, contracts check, `pnpm check`, `pnpm test` (Vitest 1/1; Rust smoke tests 3/3), `pnpm test:integration`, `pnpm build`, Cargo fmt/Clippy/tests, `pnpm tauri:build`, alternate-CWD contracts check, bootstrap validator, and MCP stdout `0` bytes: pass.
- 2026-09-06: `git verify-commit b8410d9afee5865b696df074955a9efe78cabc64`: pass — good signature from the configured project user key.
- 2026-09-05: tool/version and repository inspection: pass; expected no-commit `git log` failure observed.
- No application tests exist yet.

## Known test failures

None. `pnpm tauri:build` emits the packet-approved informational warning that `dev.rehow.app` ends in macOS's `.app` suffix; Linux no-bundle output succeeds, and identifier review remains available before packaging.

## Important files changed

- `AGENTS.md`, `PROGRESS.md`
- `.codex/config.toml`, `.codex/agents/*.toml`
- `.codex/skills/web-design-guidelines/SKILL.md`
- `docs/product-spec.md`, `docs/architecture.md`, `docs/content-model.md`
- `docs/design-direction.md`, `docs/mcp-spec.md`, `docs/testing-strategy.md`, `docs/threat-model.md`
- `docs/installation.md`, `docs/development.md`, `docs/implementation-plan.md`, `docs/orchestration.md`
- `docs/error-contract.md`, `docs/adr/0001` through `0010`, `docs/task-packets/TEMPLATE.md`
- `scripts/check_bootstrap.py`
- `docs/task-packets/phase-1a-scaffold.md` was approved at `c0a2d4d`, amended for the user-selected host toolchain at `d06d17a`, and amended for Tauri's compile icon at `9300ccd`; its final pnpm 11 setting placement and this ledger follow `b8410d9` in the current worktree.
- Phase 1A code/config: root pnpm/Cargo/toolchain/lock files, `apps/desktop/`, `crates/rehow-core/`, `crates/rehow-mcp/`, `packages/contracts/`, and contract scripts; checkpointed at `b8410d9`.

## Next recommended action

Commit the final Phase 1A packet/ledger wording, then have Planner/medium produce the exact Phase 1B domain/schema packet.

## Recommended agent role and reasoning effort

Planner — `gpt-5.6-sol`, medium, for Phase 1B packet creation; then Orchestrator/high readiness review.

## Explicit restart instructions

1. Read `AGENTS.md` and this file.
2. Read `docs/product-spec.md`, `docs/architecture.md`, `docs/implementation-plan.md`, `docs/threat-model.md`, `docs/error-contract.md`, and ADRs 0001–0010.
3. Run `git status --short --branch`, `git log --oneline -8`, and `python3 scripts/check_bootstrap.py`.
4. Confirm the Phase 0 commit/signature and reconcile any unexpected worktree changes.
5. Verify `b8410d9` and treat Phase 1A as accepted unless the actual tree/tests differ from this record.
6. Create/review `docs/task-packets/phase-1b-domain-schema.md`; do not assign Worker/low until every schema/parser/error dependency and test fixture is frozen.
