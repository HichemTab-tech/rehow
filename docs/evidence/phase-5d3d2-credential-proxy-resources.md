# Phase 5D3D2 credential proxy and resources evidence

Date: 2026-09-20

## Gate status

Complete. Verification and the focused Security Review pass.

## Implemented boundary

- The dedicated proxy accepts only `get`, `store`, or `erase`, one closed direct/`gh` kind, and one bounded expected HTTPS host. The request must be a complete LF record with exact protocol/host, allowlisted unique scalar fields, bounded repeatable fields, and no secret-bearing lookup input.
- The proxy directly invokes only `/home/rehow/bin/credential-helper`; direct mode appends the operation, while `gh` mode reconstructs exactly `auth git-credential <operation>`. Its environment is cleared and synthetic, stdin/stdout/stderr and time are bounded, the helper receives a fresh process group, output is allowlisted before forwarding, and every failure is silent.
- Native launch retains the separately captured sibling proxy. Resource preparation clones only the accepted proxy and helper descriptors, redacts `Debug`, and mounts them read-only at fixed paths. A networkless Bubblewrap test proves both executable mounts and no `/workspace`.
- The existing bounded child runner now terminates the complete child process group on time/output failure and after the leader exits. It keeps deadline/output enforcement active until every pipe closes, closing the inherited-pipe descendant hang found during Security Review.

## Focused evidence

Four proxy tests cover request/response grammar, exact direct and `gh` argv, cleared environment, a real empty direct process, hostile argv/kind/host, secrets, duplicates, controls, and bounds. One resource test covers trusted/redacted proxy capture, missing/unsafe provenance, redacted preparation, fixed descriptor mounts, network isolation, and absent Workspace. The native test covers launch-only retention. The packaged binary exits 255 with zero output on invalid standalone invocation. A regression test proves that a successful helper leader cannot hang the runner by leaving a pipe-holding descendant behind.

## Combined gate

`pnpm check`, the packaged networkless proxy smoke test, 39 Vitest tests, 438 core tests with 2 intentional ignores, all contract/API/desktop/MCP targets, strict all-Workspace Clippy, production build, optimized Tauri no-bundle build, Cargo formatting, bootstrap validation, and whitespace checks pass. The single reused Security Reviewer found one inherited-pipe descendant hang; the bounded runner correction and regression test were rechecked with a final **PASS** verdict.
