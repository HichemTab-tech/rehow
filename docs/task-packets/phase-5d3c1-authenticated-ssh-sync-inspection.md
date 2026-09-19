# Task Packet: authenticated SSH sync inspection

- **Objective:** Allow the existing read-only sync inspection and bounded ancestry classification to use the accepted authenticated SSH transport resources.
- **Required reasoning level:** xhigh.
- **Prerequisites:** Signed Phase 5D3B3 checkpoint `547d55f`; accepted anonymous-HTTPS sync inspection/classification; accepted sealed SSH transport resources.
- **Allowed files or subsystem:** `crates/rehow-core/src/git.rs`, native sync adapter/tests, ADR 0025, Git architecture/threat/testing docs, this packet/evidence, and `PROGRESS.md`.
- **Relevant specifications and ADRs:** ADR 0025; `docs/architecture.md` Git boundary; `docs/testing-strategy.md` Git fixtures; `docs/threat-model.md` SSH and credential controls.
- **Exact required behavior:** A sync plan with canonical SSH upstream prepares native-held launch resources, runs remote-ref discovery, initial fetch, complete Workspace validation, bounded history deepening, and ancestry classification through the fixed SSH proxy. HTTPS remains unchanged. Missing resources and SSH authentication/host-key failures are closed actionable errors.
- **Important states and edge cases:** Up-to-date, local-ahead, remote-fast-forward, diverged, history-limit, cancellation, stale plan, staging limit/conflict, missing launch resources, wrong host key, rejected agent identity, and reusable sealed descriptors across all network subprocesses.
- **Security constraints:** No push, active Workspace/Git mutation, private-key read, arbitrary SSH/config/helper path, shell, prompt, TOFU, repository/user config, raw stderr, public host, or renderer credential/path authority.
- **Tests to add or update:** Exact SSH sync discovery/history command policy; exhaustive native error mapping; disposable loopback `sshd`/agent/repository sync inspection and ancestry classification; wrong-host refusal; unchanged active manifest, HEAD, index, and config.
- **Commands to run:** Focused SSH fixture; `pnpm check`; `pnpm test`; strict all-Workspace Clippy; production build; optimized Tauri no-bundle build; whitespace/bootstrap checks.
- **Required reviews and artifacts:** One reused Security Reviewer only after the complete local authority gate; evidence file with fixture and verified gate results.
- **Explicit non-goals:** Authenticated push, HTTPS credential helpers, UI redesign, private-key files, installer bundle changes, scheduling, packed-branch publication, or MCP mutation.
- **Definition of done:** SSH sync inspection/classification succeeds only through sealed launch authority, safely reports failures, leaves active state unchanged, preserves HTTPS behavior, and passes the complete gate and Security Review.
- **Expected commit boundary:** Core/native/test/docs implementation in one signed commit; verified progress ledger may follow separately.
- **Conditions requiring escalation:** Any need for arbitrary paths/config, credential storage/logging, public credentials/network, active mutation, new dependency, destructive Git behavior, or unresolved security finding.
