# Phase 5D4 packed-current-branch publication evidence

Date: 2026-09-20

## Gate status

Accepted. Implementation, focused verification, full regression verification, and focused Security Review are complete.

## Implemented boundary

- The existing bounded parser still validates the complete `packed-refs` file and rejects malformed, duplicate, or wrong-width target entries.
- A valid loose current-branch ref now follows Git's normal precedence over a possibly stale packed entry. The loose OID is authoritative while the packed file remains part of the exact repository snapshot.
- Packed-only sync publication records the existing `Packed` intent representation, preserves exact packed bytes, publishes the verified candidate index first, and creates the candidate loose ref last through the accepted recovery/CAS path.
- Rehow does not rewrite, delete, prune, repack, or otherwise maintain `packed-refs`. No network, credential, renderer, UI, or MCP boundary changed.

## Focused evidence

- Parser coverage proves a differing valid loose OID shadows the packed OID while invalid packed content still fails closed.
- The packed-only fast-forward fixture proves the remote candidate becomes `HEAD`, `packed-refs` stays byte-identical, a direct loose branch appears, the completed state reopens as `Loose`, and the sync intent reaches `Complete`.
- The existing packed-branch commit interruption/recovery fixture remains green and exercises the same absent-before-ref to candidate-loose-ref transition.

## Combined gate

- Five focused packed-ref tests and six focused sync-publication tests pass.
- `pnpm check` passes engine, Biome, TypeScript, and generated-contract verification.
- `pnpm test` passes 39 Vitest tests; 440 core tests with 2 explicit network/performance ignores; 18 error-contract, 13 protocol, public-API/trybuild, 11 unknown-YAML, 25 native desktop, MCP/example, and packaged helper checks.
- Strict all-feature/all-target core Clippy, Cargo formatting, bootstrap validation, and whitespace checks pass.
- The reused Security Reviewer checked precedence, full-file binding, packed-only recovery, drift/foreign-state behavior, and authority boundaries and returned **PASS** with no remaining blocker, high, or medium finding.
