# Rehow Agent Handbook

## Mission

Build Rehow: a polished, Linux-first, local desktop knowledge tool for finding and preserving repeatable technical Guides. The promise is “Remember how you did it.” A Workspace folder and its human-readable Markdown are the portable source of truth.

## Non-negotiable invariants

- Product terms are **Workspace**, **Guide**, **Category**, **Tag**, and **Attachment**.
- The product is a Tauri v2 desktop app, not a browser-served local website.
- Workspace Markdown and YAML are canonical. Derived indexes/caches are disposable.
- Never overwrite, reorganize, migrate, delete, or adopt existing content silently.
- All writes are confined to an explicitly opened Workspace, atomic, revision-checked, and recoverable.
- Reject traversal and symlink escapes. Commands in Guides are inert documentation; Rehow never executes them.
- Render Markdown with raw HTML disabled and links/media constrained by policy.
- Git is optional. Never store credentials, install hooks, force-push, hard-reset, discard changes, rewrite history, or resolve ambiguous conflicts automatically.
- AI-created or AI-edited Guides remain `draft` (or retain a non-tested state). Only explicit user confirmation may set `tested`.
- Preserve unknown valid metadata where practical.
- Every project agent uses `gpt-5.6-sol`; never substitute a model.
- Preserve unrelated user changes. Do not push, publish, or create external resources without authorization.

## Repository map

- `apps/desktop/` — React/Vite UI; `apps/desktop/src-tauri/` is the Tauri binary/command adapter.
- `crates/rehow-core/` — canonical Rust `domain`, `protocol`, `markdown`, `storage`, `search`, `git`, and `application` modules.
- `crates/rehow-mcp/` — separately runnable stdio MCP server.
- `packages/contracts/` — generated/checked TypeScript-facing contracts only; Rust schemas remain authoritative.
- `docs/` — product, architecture, security, design, testing, operations, and ADRs.
- `docs/task-packets/` — implementation-ready packets; use `TEMPLATE.md`.
- `.codex/agents/` — project-scoped role configurations.
- `.codex/skills/` — project-pinned skills; design reviews use `web-design-guidelines`.

The map is the target shape during bootstrap; missing paths are created only by their approved phase.

## Standard commands

Use the root scripts once the workspace is scaffolded:

- `pnpm install --frozen-lockfile` — install JavaScript dependencies.
- `pnpm check` — formatting/lint/type checks for all workspaces.
- `pnpm test` — unit tests.
- `pnpm test:integration` — isolated filesystem/Git/MCP integration tests.
- `pnpm build` — production web/MCP artifacts.
- `pnpm tauri:dev` — native desktop development.
- `pnpm tauri:build` — native release bundle.
- `cargo fmt --all --check` — Rust formatting gate.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — Rust lint gate.
- `cargo test --workspace --all-features` — Rust unit/integration tests.

Until scripts exist, run the nearest applicable command and record the gap in `PROGRESS.md`. Tests must use temporary Workspaces and repositories, never personal Rehow content.

## Agent responsibilities

- **Architect / xhigh:** foundational architecture, schemas, migrations, threat modeling, and cross-cutting change.
- **Orchestrator / high:** owns the loop, inspects real diffs, verifies gates, commits coherent checkpoints, and maintains progress.
- **Planner / medium:** writes exact task packets. It does not leave product or security decisions to workers.
- **Worker / low:** implements only a ready packet. No new architecture, dependencies, storage semantics, or security policy.
- **Integrator / medium:** integrates bounded related work and runs broader verification.
- **Reviewer / high:** independent correctness, maintainability, accessibility, and regression review.
- **Design reviewer / high:** applies the current web-design guideline skill to implemented UI and screenshots.
- **Security reviewer / xhigh:** reviews confinement, Markdown, Git, MCP, concurrency, secrets, and destructive behavior.

Only one agent writes overlapping files at a time. Parallelize read-only analysis or isolated, non-overlapping implementation. The Orchestrator normally commits.

## Orchestration workflow

