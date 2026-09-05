# Rehow Progress

Last updated: 2026-09-05
State authority: actual Git/worktree first; this file summarizes verified facts.

## Current phase

Phase 0 — Bootstrap foundation.

## Current task

Review, validate, and commit the planning foundation; then create the Phase 1A scaffold packet.

## Overall status

In progress. The repository began empty with no commits. The required planning/specification set, ten-ADR baseline, public error contract, project-local design skill, project-scoped Codex roles, and bootstrap checker exist in the working tree. Architect/xhigh, Reviewer/high, Design Reviewer/high, and Security Reviewer/xhigh gates pass after two correction rounds. The content is ready to stage; the first checkpoint commit has not yet occurred.

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

## Work in progress

- Stage and inspect the exact Phase 0 tree.
- Create the signed planning checkpoint commit.

## Remaining work

- Phase 0: obtain clean closure verdicts, inspect/stage the exact tree, commit, and produce a ready Phase 1A packet.
- Phase 1: scaffold and prove content format/confinement/revision/atomic recovery.
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

None — repository has no commits yet. Current claims apply to a dirty, uncommitted planning tree.

## Tests last run and results

- 2026-09-05: `python3` `tomllib` parsed `.codex/config.toml` plus all 8 `.codex/agents/*.toml`: pass (9 files).
- 2026-09-05: `python3 scripts/check_bootstrap.py`: pass — 17 required files, 10 ADRs, 8 pinned agents.
- 2026-09-05: Architect/xhigh closure review: pass — no blocker/high architecture finding.
- 2026-09-05: Reviewer/high Phase 0 content gate: pass — no blocker/high/medium finding.
- 2026-09-05: Security Reviewer/xhigh closure review: pass — no blocker/high/medium security finding.
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

## Next recommended action

Validate links/TOML/model pins once more, inspect the staged diff, and create the signed Phase 0 checkpoint commit. Then have Planner create `docs/task-packets/phase-1a-scaffold.md`.

## Recommended agent role and reasoning effort

Orchestrator — `gpt-5.6-sol`, high; create the checkpoint, then delegate Phase 1A packet creation to Planner/medium.

## Explicit restart instructions

1. Read `AGENTS.md` and this file.
2. Read `docs/product-spec.md`, `docs/architecture.md`, `docs/implementation-plan.md`, `docs/threat-model.md`, `docs/error-contract.md`, and ADRs 0001–0010.
3. Run `git status --short --branch`, `git log --oneline -8`, and `python3 scripts/check_bootstrap.py`.
4. Inspect the actual planning diff; do not assume Phase 0 is committed.
5. Treat the recorded closure reviews as passed unless the actual tree differs from this record.
6. Commit only when Phase 0 gate is satisfied; update this file with the real commit/tests and proceed to the Phase 1A packet.
