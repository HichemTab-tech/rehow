# Task Packet: launch SSH-agent provenance

- **Objective:** Capture a launch-inherited SSH agent as a path-redacted, inode-held native capability without enabling authenticated Git.
- **Required reasoning level:** high.
- **Prerequisites:** ADRs 0023 and 0025; accepted config-free Git sandbox; Phase 5D3A checkpoint `de848d7`.
- **Allowed files or subsystem:** `crates/rehow-core/src/git.rs`, `apps/desktop/src-tauri/src/lib.rs`, focused tests, ADR 0025, architecture/threat/testing docs, evidence, and `PROGRESS.md`. No dependencies or contracts.
- **Relevant specifications and ADRs:** ADR 0023 credential provenance; ADR 0025 Phase 5D3B; Git sections of `docs/architecture.md`, `docs/threat-model.md`, and `docs/testing-strategy.md`.
- **Exact required behavior:** At native state construction, read `SSH_AUTH_SOCK` once. Missing means not configured. An absolute, bounded Unix socket owned by the effective user, with one link, private mode, and owner-safe/root-safe non-symlink ancestors becomes an O_PATH-held capability. Every other value becomes unsafe without blocking startup. Public status and Debug reveal no path or inode.
- **Important states and edge cases:** absent/empty/relative/oversized path; regular file; final or ancestor symlink; loose user-owned parent; safe root sticky parent; socket replacement during inspection; non-UTF-8 path.
- **Security constraints:** No connect, network, SSH/Git process, key read, user config, renderer IPC, logging, or filesystem mutation. Recheck path and ancestors around descriptor capture; retain only the selected inode capability.
- **Tests to add or update:** Focused core tests for trusted socket, redacted Debug, absent status seam, relative/regular/symlink/unsafe-parent refusal, and held-inode behavior after replacement; desktop construction remains smoke-covered.
- **Commands to run:** focused Cargo tests; Cargo format/Clippy/full tests; `pnpm check`; bootstrap and whitespace checks.
- **Required reviews and artifacts:** Defer the one Security Reviewer to the first slice that actually invokes SSH or exposes status in UI; record local adversarial evidence now.
- **Explicit non-goals:** SSH invocation, known-host parsing, credential helpers, authenticated network access, UI, IPC, clone/sync/push integration, private-key access.
- **Definition of done:** Native startup retains only a safely captured agent capability or a closed unavailable status; hostile path forms fail closed; focused and standard gates pass; docs and signed checkpoint match reality.
- **Expected commit boundary:** Core capture primitive, native launch retention, focused tests, and truthful docs form one commit; progress may follow separately.
- **Conditions requiring escalation:** Need to open keys, follow symlinks, trust writable ancestry, expose a path/descriptor to renderer, add dependencies, invoke SSH/Git/network, or broaden the environment/config boundary.
