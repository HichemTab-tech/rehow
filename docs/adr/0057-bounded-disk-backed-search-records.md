# ADR 0057: Bounded disk-backed search records and lazy Guide bodies

- Status: Accepted
- Date: 2026-09-23
- Supersedes: ADR 0018's retained in-memory field representation; its ranking and performance requirements remain active.

## Context

The accepted scanner permits up to 10,000 Guides and 2 GiB of managed Markdown/YAML. A compatible `WorkspaceView` retained every parsed Guide body and ADR 0018 retained a second normalized full-text copy. The 5,000-Guide/50 MiB benchmark passed, but memory scaled with all source and normalized text and therefore could not safely approach the documented scan envelope.

## Decision

`WorkspaceView` retains typed Guide metadata, relative paths, revisions, effective status, and bounded search descriptors, but not Guide bodies. A body is reread only for an explicit read/use case through the retained root capability, exact relative path, and expected revision, then reparsed and rebound before use. Desktop and MCP callers receive no path authority.

Search normalization and Markdown field extraction still happen once for each changed revision. The resulting title, alias, command, path, error, Tag, summary, heading, and body fields are encoded into a versioned length-framed record in a private anonymous temporary file. Process memory retains only immutable record offsets/lengths and small ranking metadata. Search reads records through positional I/O into one reusable bounded buffer, decodes one Guide at a time, applies the existing exact product-owned ranking, and rereads only the selected records for excerpts. The file is disposable derived state, never canonical content, never stored inside a Workspace, and disappears when its final descriptor is dropped.

A successful refresh reuses the exact record and document descriptor for an unchanged revision. Changed records go to a new anonymous backing; failed inspection drops the candidate backing and retains the prior view/index. Each encoded record has a hard 64 MiB limit, each repeated-field collection has a 65,536-value limit, and physically retained prior backings plus the candidate backing have a hard 4 GiB aggregate limit checked before every write. A one-over result is `ResourceLimit`; the anonymous candidate is dropped without displacing the prior index. After eight referenced generations, refresh compacts all live records into one new backing only when the prior side is at most 2 GiB, ensuring prior plus replacement can never bypass the 4 GiB transient bound. Above that threshold, incremental backings remain allowed only within the same hard aggregate cap. Compaction copies only already validated, length-framed derived records; it grants no Workspace write authority.

The record codec rejects bad magic, invalid UTF-8, truncation, trailing bytes, length/count overflow, impossible or over-limit collection counts before allocation, allocation refusal, over-limit records, and positional offset/length failure. A search-side failure returns `IndexUnavailable`; refresh is the recovery path. `tempfile 3.23.0` was already locked as a test dependency and is promoted with default features disabled; no new transitive package is introduced.

## Consequences

- Retained native memory is proportional to Guide metadata plus the largest record being scanned, rather than total Markdown bytes.
- Exact ranking, all-term coverage, status filtering, excerpts, and malformed-Guide isolation are unchanged.
- The production 5,000-Guide/52.7 MiB benchmark remains within the existing 8 s cold, 50 ms warm-p95, and 300 MiB RSS budgets; results are recorded in the Phase 4B evidence.
- Search remains a sequential exact scan of disposable normalized records. The measured v1 corpus is responsive; the 2 GiB limit is a safe scan/storage envelope, not a claim that a worst-case 2 GiB query meets the 50 ms reference-corpus latency budget.
- A future inverted index may replace the record reader if measured scale requires it, without changing canonical Workspace content or the public ranking contract.
