# Phase 5B1 Git initialization evidence

Date: 2026-09-19

## Boundary

The renderer supplies no path, branch, repository structure, config, command, or confirmation result. Planning is pathless and returns only a random `PlanToken` plus the fixed `main` branch. The native process retains the opaque plan, requires the exact token once, rechecks the open writable Workspace, and presents an out-of-band native confirmation before apply.

The core binds the plan to Workspace ID, current manifest revision, and the launch-scoped retained-root capability identity. Apply rereads `rehow.yaml`, repeats writable/ancestor inspection, and refuses stale or nested state. Rust creates `.git` without spawning Git: only `HEAD`, canonical core config, `objects/info`, `objects/pack`, `refs/heads`, and `refs/tags` are accepted. Directories are `0700`; regular files are single-link `0600`; all access is descriptor-relative/no-follow. Files are written and fsynced as unnamed same-filesystem inodes, then linked no-replace at their canonical names; every leaf and parent directory descriptor is explicitly fsynced.

An exact inert `rehow-init` provenance marker binds interrupted work to Workspace ID, manifest revision, and stable root hash. Empty or exact marker-bound canonical partial state can resume. Unknown entries, differing bytes/modes, symlinks, non-direct repositories, and noncanonical residue are never deleted, overwritten, or adopted. The marker remains after completion, avoiding a name-based unlink race; a complete repository is recognized normally and cannot be reinitialized.

## Adversarial evidence

- A newly initialized repository is recognized by the already accepted config-free status sandbox with unborn branch `main`.
- Exact `0700`/`0600` modes, canonical HEAD, and the inert provenance marker are asserted.
- External manifest change makes the plan stale before any `.git` entry is created.
- Unknown `.git` content, a symlinked `.git`, and a Workspace nested inside an ancestor repository are refused without modifying their targets or bytes.
- A deterministic failure after an unnamed file is durable but before publication leaves no partial canonical filename and resumes successfully. Failures after marker creation and directory durability leave marker-bound state; reopening, replanning, and applying completes each case without overwrite. A failure after all file durability stages is recognized as an already valid repository rather than reinitialized.
- A second apply of the same plan is stale; native payload tests prove the plan/result contain no path.

## Verification

Focused core Git initialization passed 7/7; native payload and single-use token tests passed 2/2; Vitest passed 31/31. The full checkpoint passed `pnpm check`, `pnpm test`, `pnpm build`, `pnpm tauri:build`, Cargo formatting, locked all-Workspace/all-target/all-feature Clippy with warnings denied, bootstrap, contract, and whitespace checks. Counts are 313 normal core tests plus 1 intentional release-only ignore, 18 error-contract, 13 protocol, 1 public-API/trybuild, 11 unknown-YAML, 18 desktop Rust, 1 MCP, and 31 Vitest tests.

The current Web Interface Guidelines were fetched and applied to the bounded footer action. It uses a named native button, visible hover/focus/busy states, native confirmation, polite live feedback, actionable visible errors, and deterministic focus return to search after success; no finding remains.

The single Security Reviewer first identified partial canonical-file publication, missing leaf-directory fsyncs, overstrong failure wording, and absent adapter replay coverage. Corrections use fsynced unnamed-inode no-replace publication, explicit leaf/parent fsyncs, truthful recovery copy, retained provenance, capability-bound plans, and tested mismatch/single-use semantics. Final verdict: PASS with no blocker/high/medium finding. Filesystems without `O_TMPFILE` fail closed; physical power-loss behavior remains subject to the documented filesystem guarantees.
