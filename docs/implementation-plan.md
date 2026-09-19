# Gated Implementation Plan

Status: approved bootstrap sequence
Last updated: 2026-09-06

## Gate rules

A phase is accepted only when its criteria, tests, documentation, review, and recoverable commit are complete. Security-sensitive changes require Security Reviewer review; meaningful UI changes require the current web-design guideline review and screenshot inspection. The Orchestrator may split phases into task packets but may not bypass gates.

## Phase 0 — Bootstrap foundation

Scope:

- Inspect empty repository, Git state/history, tools, official Codex project-agent configuration, current platform constraints, and design guidelines.
- Establish durable specifications, ADRs, role configs, task-packet template, orchestration protocol, and recovery ledger.
- Select architecture, storage/content formats, trust boundaries, search approach, MCP contract, visual direction, testing strategy, and phase sequence.

Gate:

- Required root files: `AGENTS.md`, `PROGRESS.md`; required specs: product, architecture, implementation, content model, design, MCP, testing, threat model, installation, development, orchestration, and public error contract; ADR records 0001–0010 with accurate status; task-packet template; project design skill.
- All 8 role files exist: Architect/xhigh, Orchestrator/high, Planner/medium, Worker/low, Integrator/medium, Reviewer/high, Design Reviewer/high, Security Reviewer/xhigh. Every file explicitly pins `gpt-5.6-sol`.
- Architect consistency review, independent Reviewer/high, Design Reviewer/high planning pass, and Security Reviewer/xhigh report no unresolved blocker/high finding (or a finding is explicitly deferred behind a named pre-phase gate that cannot affect Phase 1).
- Run `python3 scripts/check_bootstrap.py`, `git diff --cached --check`, and inspect `git status --short --branch` plus the staged diff; all pass/are coherent.
- A signed planning-foundation commit exists when configured signing works, and `PROGRESS.md` identifies Phase 1's first packet and exact verified state.

## Phase 1 — Core foundation and format proof

Objective: prove the riskiest content-durability and confinement choices before building the UI.

Milestones:

1. Scaffold minimal pnpm + Cargo workspace, pinned toolchains, checks, and generated-contract workflow.
2. **Phase 1B:** implement domain/protocol identifiers, decimal/revision/token/text primitives, the exhaustive DTO/error baseline, and bounded parser-neutral unknown-YAML semantic values over directly constructed inputs. Phase 1B does not implement Workspace Manifest, Category, Guide, or Attachment aggregate records; YAML/Markdown/CST parsing or serialization; filesystem/Git conversion; TypeScript representations for unknown YAML; or storage I/O.
3. **Phase 1C:** after ADR 0011 receives Architect and Security Reviewer approval, introduce its exact pinned dependencies and implement the bounded source/envelope, semantic, and presentation parsers plus their immutable generic reconciliation artifact. This phase proves semantic acceptance, limits, complete byte coverage, and read-only versus non-round-trippable classification without identifying product known fields, creating mutation authority, defining product aggregate validation, or writing Workspace files.
4. **Phase 1D:** deliver ADR 0014 in three small gates: 1D-A Manifest/Category read-only binding with no new dependency; 1D-B Guide/nested binding after dependency audit; 1D-C emitter and closed mutation candidates. The phase remains pure and in-memory and exposes no effective-tested, filesystem, revision, inventory, clock, entropy, renderer, process, network, adapter, Tauri, or MCP capability.
5. **Phase 1E:** implement capability-confined regular-file reads, symlink/hardlink rejection, exact revision/inventory vectors, the fixed Markdown/YAML and Attachment count/per-file/aggregate scan limits in `docs/content-model.md`, atomic no-replace/exchange single-file writes, and the full ADR 0009 recovery journal/object state machine.
6. Add isolated malicious/round-trip/crash/concurrency fixtures and a minimal CLI/test harness.

Gate:

- Create/read/update proof works only in temporary Workspaces.
- Unknown valid metadata values survive; unsafe-to-round-trip edits are refused without content change.
- Traversal, absolute paths, managed symlinks/hardlinks/races, duplicate IDs/keys, fixed revision/snapshot vectors, stale revisions, concrete-syntax aftermath, numeric limits, every recovery state, and injected durability failure point are tested.
- `cargo fmt`, `clippy -D warnings`, Rust tests, applicable TS type/tests, and contract drift pass.
- Independent Reviewer and Security Reviewer approve.
- ADR/content docs match the implementation and a verified commit exists.

## Phase 2 — Desktop shell and Workspace onboarding

Milestones:

1. Scaffold React/Vite/Tailwind/shadcn and minimal Tauri v2 shell with restrictive CSP/capabilities.
2. Implement typed IPC/session handles and Workspace inspection matrix.
3. Implement first-launch Create/Open and current/old/newer/malformed/read-only behavior. Reserve a clearly unavailable Clone/Connect affordance for development only; operational Git onboarding arrives in Phase 5 and is required before product completion.
4. Implement copy import for ordinary Markdown and migration/adoption dry-run/backup foundation.
5. Build the Reference Desk shell including responsive panes and required onboarding/error states.

Gate:

- Production app launches as a native window without a local server.
- Inspection fixtures cover every specified class plus Git/nested/writability dimensions.
- No write occurs during inspect/open-read-only; create refuses changed/nonempty targets.
- Copy import leaves sources untouched; migration has verified backup and explicit confirmation.
- UI unit/integration/keyboard/accessibility tests pass.
- Screenshots at the design matrix sizes are visually reviewed with current guidelines; findings resolved/recorded.
- Security review covers Tauri capabilities/CSP/dialog paths/import.

