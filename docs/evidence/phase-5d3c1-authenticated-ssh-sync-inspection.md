# Phase 5D3C1 authenticated SSH sync inspection evidence

Date: 2026-09-20

## Implemented boundary

- Core sync discovery, initial fetch, and bounded history deepening accept the same optional opaque SSH resources as clone and fail closed when an SSH plan lacks them.
- Native sync derives transport only from the core-validated configured upstream and prepares resources only from launch-captured agent, host-key, proxy, and system-SSH provenance.
- Every SSH child retains the existing protocol allowlist, cancellation, timeout, output, staging, revision, and active-state checks. Authentication and host-key failures are closed actionable errors; raw stderr remains private.
- The transport-independent validated classification continues to drive the existing apply boundary; inspection adds no push or renderer authority.

## Verification fixture

The accepted disposable loopback SSH fixture now continues after clone through a separate initialized Workspace whose configured upstream uses the same SSH repository. It completes remote-ref discovery, initial fetch, canonical Workspace validation, bounded history deepening, and ancestry classification through the sealed launch resources. Exact before/after bytes for `rehow.yaml`, `.git/HEAD`, `.git/index`, and `.git/config` are unchanged. Repeating inspection with a sealed wrong host key returns `HostKeyRejected` and again leaves those active files unchanged.

## Gate status

Accepted on 2026-09-20. The focused loopback SSH, command-policy, and native error-contract tests pass. `pnpm check`, `pnpm test` (39 Vitest; 427 core passed and 2 ignored; all protocol, API, desktop, MCP, and example targets), strict all-Workspace Clippy, production pnpm build, optimized Tauri no-bundle build, bootstrap validation, and whitespace checks pass. The one reused Security Reviewer verdict is **PASS** with no blocker, high, or medium finding.
