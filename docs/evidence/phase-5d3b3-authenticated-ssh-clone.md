# Phase 5D3B3 authenticated SSH clone evidence

Date: 2026-09-19

## Implemented boundary

- Native clone validation prepares opaque SSH resources only for a canonical SSH plan and maps missing agent, host keys, helper, or system SSH into closed actionable errors.
- Discovery and fetch reuse the accepted isolated clone validator, but enable only `protocol.ssh`, set the fixed proxy path, and mount no Workspace or private-key file.
- The system SSH executable is captured outside the user namespace and mounted by descriptor beside the proxy. The proxy executes only that synthetic path.
- Sealed host-key memfds are rewound in each Bubblewrap setup child before `--ro-bind-data`, allowing safe reuse for discovery and fetch without exposing source paths.
- Standard test, dev, and optimized no-bundle build scripts build and replace the helper as a one-link sibling of the consuming binary. The SSH test therefore cannot silently use an old helper left in `target/`. Final installer bundle layout remains a later packaging gate while Tauri bundles are disabled.
- SSH stderr is bounded and used only for closed authentication/host-key classification; raw output never crosses core/native boundaries.

## Verification fixture

The core test creates an isolated temporary Ed25519 host key, client key, SSH agent, unprivileged loopback `sshd`, Git repository, compatible Rehow manifest, trusted `known_hosts`, empty destination, and private staging root. It successfully performs real SSH discovery/fetch/Workspace validation through the product proxy and sandbox. A second sealed snapshot containing the wrong host key returns `HostKeyRejected`; both selected destinations remain empty. No public network or user credential is used.

## Gate status

Accepted on 2026-09-19:

- `pnpm check` passes.
- `pnpm test:ssh` passes the real loopback SSH fixture.
- `pnpm test` passes: 39 React/Vitest tests; 427 Rust core tests passed and 2 ignored; all desktop, MCP, protocol, public-API, unknown-YAML, and example targets passed.
- `pnpm build` passes.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` passes.
- `pnpm tauri:build` passes and produces the optimized no-bundle desktop binary.
- `git diff --check` passes.
- The one reused Security Reviewer found an obsolete native HTTPS-only guard and an equivalent renderer restriction, plus a stale-helper fixture risk. The native and renderer transport mappings are now closed and tested, SSH addresses reach the Rust authority, and the standard test scripts rebuild and one-link-copy the current helper before the fixture. The correction re-review verdict is **PASS**.
