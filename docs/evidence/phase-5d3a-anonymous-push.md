# Phase 5D3A anonymous push evidence

Date: 2026-09-19

## Implemented boundary

- `localAhead` is the only ancestry authority accepted for push.
- The core repeats the complete local Workspace/repository binding, re-discovers the exact upstream before publication, sends one internally constructed full-OID-to-full-branch refspec through ordinary `git push`, and verifies the exact upstream afterward.
- No force, force-with-lease, delete, mirror, wildcard, tag, upstream-setting, multi-ref, caller option, or caller Git identifier exists.
- The push child uses a cleared environment, sealed synthetic config, empty synthetic refs, the retained `HEAD`, read-only object storage, config/helper/prompt/proxy/redirect isolation, an HTTPS-only protocol allowlist, bounded output, and a bounded process group.
- Native code consumes the existing opaque single-use sync token, confirms remote/branch/transport/host out of band, blocks Workspace switching through the existing applying lifecycle, and returns a closed result.
- The Reference Desk offers Push only for `localAhead`; uncommitted changes are explicitly excluded, and errors/decline/success restore predictable focus and live status.
- If publication may have succeeded but postflight discovery cannot prove the final remote state, Rehow returns `remoteStateUncertain` and directs the user to inspect before retrying.

## Verification

- Focused core push tests: 4 passed. They cover exact config-free command policy, absence of force-capable arguments, local-ahead-only authority, exact pre/post binding, remote-race refusal, no publication after a changed preflight, and ambiguous postflight reporting.
- Desktop Rust tests: 24 passed, including closed payload/error/confirmation coverage.
- Vitest: 39 passed, including strict push payload validation, exact status refresh, scheduled focus restoration, and local-ahead action presentation.
- Full locked all-feature Workspace tests: core 416 passed with 2 intentional performance/network ignores; protocol/error/public-API/unknown-YAML, desktop 24, and MCP 1 passed.
- Locked all-target/all-feature Clippy for core and desktop with warnings denied: passed.
- `pnpm check`, `pnpm test`, `pnpm build`, contract drift, bootstrap validation, Cargo formatting, and whitespace checks: passed.
- Optimized Tauri no-bundle build: passed at `target/release/rehow-desktop`; the existing `.app` suffix warning remains informational.

No live external push was performed: doing so would mutate an external repository and require authorization. Deterministic injected preflight/publication/postflight tests exercise the production state machine, while command construction tests inspect the exact production argv, environment, and mounts.

## Review

Security Reviewer verdict: **PASS**, with no blocker, high, or medium findings remaining. Review corrections distinguished a missing upstream ref from a generic transport failure, refreshed Git status after successful publication, restored focus only after the updated control rendered, expanded adversarial lifecycle coverage, and removed active repository refs from the publication sandbox. The remaining low documentation finding was resolved by updating ADR 0025's implementation status.