## Phase 3 — Category, Guide, and Attachment lifecycle

Milestones:

- Category create/edit/reorder/archive/restore/remove-safe.
- Guide create/edit/rename/move/status/archive/restore/recoverable delete.
- Structured metadata editor plus Markdown source/preview; validation diagnostics.
- Related Guides, references, Tags/aliases, files affected, and trust/status affordances.
- Managed Attachment import/metadata/inert text or bounded raster view/export/reveal/removal after size/MIME decision; never externally open an Attachment file.
- External-change dirty-editor conflict comparison and resolution.

Gate:

- CRUD/status/move/recovery/concurrency cases pass across application/core/UI.
- `tested` is impossible without native user approval for the exact verification content hash; verification-relevant edits become effectively stale.
- Unknown metadata remains preserved.
- Attachment traversal/symlink/device/oversize/type tests pass.
- Keyboard, screen-reader, zoom, reduced-motion, error/empty/conflict, and narrow UI tests/review pass.
- Independent, Design, and Security reviews approve.

## Phase 4 — Search and retrieval

Milestones:

- Field extraction and explicit ranking policy implementation.
- Disposable index lifecycle, corruption rebuild, incremental change handling, diagnostics.
- Keyboard result navigation, excerpts/highlights, scopes/filters, archived search.
- Deterministic relevance corpus and 5,000-Guide production benchmark harness.

Gate:

- Ranking fixtures prove exact title/alias/command/path/error treatment.
- Watch loss, malformed docs, duplicate IDs, index corruption, rebuild, and stale cursor behavior pass.
- Production budgets in product spec pass on recorded reference environment or deviations become explicit blockers/ADR decisions.
- Search interaction screenshots, accessibility, and responsiveness are reviewed.

## Phase 5 — Git workflows

Milestones:

1. Git discovery and strictly contained read status/branch/remotes/divergence/conflicts.
2. Separately accepted init/connect/clone ADR and implementation with descriptor-bound destination safety.
3. Separate commit ADR and implementation covering managed-only scope, clean index/ref CAS, unrelated-change preservation, and no hooks/filters/helpers.
4. ADR 0025 synchronization gates: isolated fetch/validated incoming tree, bounded ancestry, journaled portable-Workspace mutation, durable object/index/direct-ref fast-forward publication, the pathless native inspect/cancel/confirm/recover lifecycle, and the Reference Desk manual-sync UI are implemented; next are safe push, trusted credential provenance, auth/conflict states, and opt-in schedule policy.

Gate:

- Malicious hooks, fsmonitor, filters, textconv, pager/editor, config/environment, filenames/refnames, oversized output, timeout, missing executable, unsafe ownership, nested repo, auth, divergence, and conflicts have adversarial coverage.
- No credential value appears in logs/UI/test snapshots.
- No destructive Git action exists; Git never writes active content outside the core journal.
- Offline content behavior remains unaffected.
- Security Reviewer approves each mutation subset; unresolved subprocess execution risk blocks that feature rather than weakening policy.

## Phase 6 — Standalone MCP server

Milestones:

- Independently packaged stdio server sharing the application/core crates.
- Startup-configured Workspace allowlist and read-only default.
- Workspace/Category/Guide/search/validation/Attachment-metadata tools.
- Per-Workspace write/broad/Git capabilities, launch handles, revisions/effect sets, dry-run tokens, and native out-of-band approval companion.
- Git status/plan/commit/safe sync only after Phase 5 primitives are approved.
- Client configuration/setup docs and end-to-end target AI workflow.

Gate:

- Protocol stdout purity, schema snapshots, pagination/limits, stable error mapping, and tool annotations pass.
- Traversal/symlink/arbitrary-root/duplicate-or-nested-root/shell attempts, prompt injection content, malformed files, conflicting writers, stale plans, replayed/substituted native approvals, destructive safeguards, and data leakage have adversarial tests.
- Desktop and MCP conformance fixtures produce the same domain results.
- Server runs without the desktop and without network access for content operations.
- Security Reviewer approves.

## Phase 7 — Packaging, desktop integration, and release hardening

Milestones:

- Linux icon, desktop entry, user-level/native packages, application-menu launch, upgrade/uninstall docs.
- Wayland/X11/WebKitGTK matrix and targeted graphics diagnostics.
- Optional user-configured Hyprland/Omarchy shortcut instructions without modifying managed files.
- MCP install/client configuration and version compatibility.
- Recovery/manual, performance, accessibility, visual, and threat-model final audits.

Gate:

- Clean-machine install → create/open → edit/search → Git → MCP → restart/recovery smoke journey passes.
- Oldest supported build baseline is recorded; `deb`/`rpm` and any AppImage are independently tested.
- No required global configuration is overwritten; uninstall preserves Workspaces by default.
- Production build, Cargo/TS tests/checks, visual matrix, accessibility review, security review, and docs audit pass.
- Release/push/publication remains a human checkpoint.

## Dependency introduction policy

New runtime dependencies require a packet rationale covering ownership/maintenance, security surface, license, binary size, alternatives, and version pin. Workers do not add dependencies. Major upgrades get their own reviewed packet.

## Initial ready packet

After Phase 0 commits, the Planner should create `docs/task-packets/phase-1a-scaffold.md` for only:

- root pnpm/Cargo workspace scaffolding;
- Node/Rust version pins and baseline lint/type/test/build scripts;
- empty boundary crates/apps with one smoke test each;
- no Workspace schema or filesystem implementation yet.

This narrow slice validates tool compatibility before domain code is committed.
