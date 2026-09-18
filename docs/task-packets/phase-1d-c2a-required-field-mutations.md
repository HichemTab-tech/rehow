# Task Packet: Phase 1D-C2a required-field and body mutations

- **Objective:** Produce validated, byte-preserving in-memory candidates for required scalar edits, Guide body replacement, and stale transitions.
- **Required reasoning level:** high because source-range edits and tested-state invalidation are durability/trust boundaries.
- **Prerequisites:** Signed C1 checkpoint `92e4c93`; ADRs 0011/0014; proven reconciliation artifacts and typed binders.
- **Allowed files or subsystem:** private `domain::content` mutation/emission modules, minimal binder accessors, focused tests/evidence, ADR 0014, and this packet.
- **Relevant specifications and ADRs:** ADR 0011 span edits; ADR 0014 mutation authority/effects/status rules; `docs/content-model.md` revision and tested-state sections.
- **Exact required behavior:** Support Workspace name; Category name/order/archive/restore; Guide title/summary/category/body/mark-stale. Validate payload before no-op; no-op preserves exact bytes even when read-only; actual changes require proven concrete ranges, use canonical replacement values, update `updated_at = max(current, stamp)`, stale tested or archived-from-tested Guides, apply at most eight nonoverlapping edits, production reparse/rebind, and verify intended/untouched values.
- **Important states and edge cases:** Same-value no-op, older/equal/newer stamp, draft/tested/stale/archived stale matrix, CRLF/BOM/quote-style preservation outside spans, body exact equality/change, comments outside replacement spans, read-only artifacts, missing/unreconciled/overlapping ranges, invalid text/body, allocation/size refusal, and candidate mismatch.
- **Security constraints:** Private typed intents only; no YAML fragments/ranges from callers; no I/O/clock/entropy/logging/process/network/adapter authority; content-free errors; tested is never created or preserved after an actual Guide edit.
- **Tests to add or update:** Golden before/after cases; no-op byte identity; read-only no-op versus actual refusal; tested/archive invalidation; old/new stamps; CRLF/BOM/unknown/comment preservation; invalid payload; candidate rebind; external API absence; forbidden-authority audit.
- **Commands to run:** Core format, locked all-feature Clippy/tests, bootstrap and whitespace checks.
- **Required reviews and artifacts:** Evidence file now; one combined correctness/security review only after remaining C2b optional/collection/archive insertion-removal behavior is complete.
- **Explicit non-goals:** Optional-field insertion/removal, collection replacement, archive/restore Guide fields, Workspace default Category, Category summary, filesystem application, revision checks, and adapters.
- **Definition of done:** Supported intents meet exact byte/status/timestamp semantics, focused/adversarial tests and core gate pass, public API remains absent, and a signed checkpoint exists.
- **Expected commit boundary:** One C2a implementation/evidence commit and one progress ledger commit.
- **Conditions requiring escalation:** Caller-controlled syntax/ranges, inability to prove outside-span identity, effective-tested ambiguity, need for I/O, or parser/binder disagreement.
