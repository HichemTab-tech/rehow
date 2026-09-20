# Task Packet: Phase 5D4 packed-current-branch publication

- **Objective:** Allow a validated remote fast-forward when the current local branch exists only in `packed-refs`, using an exact durable loose shadow without rewriting packed refs.
- **Required reasoning level:** xhigh for ref precedence/recovery policy; high for implementation.
- **Prerequisites:** Accepted Phase 5C packed-branch commit publication and Phase 5D2 durable sync intent/publication; signed Phase 5D3 transport completion.
- **Allowed files or subsystem:** Core Git parsing/publication tests plus ADR 0025, testing/implementation plans, this packet/evidence, and progress documentation.
- **Relevant specifications and ADRs:** ADR 0024, ADR 0025, `docs/architecture.md`, `docs/testing-strategy.md`, and `docs/threat-model.md`.
- **Exact required behavior:** Parse the complete bounded `packed-refs` file as before. When a valid current-branch loose ref and packed entry both exist, apply Git's ordinary loose-over-packed precedence. Permit a packed-only current branch to enter the existing sync intent as `Packed`; require the exact packed bytes and parent throughout pre-publication; publish the candidate index then a direct loose ref through the accepted no-follow CAS/recovery path; leave `packed-refs` byte-identical; reopen the result as a loose authoritative branch.
- **Important states and edge cases:** Packed-only before state; stale packed target under a valid loose shadow; malformed/duplicate/wrong-width packed entries; interruption before/after index/ref publication; foreign loose-ref race; packed-file drift; SHA-1/SHA-256 widths; nested branch names.
- **Security constraints:** No packed-ref mutation, deletion, repacking, maintenance process, shell, network, credential, renderer path/ref/OID, force, reset, or rollback after durable ref publication. Exact before/candidate states and foreign-state preservation remain mandatory.
- **Tests to add or update:** Parser precedence with differing loose/packed OIDs; packed-only sync fast-forward creates an exact loose shadow, preserves packed bytes, completes its intent, and reopens under the accepted policy; retain packed-branch commit/recovery and malformed packed-ref regressions.
- **Commands to run:** Focused packed/branch/sync tests; Cargo formatting; strict core Clippy; full `pnpm check` and `pnpm test`; bootstrap and whitespace checks.
- **Explicit non-goals:** General ref packing, pruning, rewriting `packed-refs`, unborn sync publication, remote-tracking refs, scheduled sync, credential changes, UI changes, or MCP mutation.
- **Definition of done:** Packed-only fast-forward succeeds through the existing recoverable publication boundary, exact packed bytes remain unchanged, parser/recovery regressions pass, docs match code, and the full gate is green.
- **Expected commit boundary:** Core policy/coverage and aligned packet/evidence/ADR documentation form one signed checkpoint; progress follows separately.
- **Conditions requiring escalation:** Any need to rewrite/delete packed refs, weaken full-file grammar/revision checks, change recovery ordering, expose ref/OID authority, or add a dependency.
