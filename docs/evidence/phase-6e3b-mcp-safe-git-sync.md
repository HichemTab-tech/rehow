# Phase 6E3B evidence: MCP safe Git synchronization

Status: accepted; complete repository gate and Security Reviewer closure pass

## Implemented boundary

- `git_sync_plan` requires startup write+Git-write authority, exact `gitState`, retained recovery storage, the trusted native helper, and the shared per-Workspace Git lifecycle reservation.
- Native inspection approval occurs before authentication resource preparation or network access. Denial returns bounded facts without contacting the remote.
- Authenticated inspection and ancestry classification reuse only the accepted Phase 5D HTTPS/SSH resources captured at process launch. Real JSON-RPC cancellation reaches the core network cancellation flag.
- Results disclose only direction, closed ancestry, remote name/branch/transport, local-change and Workspace-match Booleans, action availability, reason, and optional expiry/token. URLs, refs, OIDs, paths, identities, credentials, raw output, and Git arguments remain private.
- Only clean `remoteFastForward` pull and `localAhead` push produce executable tokens. `git_sync` obtains a separate native approval and invokes only the retained core fast-forward or exact ordinary push authority.
- Protocol cancellation is honored through inspection. After mutation approval, pull/push run to the core's terminal or recovery state so cancellation cannot abandon ambiguous publication.
- Commit, sync, and both recovery lifecycles share one atomic Workspace reservation from pre-planning through terminal execution.
- `git_sync_recovery_plan` binds a random opaque token to the exact pending interrupted pull operation. `git_sync_recover` is separately approved, pathless, networkless, and refuses operation replacement.

## Verification

- `cargo test -p rehow-mcp --all-features`: 89 unit tests, 3 approval-helper integration tests, and 1 stdio integration test pass.
- `cargo test -p rehow-core sync_recovery_plan_is_bound_to_the_exact_interrupted_operation --lib`: pass.
- `cargo test -p rehow-core --test error_contract`: 18 tests pass.
- `cargo clippy -p rehow-mcp --all-targets --all-features -- -D warnings`: pass.
- `git diff --check`: pass.
- Complete repository gate: `pnpm check`, `pnpm test`, `pnpm build`, `cargo fmt --all --check`, strict all-Workspace/all-target/all-feature Clippy, Bootstrap validation, and whitespace checks pass.
- The complete gate includes 39 Vitest tests, 455 core tests (453 pass and 2 intentional network/performance ignores), 18 error-contract tests, 13 protocol tests, public-API/trybuild and unknown-YAML suites, 25 desktop Rust tests, 89 MCP unit tests, 3 approval-helper integration tests, and 1 stdio integration test.
- Bootstrap reports 17 required files, 48 ADRs, and 8 pinned agents. The installed Node 26.2.0 and pnpm 11.20.0 were used unchanged.
- Focused MCP regressions prove the exact forty-three-tool schema, closed inputs, capability/helper matrix, safe action matrix, exact host/authentication-policy consent and binding, URL/secret non-disclosure, native denial before network/mutation, apply denial/replay/expiry/scope handling, stable errors, real JSON-RPC cancellation at the network boundary, shutdown cancellation with reservation retention, and atomic shared lifecycle reservation.
- Accepted core suites continue to own successful/hostile HTTPS and SSH inspection, bounded ancestry, fast-forward publication/recovery, exact push/postflight, authentication, and redaction behavior.

The first Security Reviewer pass found missing destination/authentication facts in approval and binding, shutdown cancellation/reservation lifetime, adapter evidence gaps, and an indistinguishable uncertain-push postflight result. The implementation now binds and displays the safe host plus exact anonymous/helper/SSH policy without exposing them over MCP, uses a drop guard and worker-owned reservation during inspection, and returns a distinct bounded warning when a push may already exist so clients inspect instead of blindly retrying. The correction recheck passes with no remaining blocker, high, or medium finding.
