# Task Packet: Phase 1D-C1 canonical new-document emission

- **Objective:** Produce deterministic, validated in-memory bytes for new Workspace manifests, Categories, and draft Guides, then reparse and rebind them before return.
- **Required reasoning level:** high because emitted bytes become future canonical content.
- **Prerequisites:** Signed Phase 1D-B checkpoint `0b5fb11`; accepted parser and read-only binders; ADRs 0011 and 0014.
- **Allowed files or subsystem:** `crates/rehow-core/src/domain/content.rs`, a new private emission module beneath it, focused core tests, ADR 0014, content model, evidence, and this packet.
- **Relevant specifications and ADRs:** `docs/content-model.md`; ADR 0011 fixed-schema emitter; ADR 0014 canonical new-document emitter and no-I/O boundary.
- **Exact required behavior:** Emit schema-v1 LF/no-BOM documents in schema order; quote every string with the frozen escape policy; emit integers and fixed enums plainly; omit absent optional fields; make new Categories active and Guides draft with required empty Tags/Aliases; require canonical Guide body bytes; parse and bind the candidate and compare identity/core values before returning a private bound artifact.
- **Important states and edge cases:** Quotes, reverse solidus, controls, bidi controls, `U+FEFF`, Unicode, `i64::MIN/MAX`, optional default Category and summary, invalid/oversized/trimmed/noncharacter text, malformed Guide body, output/parser limits, and allocation refusal.
- **Security constraints:** Pure memory only; no filesystem, clock, entropy, logging, process, network, adapter, or public construction authority. Errors contain no input content. All retained output growth is fallible.
- **Tests to add or update:** Exact golden bytes for all three documents; escape matrix; optional omission/presence; integer extremes; invalid input/body; deterministic repeat emission; parser/binder round trip; source audit for forbidden authority.
- **Commands to run:** `cargo fmt --all --check`; `cargo clippy -p rehow-core --all-targets --all-features --locked -- -D warnings`; `cargo test -p rehow-core --all-features --locked`; `git diff --check`.
- **Required reviews and artifacts:** Record golden/evidence results. C1 may checkpoint after tests; one combined independent correctness/security review occurs once at the complete 1D-C boundary.
- **Explicit non-goals:** Editing existing bytes, mutation/status transitions, filesystem writes, IDs/timestamps generation, storage revisions, Attachment file handling, public/adapter APIs, or effective tested trust.
- **Definition of done:** All three emitters are private, deterministic, reparse/rebind successfully, exact tests and core gate pass, docs match, and a signed recoverable checkpoint exists.
- **Expected commit boundary:** One signed C1 implementation/evidence commit followed by a progress ledger commit.
- **Conditions requiring escalation:** Inability to reproduce the frozen escape format, parser/emitter disagreement, need for I/O/clock/entropy, public unchecked construction, or a new dependency.
