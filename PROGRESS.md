# Rehow Progress

Last updated: 2026-09-18
State authority: actual Git/worktree first; this file summarizes verified facts.

## Current phase

Phase 1E-A / 2B — read-only Workspace implementation checkpoint; independent security acceptance remains open.

## Current task

Checkpoint the implemented descriptor-confined Workspace picker/list/read vertical slice, then complete its independent Security Reviewer and native portal/manual UI gates when agent quota and an interactive display are available. Continue routine work single-agent with writable-state/onboarding classification or safe Markdown rendering; advanced Phase 1D-C2b remains required before edits.

## Overall status

In progress. Phase 0 through Phase 1C and Phase 1D-A/B are accepted; Phase 1D-C1/C2a are checkpointed. The Reference Desk shell now has a tested real-data read path: a native no-path-input folder picker, retained descriptor capability, bounded current-schema scan, Category/Guide lists, and per-Guide body reads. Its full Cargo/pnpm/native build gate is green. This Phase 1E-A work is an implementation checkpoint, not accepted phase closure, because the single Security Reviewer attempt could not start after the agent usage quota was exhausted and manual portal/real-data visual checks remain.

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
- Created signed protocol/YAML decision checkpoint `a0a5a0b4a782167368298025887f3642bd580633`; its configured GPG signature verifies successfully.
- Accepted ADR 0012 after Architect/xhigh, Reviewer/high, and Security Reviewer/xhigh review. It freezes the exact Phase 1B dependency edges; parse-only UUIDv7, timestamp, decimal, digest, and plan-token values; safe-display validation; all 36 correlated errors and generated TypeScript declarations; producer-versus-consumer text authority; and the only bounded parser-neutral unknown-YAML construction API.
- Corrected review findings covering `INDEX_NOT_READY` retryability, error-copy compatibility, deterministic bounded-list prefixes, nested Serde object rejection, exact TypeScript alias/union shape, unknown-YAML finish/error/accessor semantics, complete-root typed binding, IPv4-mapped IPv6 spelling, factory visibility/lint policy, and Cargo feature-unification evidence.
- Planner/medium created `docs/task-packets/phase-1b-protocol-domain.md`; Reviewer/high corrected impossible nominal TypeScript assertions, test-harness filesystem wording, construction-error visibility, dependency lock-resolution steps, and dirty-tree dispatch semantics, then passed the packet with no blocker/high/medium finding. Security Reviewer/xhigh passed the correction delta.
- Implemented and accepted Phase 1B's complete protocol/domain surface: bounded public values and Serde, all 36 correlated public errors, generated TypeScript contracts, bounded parser-neutral unknown-YAML construction, exhaustive adversarial/trait-surface evidence, and deterministic contract generation.
- Reviewer/high and Security Reviewer/xhigh independently passed the final Phase 1B implementation with no blocker, high, or medium finding. The Orchestrator repeated the exact full packet gate and created signed checkpoint `df067b2`.
- Completed the four-boundary Phase 1C closure: exhaustive source/CST inspection, bounded production Saphyr reducer and scalar meters, monotonic eight-dimension reconciliation, proven entry/range/comment ownership, production-backed fault controls, 73-file named corpus, 8,192-case property harness, exact dependency/API/rustdoc audits, and truthful evidence.
- Reviewer/high and Security Reviewer/xhigh passed the final Phase 1C implementation and all correction deltas with no remaining blocker, high, medium, or low finding. The Orchestrator repeated the complete Cargo/pnpm/Tauri acceptance gate and created signed checkpoint `cf8341f`.
- Split Phase 1D into 1D-A read-only Manifest/Category binding, 1D-B Guide/nested binding, and 1D-C emission/mutation so implementation can advance through smaller gates.
- Implemented Phase 1D-A with production-parser-backed binding, exact current/newer Manifest classification, preserved unknown-entry identities, full `i64` Category ordering, timestamp/text/schema validation, read-only artifact support, positioned content-free failures, and deliberate public API/compile-fail evidence.
- Corrected the independent review findings covering newer-schema collapse, missing diagnostic positions, settings/malformed/boundary/read-only coverage, and aggregate constructor/Serde/mutation absence. Signed checkpoint `4d26e8f` verifies.
- Audited and pinned `focaccia 2.0.0`, `unicode-normalization 0.1.25`, and the already locked `url 2.5.8` without installing or switching tools.
- Implemented Phase 1D-B Guide, Applicability, Reference, Attachment, verification, and archive read-only binding with strict status/time, Unicode fold, URL canonicality, Attachment path/NFC/MIME/hash/size, unknown-partition, and body-preservation rules.
- Corrected the combined review's URL alias, diagnostic-position/type, allocation/resource, bounded-work, and filename-policy findings. The same Security Reviewer passed the correction closure. Signed checkpoint `0b5fb11` verifies.
- Implemented private canonical emitters for new Workspace manifests, active Categories, and draft Guides with exact ADR 0011 escaping, schema order, LF/no-BOM envelopes, fallible retained growth, production parse/rebind comparison, golden tests, invalid-input/resource tests, and public-absence evidence. Signed checkpoint `92e4c93` verifies.
- Implemented private existing-document mutation planning for Workspace name; Category name/order/archive/restore; and Guide title/summary/Category/body/stale transitions. Valid no-ops preserve exact bytes, actual changes require proven spans, candidates reparse/rebind, unknown data remains preserved, timestamps are monotonic, and tested content becomes stale. Signed checkpoint `539c29a` verifies.
- Replaced the inert UI with the responsive Reference Desk shell: three/two/one-pane layouts, incremental search, result selection, keyboard search/results/Escape behavior, responsive Workspace drawer, deterministic focus restoration, Guide reading, command copy, empty results, live announcements, light/dark themes, and current-guideline accessibility. The combined Design Reviewer closure reports no remaining finding; signed checkpoint `df95874` verifies.
- Implemented Phase 1E-A read-only Workspace loading: Linux descriptor confinement, bounded classification and scanning, BLAKE3 revisions, symlink/hardlink/special-file denial, changed-during-read refusal, duplicate/malformed isolation, a native Rust folder dialog with no renderer path argument, retained native session state, bounded metadata IPC, validated per-Guide body IPC, and real-data Reference Desk states. Full tests/Clippy/build/native build pass; independent security and manual UI closure remain open.

