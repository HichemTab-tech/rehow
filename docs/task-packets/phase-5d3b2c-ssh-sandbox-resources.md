# Task Packet: sealed SSH sandbox resources

- **Objective:** Convert trusted launch-time agent/proxy/host-key provenance into exact fixed Bubblewrap mounts without exposing user paths or enabling product SSH.
- **Required reasoning level:** xhigh.
- **Prerequisites:** Phase 5D3B1, 5D3B2A, and 5D3B2B signed checkpoints.
- **Allowed files or subsystem:** core SSH sandbox module/proxy constants/tests, native launch state, ADR 0025, architecture/threat/testing docs, evidence, and `PROGRESS.md`. No clone/sync/push caller.
- **Relevant specifications and ADRs:** ADRs 0023/0025; Git process policy; credential/filesystem threats.
- **Exact required behavior:** Capture only the exact sibling proxy executable with safe owner/mode/type/link/ancestry and held inode. Prepare one-use resources only when agent, host keys, and proxy are trusted. Combine bounded host-key snapshots into sealed read-only memfds. Mount only held agent/proxy inodes and sealed key files at fixed synthetic paths; clear close-on-exec only for the exact Bubblewrap setup descriptors.
- **Important states and edge cases:** missing/unsafe proxy; unavailable agent/keys; writable/symlink proxy; combined overflow; memfd/seal failure; descriptor inheritance; absent Workspace mount; network namespace isolation.
- **Security constraints:** No caller path, shell, repository/user config, private key, renderer IPC, logging, public network, product Git operation, or writable user file. Exact descriptors are consumed by Bubblewrap before child exec.
- **Tests to add or update:** Proxy provenance/redaction/refusal; unavailable-resource mapping; required memfd seals; networkless Bubblewrap visibility of only fixed socket/key/proxy mounts; no Workspace path; empty output.
- **Commands to run:** focused tests; format/strict Clippy/full Workspace tests; `pnpm check`; production/optimized native build; bootstrap/whitespace checks.
- **Required reviews and artifacts:** Defer the one Security Reviewer to the first completed product SSH invocation, which must include cancellation/output/auth/host-key canaries.
- **Explicit non-goals:** Live SSH, product clone/sync/push integration, helper credentials, UI/IPC, packaging the sidecar, TOFU/prompts, arbitrary commands.
- **Definition of done:** Exact capabilities become sealed fixed sandbox mounts in a networkless adversarial test, remain unreachable from product operations, and all gates/docs/signed checkpoint pass.
- **Expected commit boundary:** Core/native sandbox resources plus tests and aligned docs/evidence form one signed checkpoint; progress may follow separately.
- **Conditions requiring escalation:** Need for path-based recapture, unsealed bytes, extra inherited descriptors, broader mounts, shell/config flexibility, public network, credentials, dependencies, or renderer authority.
