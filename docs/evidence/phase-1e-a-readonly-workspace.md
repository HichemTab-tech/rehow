# Phase 1E-A Read-only Workspace Evidence

Date: 2026-09-18
Status: implementation checkpoint ready; independent Security Reviewer gate pending because the only requested review agent could not start when the agent usage quota was exhausted.

## Implemented

- Linux root directory retained as an owned descriptor; managed traversal uses one-component `openat` calls with `O_NOFOLLOW`.
- Bounded descriptor-relative enumeration and reads; only regular single-link files are accepted. Symlinks, hardlinks, sockets, directories-as-files, oversize files, and changed-during-read files fail closed.
- Empty, compatible, newer-schema, ordinary Markdown (nested to a bounded depth), unrelated, and malformed inspection outcomes.
- Orthogonal effective writability and Git relationship facts: safe root `.git` directory/worktree marker, ancestor repository detection through bounded descriptor-only parent walking, incomplete-probe reporting, and unsafe marker reporting without following it.
- Current manifest/Category/Guide parsing through the accepted parser and binders; category mismatch, duplicate IDs, malformed documents, unsupported entries, and oversize Guides are isolated or stop at the documented global limits.
- Exact BLAKE3 document revision framing with a checked-in fixed vector.
- A native Rust folder dialog whose Tauri command accepts no path. The selected absolute path never enters the renderer response or logs. The compatible Workspace descriptor remains retained in native state.
- Bounded list DTOs omit Guide bodies. The renderer requests one body at a time using a canonical Guide UUID already present in the retained Workspace; no path-bearing read command exists.
- React fixtures were removed. The Reference Desk shell now shows actual Workspace/Category/Guide metadata, read-only Guide content, safe onboarding classifications, and explicit read-only status.

## Self-review corrections

- Isolated later duplicate IDs so UI keys and identity lookup remain unambiguous.
- Counted every valid Guide filename as a scan candidate before parsing, preventing malformed files from bypassing the 10,000-Guide cap.
- Removed all Guide bodies from the broad IPC response and added validated per-Guide reads.
- Added pre/post descriptor metadata comparison to reject concurrent in-place changes during reads.
- Added RAII closure for `DIR*` on every enumeration exit.
- Replaced hostile unsupported filenames in public issues with fixed safe placeholders.
- Added nonblocking special-file handling and a Unix-socket adversarial test.
- Added `faccessat` effective-access probing and bounded ancestor Git-marker inspection; regular `.git` files must use the bounded `gitdir: ` form, while symlink/hardlink/special/invalid markers are unsafe rather than followed.

## Dependency review

Direct pins and rationale are recorded in the task packet. The resolved new Rust graph uses permissive licenses (MIT, Apache-2.0 variants, CC0-1.0, MIT-0, or ISC). `blake3` uses `std,pure`; `rfd` uses its Wayland/XDG portal defaults and introduces no shell-command API. Existing Node 26.2.0 and pnpm 11.20.0 were used unchanged.

## Verification

- `cargo test --workspace --all-targets --all-features --locked`: pass; core includes 188 library tests plus 18 error, 13 protocol, 1 public-API/32 compile-fail, and 11 unknown-YAML tests; desktop 2; MCP 1.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: pass.
- `pnpm check`, focused desktop tests, and `pnpm build`: pass; desktop Vitest 7/7 and Vite production build.
- `pnpm tauri:build`: pass; optimized native binary produced. The existing informational `.app` identifier warning remains.
- `git diff --check` and `python3 scripts/check_bootstrap.py`: pass.

## Gates still open

- Independent Security Reviewer/xhigh review must be rerun when agent quota is available; no substitute agent was launched.
- Manual interaction with the native folder portal and a screenshot/visual pass of the real-data and error states remain before Phase 2 acceptance.
- Workspace writability, existing/nested Git state, recent-Workspace persistence, external-change watching/reload, rendered safe Markdown, creation/import/adoption, and all writes remain later packets.
