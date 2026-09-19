# Task Packet: trusted known-hosts provenance

- **Objective:** Snapshot optional launch-time user and system `known_hosts` as bounded, stable, path-redacted native trust material without enabling SSH.
- **Required reasoning level:** high.
- **Prerequisites:** Phase 5D3B1 agent capability and 5D3B2A fixed proxy.
- **Allowed files or subsystem:** `crates/rehow-core/src/git.rs`, native state construction, focused tests, ADR 0025, architecture/threat/testing docs, evidence, and `PROGRESS.md`. No proxy invocation or renderer surface.
- **Relevant specifications and ADRs:** ADR 0023 SSH trust boundary; ADR 0025 Phase 5D3B; Git process/threat/testing policies.
- **Exact required behavior:** At native startup, inspect `$HOME/.ssh/known_hosts` and fixed `/etc/ssh/ssh_known_hosts{,2}`. Missing files are optional. Existing sources must be bounded regular one-link files with non-writable mode, expected user/root ownership, and safe non-symlink ancestry. Open no-follow, compare path/descriptor identities, read twice with hard caps, and retain only stable bytes. Any unsafe existing source makes the closed result unsafe and discards all snapshots.
- **Important states and edge cases:** missing/relative/oversized HOME; absent files; symlink/hardlink/directory/socket; loose file/parent mode; wrong owner; size exact/one-over; replacement or mutation; empty/non-UTF-8 content; multiple sources.
- **Security constraints:** No SSH/Git process, socket connection, network, key read, config include, renderer IPC, path/content logging, or mutation. Debug exposes only closed status/count.
- **Tests to add or update:** Trusted user snapshot/redaction/immutability; missing status; invalid HOME; symlink/hardlink/loose mode/oversize refusal; unsafe source clears trusted peers; exact size cap where practical.
- **Commands to run:** focused Cargo tests; formatting/strict Clippy/full Workspace tests; `pnpm check`/build; optimized native build; bootstrap/whitespace checks.
- **Required reviews and artifacts:** Defer the one Security Reviewer to completed sandbox descriptor handoff/invocation.
- **Explicit non-goals:** Parsing host-key algorithms, TOFU, prompts, proxy invocation, agent mount, live SSH, clone/sync/push integration, UI/IPC, credential helpers.
- **Definition of done:** Startup retains only stable bounded host-key snapshots or a closed unavailable status; hostile filesystem forms fail closed; all gates pass; docs and signed checkpoint match reality.
- **Expected commit boundary:** Core/native capture, adversarial tests, and aligned docs/evidence form one signed checkpoint; progress may follow separately.
- **Conditions requiring escalation:** Need for mutable/path-based trust, prompts/TOFU, private keys, live credentials/network, renderer authority, new dependencies, or weakened ownership/ancestry checks.
