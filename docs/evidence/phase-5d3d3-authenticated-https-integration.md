# Phase 5D3D3 authenticated HTTPS integration evidence

Date: 2026-09-20

## Gate status

Accepted. Implementation, full regression verification, and the single focused Security Review are complete.

## Implemented boundary

- Native launch kernel-copies and seals the ordinary GitHub CLI `hosts.yml` only after private, one-link, effective-user-owned, bounded regular-file and stable-ancestry checks. Rehow never parses or returns its contents, and later source mutation cannot change the snapshot. Direct helpers require no backing file; `gh` fails closed when its backing is absent or unsafe.
- A closed `GitRemoteAuthentication` value selects anonymous HTTPS, authenticated HTTPS, or authenticated SSH. Authenticated HTTPS binds the canonical host and explicit port, mounts only fixed proxy/helper/backing descriptors, resets every other helper, and reuses the accepted discovery, staged fetch/history, and exact push pipelines. IPv6-literal URLs remain explicitly rejected by the pre-existing conservative remote policy.
- No renderer contract changed. HTTPS without trusted launch provenance remains anonymous. Authentication failures map to closed errors without raw Git/helper output.
- Git internally launches its fixed absolute credential helper through `/bin/sh`; the sandbox exposes `/bin` as the standard `/usr/bin` link. No user, repository, remote, or renderer value contributes shell syntax, helper selection, arguments, or environment.

## Focused evidence

- Provenance/resource and command-surface tests cover direct and exact `gh` selection, private backing type/mode/link/ancestry, fixed descriptor mounts, redaction, canonical host with explicit port, discovery/fetch/history/push helper isolation, anonymous/SSH absence, exact push semantics, IPv6-literal refusal, and closed transport-neutral authentication classification.
- Four proxy tests cover real Git EOF framing, exact host/protocol/operation grammar, secret-on-get rejection, response allowlisting (including exact `gh` protocol/host echoes and mismatches), fixed argv/environment, bounds, and silent failures.
- The packaged networkless smoke runs real Git `credential fill` through the packaged proxy and a disposable canary helper. The canary is captured only by the test assertion and never logged.
- Existing anonymous command-shape tests and the authenticated-SSH clone/sync loopback regression pass unchanged.

## Combined gate

- `pnpm check` passed: engine pin, Biome, desktop/contracts TypeScript, and generated-contract verification.
- `pnpm test` passed: 39 Vitest tests; 440 core tests with 2 explicit network/performance ignores; 18 error-contract, 13 protocol, public-API/trybuild, 11 unknown-YAML, 25 native desktop, MCP/example, and packaged credential-proxy checks.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` passed.
- `cargo fmt --all` and `git diff --check` passed.
- The reused Security Reviewer rechecked the IPv6 policy, transport-neutral authentication copy, and every network command surface, then returned **PASS** with no remaining blocker, high, or medium finding.