## Work in progress

- The fixture data is removed and the read-only vertical slice is green locally. Awaiting independent Security Reviewer/xhigh closure and manual native portal/real-data visual validation before acceptance. The UI intentionally shows Guide Markdown as inert preformatted source until the safe renderer packet; C2b optional-field/collection/Guide archive mutations remain deferred, not cancelled.

## Remaining work

- Phase 1D-C: finish deterministic emission and private mutation candidate authority; Phase 1E proves confinement/revisions and atomic recovery.
- Phase 2: native shell and safe Workspace onboarding/import/migration.
- Phase 3: Category/Guide/Attachment lifecycle and edit conflicts.
- Phase 4: search/ranking/index performance.
- Phase 5: staged safe Git workflows.
- Phase 6: standalone full MCP contract.
- Phase 7: Linux packaging, desktop integration, cross-environment hardening.

## Known bugs

No known regression in accepted phases. Phase 1E-A currently retains all parsed Guide bodies in native memory for the session; broad IPC is bounded, but streaming/lazy native reread is still required before claiming the 2 GiB worst-case scan budget is production-memory-safe. External changes after the initial scan are not watched/reloaded yet. Writability and Git-repository classification are not yet surfaced.

## Design and accessibility findings

- Reference Desk was accepted as the Phase 2A fixture-backed shell; the fixtures are now removed and real read-only Workspace states are wired. Existing captures cover 1440, 1024, 720, and 480 widths; light/dark, visible focus, empty results, narrow reader/back, responsive navigation, and 2× effective reflow.
- The first Design Reviewer pass found 2 high, 7 medium, and 3 low issues. Search naming, focus restoration, responsive navigation, Arrow/Escape behavior, contrast, announcements, command accessibility, tests, screenshots, repeated skip behavior, tooltips, and path translation were corrected.
- Final current-guideline closure: pass with no blocker, high, medium, or low finding.
- A fresh real-data/onboarding screenshot and interaction pass, rendered Markdown, edit/conflict/error states, and manual screen-reader verification remain Phase 2 acceptance gates.

