# Task Packet: authenticated SSH push

- **Objective:** Publish one exact validated local-ahead commit to its configured SSH upstream through the sealed launch transport and normal server-enforced fast-forward semantics.
- **Required reasoning level:** xhigh.
- **Prerequisites:** Signed Phase 5D3C1 checkpoint `cfc2d1a`; accepted anonymous-HTTPS exact push; accepted SSH clone/sync resources and loopback fixture.
- **Allowed files or subsystem:** Core SSH proxy and Git push boundary, native push adapter/tests, ADR 0025, Git architecture/threat/testing/install docs, this packet/evidence, and `PROGRESS.md`.
- **Relevant specifications and ADRs:** ADR 0025 Sections 5D3A–5D3C1; `docs/architecture.md` Git process policy; `docs/threat-model.md` credentials, proxy grammar, remote races, and destructive Git controls.
- **Exact required behavior:** The fixed proxy accepts only exact upload-pack or receive-pack commands with the existing safe path grammar. A validated SSH `localAhead` classification prepares sealed launch resources, repeats exact preflight discovery, sends one internally constructed full-OID-to-full-branch refspec with no force semantics, and verifies exact postflight state.
- **Important states and edge cases:** Missing resources, auth failure, wrong host key, remote changed before push, server rejection, timeout/uncertain postflight, local plan drift, and successful ordinary fast-forward.
- **Security constraints:** No force/lease/delete/tag/wildcard, caller refspec/OID/URL/path, shell, arbitrary remote command, prompts, TOFU, private-key files, repository-selected helper/config, writable Workspace mount, raw stderr, or renderer credential authority.
- **Tests to add or update:** Proxy upload/receive exact grammar and hostile command corpus; SSH push command/env/refspec; disposable loopback bare-repository fast-forward success; wrong-host refusal; remote-race/uncertain behavior remains covered by existing state-machine tests.
- **Commands to run:** Focused proxy/SSH fixture/native error tests; `pnpm check`; `pnpm test`; strict all-Workspace Clippy; production and optimized Tauri builds; bootstrap/whitespace checks.
- **Required reviews and artifacts:** One reused Security Reviewer after the complete authority gate; evidence with exact fixture and gate results.
- **Explicit non-goals:** Force push, arbitrary refs, tags, branch selection, HTTPS credential helpers, UI redesign, scheduling, packed-branch publication, or MCP mutation.
- **Definition of done:** Product SSH push publishes only the validated local-ahead commit, verifies the server result, exposes no new authority, passes the full gate and Security Review, and records the next exact action.
- **Expected commit boundary:** Core/native/test/docs implementation in one signed commit; verified progress ledger may follow separately.
- **Conditions requiring escalation:** Need for arbitrary remote commands/config/paths, credentials outside the accepted agent, destructive Git behavior, new dependency, public credentials/network, or unresolved security finding.