1. Read this file, `PROGRESS.md`, applicable specs/ADRs, and Git status/history.
2. Reconcile durable docs with the actual tree; correct stale progress first.
3. Select the next ready phase item and confirm prerequisites/decisions.
4. The Planner emits a packet containing every field in `docs/task-packets/TEMPLATE.md`.
5. Delegate at the lowest safe effort. If hidden design judgment remains, escalate the role instead.
6. Wait, then inspect the actual diff and files—not only the delegate summary.
7. Run the packet’s commands. Request corrections for failures or scope drift.
8. Meaningful changes require independent review; security-sensitive or major UI changes require the specialized review.
9. Integrate only after the phase gate passes. Create a coherent commit when appropriate.
10. Update `PROGRESS.md` with verified facts, the commit, tests, and exact restart action; continue automatically.

Stop only for a material requirement conflict, major unresolved architecture tradeoff, destructive/hard-to-recover action, credentials/authorization, external publication, unsafe Git conflict, security risk that cannot be reduced, out-of-scope filesystem access, or an explicit budget limit.

## Design and accessibility

- Follow `docs/design-direction.md`; the approved direction is **Reference Desk**.
- Before every formal UI review, read `.codex/skills/web-design-guidelines/SKILL.md` and fetch its current upstream rules. Record findings in `PROGRESS.md`.
- Preserve a search-first, keyboard-first workflow and useful density at wide and narrow sizes.
- Use semantic controls, visible `:focus-visible` treatment, labels/accessible names, hierarchical headings, a skip link, polite live regions, and predictable focus restoration.
- Support reduced motion, 200% text scaling, light/dark system themes, long text, long commands, and touch targets where applicable.
- Avoid generic dashboards, marketing UI, gradients/glow/glass, card nesting, decorative pills, excessive rounding/space/animation, and raw default shadcn styling.
- Capture and inspect the representative states/sizes required by the design spec before accepting a UI phase.

## Security boundaries

- The Rust core owns filesystem, schema, revision, search-index, and Git safety. UI and MCP adapters cannot bypass it.
- Native code owns folder/file dialogs and returns non-forgeable selection capabilities. Renderer IPC uses those tokens, recent-Workspace IDs, or opened handles—never caller-selected arbitrary absolute paths.
- Canonicalize the root once; reject symlinks in managed content paths and confine all operations beneath the capability root.
- Use same-directory temporary files, flush, exchange/no-replace atomic rename where supported, parent-directory sync, and durable before-images. Recover or explain interrupted operations.
- Treat Markdown, imported files, filenames, Git repositories, MCP inputs, and Git output as untrusted.
- Git uses an argument-vector subprocess without a shell; disable hooks for every invocation and never invoke aliases, editors, pagers, difftools, or mergetools.
- Destructive/broad MCP mutations require per-Workspace startup authority, dry-run plans, current revisions/effect-set tokens, and plan-bound confirmation tokens. MCP cannot directly assert `tested`; it may create a pending approval for the native app/user.
- Never log file contents, commands, remote credentials, tokens, SSH keys, or unredacted credential-bearing URLs by default.

See `docs/threat-model.md` for controls and review triggers.

## Testing and commit expectations

- Add tests with behavior changes; include hostile and recovery cases for security boundaries.
- Required phase gates are specified in `docs/implementation-plan.md` and `docs/testing-strategy.md`.
- Do not hide failures or call a phase complete while a required gate fails.
- A commit is a reviewable unit with applicable tests passing. Do not mix unrelated work.
- Do not amend/rewrite another agent’s or the user’s commits unless explicitly authorized.
- `PROGRESS.md` must name the latest verified commit. If progress itself follows that commit, record that exact condition rather than claiming it is included.

## Session recovery

At every new session:

1. Read `AGENTS.md` and `PROGRESS.md`.
2. Read the specifications/ADRs named by the current task.
3. Run `git status --short --branch` and `git log --oneline -8`.
4. Run the smallest useful validation for the touched subsystem.
5. Reconcile and correct stale `PROGRESS.md` claims.
6. Resume the exact next action. Do not depend on conversation history.

If the tree is dirty, identify ownership before editing overlapping files. If recovery is uncertain, preserve evidence and escalate rather than deleting or resetting.
