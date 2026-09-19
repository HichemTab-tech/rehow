# Task Packet: Bounded synchronization ancestry

- **Objective:** Classify the active branch and retained validated remote snapshot without mutating the active repository.
- **Required reasoning level:** xhigh for the Git authority boundary.
- **Prerequisites:** Signed Phase 5D1 checkpoint `713eb7d`; ADR 0025; retained opaque validated staging.
- **Allowed files or subsystem:** Core Git module, ADR 0025, focused evidence, and progress ledger. No native/UI/MCP/dependency changes.
- **Relevant specifications and ADRs:** Git sections of architecture, threat model, testing strategy, and implementation plan; ADRs 0006, 0023–0025.
- **Exact required behavior:** Revalidate local state; return up-to-date for equal heads; otherwise deepen only the exact staged remote OID to 4,096 commits; revalidate the remote tree; read bounded local/remote histories; report remote-fast-forward/local-ahead/diverged only when proven and history-limit otherwise; revalidate local state again.
- **Important states and edge cases:** Equal heads, either head contained by the other, complete divergence, shallow history, cap reached, malformed/duplicate/wrong-width history, cancellation, resource limits, local drift, and remote tree mismatch.
- **Security constraints:** Explicit argv, no shell/config/helpers/hooks/prompts, remote writes confined to private staging, active repository read-only, redacted output, and no active objects/index/refs/content mutation.
- **Tests to add or update:** Classification matrix, strict parser/shallow state, exact depth command, formatting, strict Clippy, and full core tests.
- **Commands to run:** Focused sync tests; `cargo fmt --all --check`; strict locked core Clippy; locked all-feature core tests; bootstrap and whitespace checks.
- **Explicit non-goals:** Integration, merge/rebase, push, credentials, scheduling, native/UI/MCP exposure, and staging cleanup.
- **Definition of done:** Every positive ancestry result is proven within bounds; uncertainty is explicit; active state remains unchanged; tests pass; signed checkpoint and recovery ledger exist.
- **Expected commit boundary:** One core/docs implementation checkpoint and one progress ledger checkpoint.
- **Conditions requiring escalation:** Any need to mutate active state, trust incomplete history, expose authority, add credentials, or weaken accepted validation.
