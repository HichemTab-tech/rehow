# Phase 5D3C2 authenticated SSH push evidence

Date: 2026-09-20

## Implemented boundary

- The fixed proxy accepts only the existing upload-pack service or exact `git-receive-pack` with the same bounded destination/path grammar; every other service and hostile argv remains rejected before OpenSSH.
- Native push prepares sealed SSH resources only for a core-validated SSH classification and only after the existing native confirmation.
- Preflight discovery, ordinary push, and postflight discovery share the accepted sealed authority. The push child receives read-only local objects and synthetic Git metadata, one exact internal refspec, and no writable Workspace.
- Missing resources, authentication failure, host-key refusal, remote change, rejection, timeout, and uncertain postflight remain closed outcomes. Renderer IPC gains no transport, URL, path, ref, OID, helper, or credential choice.

## Verification fixture

The disposable loopback fixture now uses a temporary bare upstream. A cloned compatible Workspace creates one local commit, inspects/classifies the SSH upstream as `localAhead`, and first attempts publication with a sealed wrong host key. The operation returns `HostKeyRejected` and the upstream branch remains at the exact old commit. A fresh classification with the trusted resources performs a normal push; the upstream branch then equals exactly the validated local commit. No public network or user credential is used.

## Gate status

Focused proxy, push-policy, loopback SSH, and native error tests pass. `pnpm check`, strict all-Workspace Clippy, 39 Vitest tests, 427 core tests with 2 intentional ignores, all contract/API/desktop/MCP targets, production build, optimized Tauri no-bundle build, Cargo formatting, bootstrap validation, and whitespace checks pass. One unrelated host-key provenance boundary test failed once during a test/build parallel run, then passed five focused repetitions and the clean sequential full suite.

The single reused Security Reviewer inspected the completed authority boundary and returned **PASS** with no findings.