## Security findings

- Highest risks: symlink/TOCTOU escape, frontmatter loss, concurrent lost updates, Git-triggered execution, Markdown/WebView injection/file disclosure, crash consistency, watcher/index staleness, duplicate IDs, MCP prompt injection/data overexposure.
- Baseline controls and required adversarial gates are documented. Phase 1B now supplies implementation evidence for bounded public protocols and parser-neutral YAML construction; filesystem, Git, renderer, and MCP mutation controls remain future-phase gates.
- Git commit/sync remain required but must follow read-only primitives and a dedicated malicious-repository gate. Unsafe shortcut implementation is explicitly blocked.
- Both security review rounds found specification gaps. All reported gaps are corrected; the Security Reviewer/xhigh closure pass reports no blocker, high, or medium finding.
- Phase 1A Security Reviewer/xhigh passes the actual implementation: exact dependencies/lockfiles, empty command manifest, one zero-permission capability, restrictive CSP, deterministic contract scripts, and zero-output MCP boundary.
- Phase 1B decision Security Reviewer/xhigh passes the full decision and the correction delta with no blocker/high/medium finding. Construction remains parse/validation-only, output text is provenance-controlled, path/Git factories stay protocol-private, and the unknown-YAML builder is bounded and poisoning.
- The initial Phase 1B implementation security rejection was fully corrected. Final Security Reviewer/xhigh review passes with no blocker/high/medium finding; no filesystem, Git, Markdown rendering, Tauri command, or MCP mutation authority was added.
- SafeHost v1 rejects legacy decimal/octal/hexadecimal/shortened IPv4 aliases before DNS fallback. Architect/xhigh, Reviewer/high, and Security Reviewer/xhigh approved and verified the implemented classifier and adversarial vectors.
- ADR 0013 final Security Reviewer/xhigh closure passes. It freezes bounded source/event traces, symmetric tag/merge guards, fail-closed borrowed-text exhaustion, linear comment/correspondence work, non-forged proven-prefix ownership, sanitized failures, and test-build-only fault seams.
- The Phase 1C packet Security Reviewer/xhigh closure passes at SHA `10525ef529832a69e4b9a50ad27efd797c800f235c3e55e7d5e8198241a518b5`. Its precompile supply-chain gate audits newly locked identities, already-locked newly reachable identities, and already-locked feature-changed identities before executing the changed graph.
- Phase 1C dependency gate passed after exact lock/source/feature/unsafe/build/proc-macro/yank review plus current official OSV crates.io dump and RustSec database scans. No reviewed exact package version is affected by a published advisory; the generated TypeScript remains unchanged.
- Partial implementation security review rejected `RoundTripStatus::Proven` safety, guarded-tail behavior, range/comment proof, and hostile-input linearity. The approved closure packet preserves all corrections and requires a fresh final Security Reviewer/xhigh pass.
- Final Phase 1C Security Reviewer/xhigh review and every security-sensitive correction delta pass with no blocker, high, medium, or low finding. The accepted parser remains crate-private, bounded, content-sanitized, non-recursive in Rehow traversal, dependency-pinned, and without filesystem/process/network/logging/mutation authority.
- Phase 1D-A adds no filesystem, revision, process, network, logging, mutation, adapter, Tauri, or MCP authority. Aggregate constructors/fields/mutation/Serde remain absent; newer schemas remain distinguishable and malformed-field failures carry parser-proven positions without rejected content.
- Phase 1D-B adds no filesystem, process, network, logging, URL-opening, Attachment-reading, adapter, Tauri, Git, or MCP authority. Canonical raw URL checks reject legacy numeric-IP aliases and misleading normalization; all product aggregate state remains read-only.
- Phase 1D-C2a adds only private in-memory mutation planning. It accepts typed intents, requires proven source spans for changes, reparses/rebinds candidates, does not create `tested`, and has no filesystem, process, network, logging, adapter, Git, Tauri, or MCP authority.
- Phase 1E-A introduces the first filesystem and Tauri command authority. Self-review verified no renderer path input/output, retained root descriptors, component-only `openat` with `O_NOFOLLOW`, regular/single-link enforcement, bounded enumeration/reads, changed-during-read checks, safe filename placeholders, canonical Guide-ID reads, inert React text rendering, content-free errors, and no write/process/Git/MCP/logging authority. Adversarial tests cover symlinks, hardlinks, sockets, oversize files, duplicates, Category mismatch, nested Markdown, and cap refusal.
- New exact pins are `blake3 1.8.7` (`std,pure`), `libc 0.2.189`, `rfd 0.17.2` (Wayland/XDG portal defaults), and `@tauri-apps/api 2.11.1`; direct rationale and permissive-license/transitive review are recorded in the packet/evidence. Independent Security Reviewer acceptance is still open.

