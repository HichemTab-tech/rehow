# Rehow Progress

Last updated: 2026-09-06
State authority: actual Git/worktree first; this file summarizes verified facts.

## Current phase

Phase 1 — Core foundation and format proof.

## Current task

Checkpoint the user-approved host-toolchain amendment, then delegate the Phase 1A scaffold packet to Worker/low.

## Overall status

In progress. Phase 0 is complete in a clean, signed, independently reviewed checkpoint. The Phase 1A monorepo-scaffold packet is complete and independently approved for Worker/low. The user explicitly directed Rehow to use the machine's already installed Node 26.2.0 and pnpm 11.20.0; no runtime installation or switch is permitted. Rust 1.98.0 is also available.

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

## Work in progress

- Orchestrator is reconciling the explicit host-toolchain override into the approved packet before Worker handoff.

## Remaining work

- Phase 1A: review the scaffold packet, delegate it to Worker/low, inspect implementation, run every packet check, obtain independent review, and commit the coherent scaffold.
- Phase 1B onward: prove content format/confinement/revision/atomic recovery.
- Phase 2: native shell and safe Workspace onboarding/import/migration.
- Phase 3: Category/Guide/Attachment lifecycle and edit conflicts.
- Phase 4: search/ranking/index performance.
- Phase 5: staged safe Git workflows.
- Phase 6: standalone full MCP contract.
- Phase 7: Linux packaging, desktop integration, cross-environment hardening.

## Known bugs

None yet; application code does not exist.

## Design and accessibility findings

- Planning review selected Reference Desk and incorporated the current guideline rules.
- No implemented UI exists, so no source-level or screenshot pass has occurred.
- First mandatory implementation review: after the complete Phase 2 application shell, with 4 target sizes, light/dark, keyboard focus, error/empty states, 200% scaling, and reduced motion.

## Security findings

- Highest risks: symlink/TOCTOU escape, frontmatter loss, concurrent lost updates, Git-triggered execution, Markdown/WebView injection/file disclosure, crash consistency, watcher/index staleness, duplicate IDs, MCP prompt injection/data overexposure.
- Baseline controls/required adversarial gates are documented; none has implementation evidence yet.
- Git commit/sync remain required but must follow read-only primitives and a dedicated malicious-repository gate. Unsafe shortcut implementation is explicitly blocked.
- Both security review rounds found specification gaps. All reported gaps are corrected; the Security Reviewer/xhigh closure pass reports no blocker, high, or medium finding.

## Blockers and human checkpoints

No human blocker. Writing `.codex/` required and received a scoped environment permission; no global Codex configuration was changed.

## Latest verified commit

`c0a2d4d5cbb92b7127a4dba82f3a8094229068d9` — signed, verified Phase 1A packet checkpoint. The host-toolchain amendment in the current worktree follows it.

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
- 2026-09-05: tool/version and repository inspection: pass; expected no-commit `git log` failure observed.
- No application tests exist yet.

## Known test failures

None. Required application/test infrastructure is not implemented and is not counted as passing.

## Important files changed

- `AGENTS.md`, `PROGRESS.md`
- `.codex/config.toml`, `.codex/agents/*.toml`
- `.codex/skills/web-design-guidelines/SKILL.md`
- `docs/product-spec.md`, `docs/architecture.md`, `docs/content-model.md`
- `docs/design-direction.md`, `docs/mcp-spec.md`, `docs/testing-strategy.md`, `docs/threat-model.md`
- `docs/installation.md`, `docs/development.md`, `docs/implementation-plan.md`, `docs/orchestration.md`
- `docs/error-contract.md`, `docs/adr/0001` through `0010`, `docs/task-packets/TEMPLATE.md`
- `scripts/check_bootstrap.py`
- `docs/task-packets/phase-1a-scaffold.md` was checkpointed at `c0a2d4d`; its toolchain pins plus `docs/architecture.md`, `docs/development.md`, and this ledger are amended in the current worktree by explicit user direction.

## Next recommended action

Validate and checkpoint the user-approved host-toolchain amendment, then delegate the exact packet to Worker/low without installing runtimes.

## Recommended agent role and reasoning effort

Orchestrator — `gpt-5.6-sol`, high, for the toolchain amendment and handoff; Worker — `gpt-5.6-sol`, low, for the approved packet.

## Explicit restart instructions

1. Read `AGENTS.md` and this file.
2. Read `docs/product-spec.md`, `docs/architecture.md`, `docs/implementation-plan.md`, `docs/threat-model.md`, `docs/error-contract.md`, and ADRs 0001–0010.
3. Run `git status --short --branch`, `git log --oneline -8`, and `python3 scripts/check_bootstrap.py`.
4. Confirm the Phase 0 commit/signature and reconcile any unexpected worktree changes.
5. Read the independently approved `docs/task-packets/phase-1a-scaffold.md` and obey its exclusive file list/version pins.
6. Confirm active Node 26.2.0 and pnpm 11.20.0 without installing either, then delegate the exact packet to Worker/low; inspect its actual diff and run every gate before integration.
