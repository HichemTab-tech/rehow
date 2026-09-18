# Task Packet: Recovery journal v1 codec

- **Objective:** Freeze and implement the bounded canonical journal/state model required before any Workspace mutation.
- **Required reasoning level:** xhigh architecture/security boundary; implementation is isolated and pure.
- **Prerequisites:** accepted ADRs 0003/0009, protocol UUIDv7/timestamp/revision types, BLAKE3 pin.
- **Allowed files or subsystem:** ADR 0009; this packet; `rehow-core::storage::recovery`; storage module declaration; progress.
- **Relevant specifications and ADRs:** architecture Atomic writes, content-model path limits, threat model partial writes/concurrency, ADRs 0002/0003/0009.
- **Exact required behavior:** encode/decode journal v1 as definite canonical CBOR with exact integer-key maps, closed operation/stage/state discriminants, canonical protocol text values, 32-byte hashes, ordered targets, strict completion invariants, and no accepted alternate encoding.
- **Important states and edge cases:** absent/existing expected state; absent/object/trash intended state; prepared/swapped/directory-synced/complete targets; prepared/applying/committed/conflict journal; empty/single/max targets; truncation; trailing/corrupt/noncanonical/unknown data; duplicate paths; invalid modes and paths.
- **Security constraints:** 4 MiB input, 1,024 targets, 1,024-byte paths, 255-byte temp components; bounded allocation before growth; no content/path text in errors or Debug; no filesystem/process/network/logging/write authority; reject floats/tags/indefinite values and non-shortest integers.
- **Tests to add or update:** frozen byte vector; round trips for every discriminant; every-prefix truncation; noncanonical integers, reordered/duplicate/extra keys, wrong major types, bad protocol values/hashes/paths/modes/stages, caps and trailing bytes.
- **Commands to run:** format; focused recovery tests; locked full core tests; core Clippy warnings denied; bootstrap/diff checks.
- **Required reviews and artifacts:** no extra agent during implementation; include this boundary in the one combined Security Reviewer gate after B2/B3.
- **Explicit non-goals:** XDG directories, object files, locks, fsync, rename/exchange, fault injection, recovery execution, Workspace mutation, UI/MCP.
- **Definition of done:** the canonical vector is frozen, hostile inputs fail closed with content-free errors, full core gates pass, and no I/O authority exists.
- **Expected commit boundary:** ADR schema addendum + pure codec/model/tests.
- **Conditions requiring escalation:** a dependency becomes necessary, any schema field is ambiguous, a protocol type cannot enforce canonicality, or decoder limits cannot precede allocation.