## Blockers and human checkpoints

- No active human decision blocker. The independent Phase 1E-A Security Reviewer attempt failed before review because the agent usage quota was exhausted; no substitute agent was launched. This blocks phase acceptance, not continued implementation or a recoverable implementation checkpoint.
- Writing `.codex/` previously required and received a scoped environment permission; no global Codex configuration was changed.
- An unrelated untracked Rust executable named `x` appeared during the review window. All three participating agents denied creating it; it has not been executed, modified, staged, or removed and is excluded from Rehow checkpoints. It does not overlap the accepted documentation.

## Latest verified commit

`2339c48` — signed Phase 1E-A read-only Workspace implementation checkpoint. Its configured GPG signature verifies successfully. This progress-only recovery update follows it; independent security/manual acceptance remains open.

## Tests last run and results

- 2026-09-18: Phase 1E-A implementation gate: `cargo fmt --all --check`; locked all-workspace/all-target/all-feature tests and Clippy with warnings denied; `pnpm check`, full tests, production build, optimized Tauri no-bundle build; bootstrap and whitespace checks all pass. Counts include core 186 library, 18 error, 13 protocol, 11 unknown-YAML, one public-API suite with one positive and 32 compile-fail cases; desktop Rust 2, MCP 1, and Vitest 6. Native output is `/home/hichemtab/Projects/rehow/target/release/rehow-desktop`. The dependency feature/license audit passes locally. Independent security and manual portal/UI review remain open.
- 2026-09-18: `git verify-commit 2339c48`: pass — good signature from the configured project key; the implementation commit excludes `PROGRESS.md` and unrelated untracked `x`.
- 2026-09-07: final Phase 1C Orchestrator gate: parser 130/130; core library 143/143; error-contract 18/18; protocol 13/13; unknown-YAML 11/11; public API positive plus all 24 trybuild negatives; desktop/MCP/Vitest smoke tests; contracts generate/check; frozen pnpm materialization; `pnpm check`, tests, integration, build; Cargo fmt, locked Workspace Clippy/tests/check/release; rustdoc/API/TypeScript absence; six Cargo-tree and three inverse-tree audits; Tauri no-bundle; bootstrap and whitespace all pass.
- 2026-09-07: final correction gate: property 1/1 over 4,096 arbitrary plus 4,096 mutations from all 27 accepted materializations; dependency surface 6/6; parser 130/130; fmt, locked Clippy, and diff check pass. Reviewer/high and Security Reviewer/xhigh report no remaining finding.
- 2026-09-07: `git verify-commit cf8341f`: pass — good signature from the configured project key; post-commit worktree contains only preserved unrelated untracked `x`.
- 2026-09-18: Phase 1D-A gate: Cargo formatting and core Clippy with warnings denied pass; all-feature core tests pass with 153 library tests, 18 error-contract tests, 13 protocol tests, 11 unknown-YAML tests, the public API positive, and 27 compile-fail cases. Bootstrap validation passes with 14 ADRs; whitespace checks pass.
- 2026-09-18: focused independent review found one high and two medium issues; all were corrected and covered by tests. `git verify-commit 4d26e8f` passes; post-commit worktree contains only preserved unrelated untracked `x`.
- 2026-09-18: Phase 1D-B gate: formatting, locked all-feature core Clippy with warnings denied, bootstrap validation, whitespace checks, and all core suites pass — 164 library, 18 error-contract, 13 protocol, 11 unknown-YAML, one public API positive, and 30 compile-fail cases. The combined Security Reviewer closure passes all three prior high findings. `git verify-commit 0b5fb11` passes.
- 2026-09-18: Phase 1D-C1 gate: formatting, locked all-feature core Clippy with warnings denied, bootstrap validation, whitespace checks, and all core suites pass — 169 library, 18 error-contract, 13 protocol, 11 unknown-YAML, one public API positive, and 31 compile-fail cases. Forbidden-authority source audit passes. `git verify-commit 92e4c93` passes.
- 2026-09-18: Phase 1D-C2a gate: formatting and locked all-feature core Clippy with warnings denied pass; all core suites pass — 175 library, 18 error-contract, 13 protocol, 11 unknown-YAML, one public API positive, and 32 compile-fail cases. Forbidden-authority and whitespace audits pass. `git verify-commit 539c29a` passes.
- 2026-09-18: Phase 2A shell gate: root Biome/type/contracts checks; 6 desktop Vitest checks; Vite production build; complete Workspace Rust test suite; optimized native Tauri no-bundle build; bootstrap and whitespace checks pass. Current Web Interface Guidelines fetched and applied; combined Design Reviewer closure passes with no remaining finding. Eight inspected PNGs cover the documented visual matrix.

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
- 2026-09-06: `git verify-commit a0a5a0b4a782167368298025887f3642bd580633`: pass — good signature from the configured project key; the post-commit worktree was clean before this progress-only ledger update.
- 2026-09-06: Phase 1B construction Architect/xhigh: complete — proposed ADR 0012 and the narrow verification-hash clarification without implementing code.
- 2026-09-06: Phase 1B Security Reviewer/xhigh full and post-correction delta reviews: pass — no blocker/high/medium finding.
- 2026-09-06: Phase 1B Reviewer/high closure after protocol, TypeScript, retryability, Serde, feature-unification, and builder-semantic corrections: pass — no blocker/high/medium finding.
- 2026-09-06: current `python3 scripts/check_bootstrap.py`: pass — 17 required files, 12 ADRs, 8 pinned agents; `git diff --check`: pass. Installed runtime checks remain Node 26.2.0 and pnpm 11.20.0; neither was installed or switched by this work.
- 2026-09-06: Phase 1B decision checkpoint validation: `pnpm check`, `cargo fmt --all --check`, `cargo test --workspace --all-features --locked` (3/3), bootstrap validation, and staged whitespace check pass.
- 2026-09-06: `git verify-commit f35302db57d43db4c45ea7af56a57134b5995d03`: pass — good signature from the configured project key.
- 2026-09-06: Planner/medium Phase 1B packet: complete — all required template fields, exact allowed paths/behavior/tests/commands/non-goals/escalations, no implementation changes.
- 2026-09-06: Reviewer/high Phase 1B packet closure and Security Reviewer/xhigh correction-delta review: pass — no blocker/high/medium finding.
- 2026-09-06: `git verify-commit c3e3109351a98043485955732ed30a85e5cb01f3`: pass — good signature from the configured project key; bootstrap and staged whitespace checks passed before commit.
- 2026-09-06: Worker/low partial Phase 1B checks passed: `cargo test -p rehow-core --locked` (5 tests), Cargo formatting, core Clippy with warnings denied, contract generation/check, `pnpm check`, and `git diff --check`. These checks do not accept the implementation because the Worker reported the mandatory gaps now assigned to Integrator/medium.
- 2026-09-06: Integrator/medium reported a green current gate for the completed Phase 1B diff, including 30 core tests, contract determinism, all pnpm/Cargo checks, release build, bootstrap validation, and whitespace checks. The Orchestrator's source audit and independent Reviewer/high and Security Reviewer/xhigh reviews rejected acceptance because required behavior and evidence remain missing despite the green commands.
- 2026-09-06: Reviewer/high Phase 1B implementation gate: blocked by ten high findings and one medium trait-surface finding covering public detail Serde, timestamp/decimal/display canonicality, unknown-detail retention, file-size correlation, YAML precedence/meters, handwritten TypeScript schemas, and incomplete tests/evidence.
- 2026-09-06: Security Reviewer/xhigh Phase 1B implementation gate: blocked by four high and four medium findings; no Phase 1B filesystem, Git, Tauri, Markdown, or MCP execution authority was introduced.
- 2026-09-06: Architect/xhigh Phase 1B correction design: complete — exact private runtime/public TS marker split, bounded typed ErrorDto visitors, BoundedList visitor, primitive fixes, YAML precedence and natural-meter vectors, and evidence matrix supplied without editing files.
- 2026-09-06: SafeHost decision delta: Architect/xhigh, Reviewer/high, and Security Reviewer/xhigh pass after numeric-only DNS was deferred and legacy Linux IPv4 aliases were reserved/rejected; `git verify-commit bd84fd63b8010964568b8e9abf16ff43295f87b7` passes with the configured project key.
- 2026-09-06: final Phase 1B Orchestrator gate: installed Node 26.2.0/pnpm 11.20.0 used unchanged; frozen install, generated-contract check, `pnpm check`, unit/integration tests, production build, Cargo formatting, locked all-feature Clippy/tests, explicit public API test, native release build, alternate-CWD generation, bootstrap validation, and clean generated diff all pass. Final counts include core unit 13, error-contract 18, protocol 13, unknown-YAML 11, trybuild 1 positive plus 18 negative, desktop 1, MCP 1, and Vitest 1.
- 2026-09-06: final Phase 1B Reviewer/high and Security Reviewer/xhigh gates: pass — no blocker/high/medium finding.
- 2026-09-06: `git verify-commit df067b2`: pass — good signature from the configured project key.
- 2026-09-06: `git verify-commit 43212be`: pass — good signature from the configured project key; it records Phase 1B acceptance without staging ADR 0013 or `x`.
- 2026-09-06: initial ADR 0013 Reviewer/high gate: changes required — one high and four medium findings; the required failure/fixture matrix is partly unreachable, Saphyr precedence lacks a bounded algorithm, tag behavior after structural disagreement conflicts, authority-bearing APIs are underspecified, and invalid-UTF-8 positioning is ambiguous.
- 2026-09-06: initial ADR 0013 Security Reviewer/xhigh gate: changes required — one high and three medium findings; comment correspondence lacks a linear-work bound, source/decoded checks conflict, fault seams are incomplete, and post-disagreement IDs could fabricate correspondence.
- 2026-09-06: ADR 0013 Integrator/medium feasibility gate: changes required — two high and four medium findings, including inaccessible builder-meter test controls, undefined entry retention after disagreement, missing dead-code policy, incomplete no-wildcard `SyntaxKind` matching, positioned-error ordering, and implicit trait/accessor surfaces. Pinned dependency APIs/MSRVs/revisions were verified feasible from local sources.
- 2026-09-06: final ADR 0013 Reviewer/high closure at semantic SHA `b58eb3c3935f8cae6f7e19f7faaaca0d219135553893e9b83b44979a6c11768c`: pass — no blocker/high/medium finding.
- 2026-09-06: final ADR 0013 Security Reviewer/xhigh closure at the same semantic SHA: pass — no blocker/high/medium finding; symmetric source/event merge guards and borrowed-text terminal behavior are fail-closed and bounded.
- 2026-09-06: final ADR 0013 feasibility closure: pass — no blocker/high/medium finding; exact APIs, dependency calls, counters, `cfg(test)` bridge, tail descriptors, and evidence plan are implementable against Phase 1B.
- 2026-09-06: ADR 0013 whitespace, 48-fence balance, bootstrap validation, exact one-file staged inspection, signed commit, and `git verify-commit f39d7ab`: pass. The post-commit worktree contains only unrelated untracked `x`.
- 2026-09-06: Planner/medium created the integrated Phase 1C parser/reconciliation packet. Reviewer/high and Security Reviewer/xhigh rejected early drafts for incomplete artifact, reachability, comment-region, layout, accepted-source, disagreement, and dependency-audit wording; Planner/medium corrected each finding without implementation changes.
- 2026-09-06: final Phase 1C packet SHA `10525ef529832a69e4b9a50ad27efd797c800f235c3e55e7d5e8198241a518b5`; Reviewer/high and Security Reviewer/xhigh pass with no blocker/high/medium finding. Six baseline/post Cargo graph pairs and the exact A/B/C changed-graph audit union close cache and feature-unification gaps.
- 2026-09-06: packet whitespace check, bootstrap validation (17 required files, 13 ADRs, 8 pinned agents), prerequisite signatures, exact one-file staged inspection, signed commit, and `git verify-commit c98ab13`: pass. Post-commit worktree contains only unrelated untracked `x` before this progress ledger update.
- 2026-09-06: Phase 1C precompile dependency gate: exact pins/lock delta and A/B/C union reviewed; archive checksums, licenses, MSRVs, features, unsafe/build/proc-macro behavior, packaged VCS revisions, yank state, current OSV crates.io dump, and current RustSec database pass Security Reviewer/xhigh with no blocker/high/medium finding. No Node/pnpm/tool install or switch occurred.
- 2026-09-06: Worker/low partial parser result: first locked core check, formatting, focused parser tests (37), core Clippy, public trybuild including six Phase 1C negatives, contracts, full pnpm/Cargo/release/Tauri gates, bootstrap, and whitespace passed. Passing commands did not establish packet completion.
- 2026-09-06: Architect/xhigh, Reviewer/high, and Security Reviewer/xhigh reject the partial implementation. One blocker and multiple high/medium findings cover absent production guard/tail state, CST shape, range/correspondence safety, post-break entry fabrication, O(E*N + C*S) work, comment ownership, precedence, meter scope, allocation bounds, and non-production test substitutes. ADR 0013 remains feasible and unchanged.
- 2026-09-06: Planner/medium produced `docs/task-packets/phase-1c-parser-closure.md`; after correction, Reviewer/high and Security Reviewer/xhigh pass SHA `f5cfb9a720d91ab3d0b6495b9e11b94331c174604c9b6a8f349a37d65ef5f4ad` with no blocker/high/medium finding. Exact frozen hashes and aggregate recipe reproduce.
- 2026-09-06: closure packet staged alone above the dirty implementation, committed at signed `bb20342`, and `git verify-commit bb20342` passes. A recovery hardening commit follows it; the implementation/evidence changes remain unstaged and unaccepted by design.
- 2026-09-06: Orchestrator caught an impossible self-referential ledger-hash requirement before dispatch. The packet now verifies signed current `HEAD`, its exact packet parent, its PROGRESS-only tree delta, and index/worktree cleanliness for both durable instruction files. Reviewer/high and Security Reviewer/xhigh pass final packet SHA `229b1816c2dca2b83a6128a6392b009af6fdc5ec22c826dbfe7160227b4a9e31`; signed correction commit `5a09943` verifies.
- 2026-09-05: tool/version and repository inspection: pass; expected no-commit `git log` failure observed.
- The current desktop/MCP shells have smoke tests; product workflow tests begin with the applicable implementation phases.

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
- Protocol/content/parser decisions through accepted ADR 0012 and aligned content/error-contract corrections are checkpointed at `f35302d`.
- `docs/task-packets/phase-1b-protocol-domain.md` and its narrow ADR 0012 visibility clarification are checkpointed at `c3e3109`.
- Phase 1B implementation and evidence are checkpointed at signed commit `df067b2`.
- `docs/adr/0013-phase-1c-parser-and-reconciliation-contract.md` is accepted and checkpointed at signed commit `f39d7ab`.
- `docs/task-packets/phase-1c-parser-reconciliation.md` is approved and checkpointed at signed commit `c98ab13`.
- `docs/task-packets/phase-1c-parser-closure.md` is approved at `bb20342` and hardened at signed commit `5a09943`; the latter is the authoritative correction path and direct parent of this recovery ledger.
- Phase 1C parser/domain bridge, private tests and fixtures, dependency lock delta, API negatives, and evidence are accepted at signed commit `cf8341f`.
- ADR 0014, the Phase 1D-A packet, Manifest/Category domain records and binder, parser position/test seam, and API evidence are accepted at signed commit `4d26e8f`.
- Phase 1D-B Guide/nested binders, dependency/review evidence, task packet, API privacy tests, and accepted ADR 0014 binding delta are checkpointed at signed commit `0b5fb11`.
- Phase 1D-C1 private canonical emitters, golden/resource/API tests, packet, and evidence are checkpointed at signed commit `92e4c93`; full 1D-C acceptance waits for C2 and one combined review.
- Phase 1D-C2a required-field/body/status mutation planning, adversarial/API tests, packet, and evidence are checkpointed at signed commit `539c29a`; full 1D-C acceptance waits for C2b and one combined review.
- Phase 2A React/CSS shell, interaction tests, task packet, current-guideline review evidence, and screenshots are checkpointed at signed commit `df95874`; it adds no IPC or filesystem authority.
- Phase 1E-A adds `crates/rehow-core/src/storage.rs`, BLAKE3/libc pins, domain record extraction, native dialog/session commands, the real-data React states, updated locks, the revision vector, `docs/task-packets/phase-1e-a-readonly-workspace.md`, and `docs/evidence/phase-1e-a-readonly-workspace.md`. It is green but awaits independent security/manual acceptance.

