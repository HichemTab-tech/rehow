# Task Packet: authenticated SSH clone

- **Objective:** Enable canonical SSH clone discovery/fetch through the launch-captured agent, host keys, fixed proxy, and sealed sandbox resources, without exposing credentials or changing the selected destination.
- **Required reasoning level:** xhigh.
- **Prerequisites:** Signed Phase 5D3B1, 5D3B2A, 5D3B2B, and 5D3B2C checkpoints; accepted anonymous-HTTPS clone validator.
- **Allowed files or subsystem:** `crates/rehow-core/src/git.rs`, `git/ssh_proxy.rs`, `git/ssh_sandbox.rs`, native clone adapter, root build scripts, this packet/evidence, ADR 0025, architecture/threat/testing/development/installation docs, and `PROGRESS.md`.
- **Relevant specifications and ADRs:** ADRs 0023 and 0025; `docs/architecture.md` Git boundary; `docs/threat-model.md` credential, proxy, host-key, and descriptor threats.
- **Exact required behavior:** SSH clone requires all trusted launch resources; mounts exact helper, system SSH client, agent socket, and sealed host keys at fixed paths; enables only SSH protocol; runs existing bounded discovery/fetch/validation; returns closed unavailable/authentication/host-key errors; keeps the destination empty.
- **Important states and edge cases:** Missing/unsafe agent, keys, helper, or system SSH; host-key mismatch; auth rejection; timeout/cancel/output cap; descriptor reuse across discovery and fetch; staging conflict; destination drift.
- **Security constraints:** No private-key read, arbitrary SSH/config/helper path, shell, prompt, TOFU, repository/user config, Workspace mount, raw stderr, public test host, or renderer credential authority. HTTPS behavior remains unchanged.
- **Tests to add or update:** Exact SSH command/protocol environment; stable stderr classification; reusable sealed descriptors; standard helper placement with one-link provenance; disposable loopback `sshd` success and wrong-host-key refusal using an ephemeral agent/key/repository.
- **Commands to run:** `pnpm test:ssh`; format; strict core/desktop Clippy; `pnpm test` (builds a current one-link helper before the full locked Workspace tests); `pnpm check`; production desktop and optimized Tauri build; whitespace/bootstrap checks.
- **Required reviews and artifacts:** One reused Security Reviewer after the complete invocation gate; evidence file with local-only fixture and verification results.
- **Explicit non-goals:** Authenticated sync/push, HTTPS credential helpers, user prompts, private-key files, final installer bundle sidecar layout, arbitrary SSH configuration, MCP mutation.
- **Definition of done:** Product SSH clone validates a compatible Workspace through only sealed launch authority, maps failures safely, leaves the destination unchanged, passes the gate and Security Review, and records the next exact action.
- **Expected commit boundary:** Core/native/build/test/docs implementation in one signed commit; verified progress ledger may follow separately.
- **Conditions requiring escalation:** Need for private-key or arbitrary path access, user/repository SSH config, credential logging, public credentials/network, destructive Git behavior, dependency additions, or unresolved reviewer finding.
