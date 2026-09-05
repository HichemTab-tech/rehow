# Multi-Agent Orchestration Protocol

Status: active
Last updated: 2026-09-05

## Objective

Keep long-running Rehow development reliable across agents and sessions while spending high reasoning only where judgment/risk warrants it. Repository state and durable documents, not conversation history or summaries, are authoritative.

## Role selection

| Work | Role | Effort |
| --- | --- | --- |
| Architecture, schema, migration, trust boundary | Architect | xhigh |
| Execution loop, diff verification, gate/commit/progress | Orchestrator | high |
| Packet decomposition | Planner | medium |
| Narrow specified implementation | Worker | low |
| Cross-package bounded integration | Integrator | medium |
| Independent correctness/maintainability review | Reviewer | high |
| UI/guideline/screenshot/accessibility review | Design Reviewer | high |
| Filesystem/Markdown/Git/MCP/security review | Security Reviewer | xhigh |

All configurations are in `.codex/agents/` and explicitly use `gpt-5.6-sol`. If unavailable, stop; no fallback is allowed.

## State authority order

When statements differ, reconcile in this order:

1. Actual filesystem and Git state.
2. Passing/failing command evidence for that state.
3. Accepted ADRs and specifications.
4. `PROGRESS.md` (then correct it if stale).
5. Task packet and agent summaries.
6. Conversation history.

An accepted ADR overrides a general architecture document when their dates/content conflict; update the general document in the same integration.

## Work lifecycle

### 1. Recover

The Orchestrator reads `AGENTS.md`, `PROGRESS.md`, named specs/ADRs, Git status/history, and runs the smallest useful validation. It identifies dirty files and avoids overlapping unknown/user work.

### 2. Select

Choose the first remaining item whose prerequisite decisions and prior gate exist. Do not ask the user to choose routine sequencing. If a human checkpoint from `AGENTS.md` applies, preserve evidence and ask one precise question.

### 3. Specify

The Planner creates a file from `docs/task-packets/TEMPLATE.md`. A packet is ready only if a Worker can execute it without choosing behavior, architecture, dependencies, persistence, public contract, or security policy. Otherwise route the unresolved part to Architect/Orchestrator and revise the packet.

### 4. Assign ownership

Record exact allowed paths/subsystem. Only one writer may touch overlapping files. Parallel tasks must be read-only or clearly non-overlapping; use isolated Git worktrees if parallel implementation becomes materially useful. Agents do not delete/move another agent's work to resolve overlap.

### 5. Execute

Delegate at the role/effort in the packet. The Worker reads references, implements only scope, tests, and returns exact files/commands/failures. It does not commit unless explicitly assigned.

### 6. Inspect and correct

The Orchestrator reads the actual diff and surrounding files, checks scope and generated artifacts, and reruns/validates commands. Summaries are hints, never acceptance evidence. Corrections return to the same role with concrete findings when practical.

### 7. Review

Meaningful changes require independent Reviewer review. Trigger Design Reviewer for shell/interaction/visual changes and Security Reviewer for the areas named in `docs/threat-model.md`. Reviewers are read-only, lead with file/line findings, and state gate result. Authors do not self-approve.

### 8. Integrate and commit

Integrator resolves only bounded inconsistencies that follow accepted decisions. Orchestrator runs the full applicable gate, stages only coherent files, inspects staged diff, and commits without pushing. Signed commit is used only when already configured and noninteractive. One commit should tell one reviewable story.

### 9. Persist and continue

Update `PROGRESS.md` with observed phase/task/status, completed/in-progress/remaining, findings, blockers, latest verified commit, exact tests/results, files, next action, role/effort, and restart steps. If the progress update is after the checkpoint commit, state that it is uncommitted or create a docs follow-up commit. Continue automatically.

## Correction loop

- Worker scope/behavior miss → send precise correction and expected test.
- Test failure caused by the change → return to Worker; do not integrate.
- Cross-package mismatch with decisions fixed → Integrator.
- Missing/contradictory foundational decision → Architect ADR, then re-plan.
- Independent finding disputed → Orchestrator reproduces/reads evidence; resolution is evidence-based.
- Security uncertainty at a review trigger → Security Reviewer; unresolved material risk blocks.
- UI code fails guideline/screenshot gate → Worker/Integrator fixes; Design Reviewer reruns with fresh upstream rules.

## Commit and progress invariants

- No push/release/publication without explicit authorization.
- No optimistic “complete”; all required gate commands/results are recorded.
- No secrets, credentials, personal Workspaces, build caches, or transient screenshots in commits.
- `PROGRESS.md` identifies whether its test claims apply to HEAD or a dirty tree.
- Known failures name command, symptom, ownership, and whether blocking.

## Human checkpoints

Stop only for material product contradiction, unresolved foundational tradeoff with materially different product outcomes, destructive/hard-to-recover operation, credentials/external authorization/resource creation, unsafe Git conflict, irreducible privacy/security risk, access outside granted repository, push/release, or explicit cost/time/task limit. Provide the evidence, safe options, and exact consequence of each choice.

## Emergency recovery

If an agent/session stops mid-change:

1. Do not reset/clean/delete.
2. Inspect status/diff/untracked files and active worktrees.
3. Read `PROGRESS.md` and relevant packet, but trust actual files.
4. Run targeted parse/type/test checks without changing content.
5. Classify changes as complete, salvageable, unrelated user work, or unsafe.
6. Resume/repair in place when ownership is clear; otherwise preserve and escalate.
7. Update progress with the recovery result before new scope.