## Next recommended action

Continue with root-writability/Git/nested-repository inspection and actionable onboarding states. When agent quota returns, run exactly one Security Reviewer/xhigh closure over Phase 1E-A; also manually exercise the native portal and capture real-data/error screenshots before accepting the phase. Keep edits disabled and return to C2b before mutation wiring.

## Recommended agent role and reasoning effort

Orchestrator — `gpt-5.6-sol`, high. Run zero extra agents for routine work; use one Security Reviewer/xhigh only when the Phase 1E-A closure can actually run.

## Explicit restart instructions

1. Read `AGENTS.md` and this file.
2. Read `docs/task-packets/phase-1e-a-readonly-workspace.md`, its evidence, `docs/architecture.md`, `docs/threat-model.md`, and ADRs 0002/0003/0009/0014.
3. Run `git status --short --branch`, `git log --oneline -8`, and `python3 scripts/check_bootstrap.py`.
4. Verify signed Phase 1E-A checkpoint `2339c48`; the following commit should change only `PROGRESS.md`.
5. Preserve and exclude unrelated untracked executable `x`; never execute, inspect, modify, stage, or delete it.
6. Run the smallest storage/desktop checks. Resume with writable/Git inspection and actionable onboarding without adding write authority, or rerun the one pending Security Reviewer closure if quota is available. Do not send or accept arbitrary paths over IPC.
