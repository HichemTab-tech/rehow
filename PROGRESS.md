# Rehow Progress

Last updated: 2026-09-06
State authority: actual Git/worktree first; this file summarizes verified facts.

## Current phase

Phase 1 — Core foundation and format proof.

## Current task

Create the signed protocol/YAML decision checkpoint, then complete a separate Architect/xhigh Phase 1B construction/dependency decision before asking Planner/medium for a Worker packet.

## Overall status

In progress. Phase 0 and Phase 1A are complete in signed, independently reviewed checkpoints. Planner/medium correctly blocked Phase 1B instead of inventing public schema choices. The reviewed protocol/YAML baseline narrows Phase 1B to protocol primitives and directly constructed parser-neutral values, while product aggregate schemas are gated for Phase 1D. ADR 0011 is accepted after Architect, Reviewer, and Security Reviewer closure. No domain implementation has begun, and a separate Architect decision must still freeze Phase 1B construction semantics and dependencies before planning.

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
- Planner/medium escalated rather than guessing Error detail types, bounded-list counts, signed 64-bit JSON transport, unknown YAML semantics, or Category requiredness.
- Xhigh architecture analysis selected code-correlated error variants, 32-item exact-count bounded lists, decimal-string JSON for 64-bit values, separate storage/protocol types, a parser-neutral YAML 1.2 semantic tree, and a dual bounded-semantic/lossless-span parser design.
- Independent review narrowed Phase 1B away from underspecified Workspace/Category/Guide/Attachment aggregates and moved typed known-field binding/mutation authority behind a Phase 1D Architect gate.
- Accepted ADR 0011 with exact pinned dual-parser boundaries, generic-versus-typed authority separation, source/decoded scalar reconciliation, implicit-empty handling, non-bypassable semantic construction, algorithm-independent numeric charging, deterministic semantic-key work, and lossless span-edit/refusal requirements.

## Work in progress

- Create and verify the signed documentation checkpoint for the accepted protocol/content/error baseline and ADR 0011.

## Remaining work

- Phase 1B prerequisites: Architect/xhigh must freeze exact dependencies/manual implementations, primitive construction/visibility, timestamp rules, identifier parsing/generation, canonical token validation, and the fixed public message/recovery catalog.
- Phase 1B: implement the accepted protocol/domain value types and validation over directly constructed semantic values only; no Workspace files, YAML, Markdown, or CST parsing.
- Phase 1C: implement the accepted bounded generic YAML/presentation parser and reconciliation artifact; Phase 1D binds typed aggregate fields and mutation authority; Phase 1E proves confinement/revisions and atomic recovery.
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

- No active human blocker. The prior read-only `.git` session ended without staging or changing the index; the current session restores repository write access.
- Writing `.codex/` previously required and received a scoped environment permission; no global Codex configuration was changed.

## Latest verified commit

`98f4bc391b4b11f9997e8bd17509177bf954c005` — signed Phase 1A verification ledger over the verified scaffold. The accepted protocol/parser specification work follows it and is staged for its signed decision checkpoint.

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
- 2026-09-06: `git verify-commit 98f4bc391b4b11f9997e8bd17509177bf954c005`: pass; post-commit bootstrap and `pnpm check` pass on a clean tree.
- 2026-09-06: Planner/medium Phase 1B attempt: safely blocked with no file written because five public schema choices were not normative.
- 2026-09-06: Contract Architect/xhigh and Parser Architect/xhigh: completed read-only recommendations; proposed spec/ADR changes are not yet accepted.
- 2026-09-06: decision Reviewer/high closure after scope/numeric corrections: pass — no blocker/high/medium finding.
- 2026-09-06: current `git diff --check`, untracked ADR whitespace check, and `python3 scripts/check_bootstrap.py`: pass — 17 required files, 11 ADRs, 8 pinned agents.
- 2026-09-06: current `cargo test --workspace --all-features`: pass — 3 smoke tests. Direct `pnpm exec biome check .`, both package type checks, and `pnpm contracts:check`: pass.
- 2026-09-06: aggregate `pnpm check` cannot complete in the resumed managed sandbox because its Node engine-check subprocess receives `spawnSync pnpm EPERM`; direct `node --version` and `pnpm --version` remain the approved 26.2.0/11.20.0. This is an environment restriction, not a changed dependency or product-test failure.
- 2026-09-06: final protocol/YAML decision gate: Architect/xhigh, Reviewer/high, and Security Reviewer/xhigh pass with no blocker/high/medium finding after exact parser-source, construction-visibility, phase-authority, PlanToken, numeric-work, and semantic-work corrections.
- 2026-09-06: final checkpoint validation: `git diff --check`, bootstrap validation, `cargo fmt --all --check`, and `cargo test --workspace --all-features` pass. `git add` then failed before staging because `.git/index.lock` cannot be created in the read-only Git metadata mount.
- 2026-09-06: resumed unrestricted session: installed Node 26.2.0 and pnpm 11.20.0 confirmed without installation or switching; full `pnpm check`, `git diff --check`, bootstrap validation, Cargo formatting, and all-feature Workspace tests pass. Git metadata write access is restored.
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
- Current staged specification work: `docs/error-contract.md`, `docs/content-model.md`, `docs/architecture.md`, `docs/testing-strategy.md`, `docs/implementation-plan.md`, and accepted `docs/adr/0011-bounded-yaml-semantics-and-lossless-span-edits.md`.

## Next recommended action

Stage only the seven documented decision files, inspect the cached diff, create/verify the signed protocol/YAML decision checkpoint, and confirm a clean tree. Then have Architect/xhigh freeze the separate Phase 1B construction/dependency decision; only after that passes should Planner/medium produce the narrower Worker packet.

## Recommended agent role and reasoning effort

Orchestrator — `gpt-5.6-sol`, high, to create and verify the signed checkpoint; then Architect/xhigh for the Phase 1B construction/dependency decision.

## Explicit restart instructions

1. Read `AGENTS.md` and this file.
2. Read `docs/product-spec.md`, `docs/architecture.md`, `docs/implementation-plan.md`, `docs/threat-model.md`, `docs/error-contract.md`, and ADRs 0001–0010.
3. Run `git status --short --branch`, `git log --oneline -8`, and `python3 scripts/check_bootstrap.py`.
4. Confirm the Phase 0 commit/signature and reconcile any unexpected worktree changes.
5. Verify `98f4bc3` and treat Phase 1A as accepted unless the actual tree/tests differ from this record.
6. Stage only `PROGRESS.md`, `docs/architecture.md`, `docs/content-model.md`, `docs/error-contract.md`, `docs/implementation-plan.md`, `docs/testing-strategy.md`, and `docs/adr/0011-bounded-yaml-semantics-and-lossless-span-edits.md`; inspect `git diff --cached`, commit with configured signing, and verify the signature.
7. Do not begin Phase 1B code from the decision docs alone. Complete a separate Architect/xhigh decision for Phase 1B dependencies, timestamp/identifier/token construction, public/private constructors, and fixed public text before asking Planner/medium for a precise Worker packet.
