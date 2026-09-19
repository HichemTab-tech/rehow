# Task Packet: fixed Git SSH proxy

- **Objective:** Compile a dedicated proxy that accepts only Git's upload-pack SSH argv and invokes root-owned OpenSSH with a fixed noninteractive policy.
- **Required reasoning level:** high.
- **Prerequisites:** ADRs 0023/0025 and Phase 5D3B1's retained agent capability.
- **Allowed files or subsystem:** `crates/rehow-core` proxy binary/module and tests; root/Cargo locks only if required; ADR 0025; architecture/threat/testing docs; evidence and `PROGRESS.md`. No desktop command or network-operation integration.
- **Relevant specifications and ADRs:** ADR 0023 SSH boundary; ADR 0025 Phase 5D3B; Git process policy and threat model.
- **Exact required behavior:** A dedicated binary parses optional exact Git protocol/port pairs, one canonical SSH destination, and one single-quoted `git-upload-pack` path. It reconstructs all argv, clears the environment, and invokes only trusted `/usr/bin/ssh` with `/dev/null` config, strict host checks, fixed known-host/agent paths, public-key-only batch authentication, forwarding/proxy/local-command/control disabling, and fixed timeouts.
- **Important states and edge cases:** missing/extra/non-UTF-8 argv; duplicate/unknown flags; zero/noncanonical port; mixed-case/invalid/multi-user host; receive-pack or extra remote command; traversal/metacharacter/quote path; missing trusted SSH executable; signal/non-byte exit.
- **Security constraints:** No shell, arbitrary option, config, executable, host/path, environment, prompt, password, forwarding, proxy, logging, or filesystem access chosen by callers. The proxy emits no diagnostic content itself.
- **Tests to add or update:** Exact accepted grammar; adversarial argv corpus; exact command/program/environment policy; binary compilation. Do not perform a live SSH connection.
- **Commands to run:** focused proxy tests; Cargo format/Clippy/full tests; `pnpm check`/build; optimized Tauri build if core binary topology affects it; bootstrap/whitespace checks.
- **Required reviews and artifacts:** Defer the one Security Reviewer until host-key provenance and sandbox descriptor handoff complete the invocation boundary.
- **Explicit non-goals:** Agent/known-host mounting, live SSH, clone/sync/push integration, HTTPS helpers, UI/IPC, receiving pushes, arbitrary SSH commands.
- **Definition of done:** The proxy binary compiles, rejects every non-upload-pack grammar tested, constructs only the fixed OpenSSH policy, passes gates, and remains unreachable from product network operations.
- **Expected commit boundary:** Proxy binary/module, adversarial tests, and aligned docs/evidence form one signed implementation checkpoint; progress may follow separately.
- **Conditions requiring escalation:** Need for a shell, free-form SSH option/command, writable config, renderer input, private-key path, live credential/network use, dependencies, or protocol grammar broader than documented Git invocation.
