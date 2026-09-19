# Phase 5A read-only Git status evidence

Date: 2026-09-19

## Boundary

The core resolves only root-owned `/usr/bin/git` and `/usr/bin/bwrap`. Repository commands have a cleared environment, direct argv, null stdin, fixed safe config overrides, no terminal prompts/optional locks, a two-second timeout, and 1 MiB stream caps. Bubblewrap unshares user/mount/network state, mounts the Workspace read-only, and constructs a config-free Git directory from retained object/ref/HEAD/index/packed-ref/shallow descriptors. An empty mount masks object `info`, including an alternates file created after capability inspection.

Local config is consulted only by `git config --file <held-fd> --no-includes -z --list` outside repository context. Only remote names and the current branch tracking pair survive parsing; URLs and raw stderr never cross the core boundary. Validated refs feed sandboxed `rev-list` for known ahead/behind counts. Porcelain paths and rename-source tokens stay opaque bytes.

The pathless Tauri command returns a closed bounded payload. React validates every key/type/count and reports missing Git/repository, unsafe/failed status, branch/detached state, changes/conflicts, and known divergence in the existing status bar. No Git mutation or network command exists.

## Adversarial evidence

- Direct process filter, included clean filter, hook path, and fsmonitor commands point to a sentinel script; status reports the modified file and the sentinel is absent.
- A secret-bearing remote URL never appears; only `origin` crosses the boundary.
- Pre-existing alternates are rejected. A deterministic post-check alternates injection is invisible inside the sandbox and status succeeds.
- Synthetic modified, untracked, renamed, and rename-source paths with invalid UTF-8 parse without decoding path bytes.
- A tracking ref equal to HEAD reports `origin/master` with known `0/0` divergence.

## Review

The single Security Reviewer initially found repository-filter execution, alternates TOCTOU, and whole-record UTF-8 decoding. All three were structurally corrected; its narrow recheck passed with no blocker/high/medium finding. The same reviewer passed the tracking/divergence delta with no blocker/high/medium finding.

## Verification

- `pnpm check`, `pnpm test`, `pnpm build`, and `pnpm tauri:build`: pass.
- `cargo fmt --all --check` and all-Workspace/all-target/all-feature Clippy with warnings denied: pass.
- Test counts: 310 core library tests passed with 1 intentional ignore; 18 error-contract, 13 protocol, 1 public-API, 11 unknown-YAML, 16 desktop Rust, 1 MCP, and 30 Vitest tests passed.
- Bootstrap validation, generated-contract checks, and whitespace checks: pass.
