# ADR 0028: Capability-gated MCP Guide update

- Status: Accepted for Phase 6C1 implementation
- Date: 2026-09-20

## Context

Phase 6B is useful for retrieval but cannot yet perform the target “Add this to Rehow” workflow. Mutation must not turn possession of a read-only launch handle into write authority, bypass revisions, assert `tested`, or expose generic filesystem operations. Broad/destructive approval and create-operation Workspace snapshots remain separate decisions.

## Decision

Phase 6C1 adds one mutation tool, `guide_update`, and no other write operation. A Workspace is writable through MCP only when its alias is named by a separate startup `--allow-write <alias>` argument. Unknown, repeated, or malformed capability aliases fail startup; read remains the default. A writable grant also requires a writable compatible Workspace and an available private XDG recovery-state base. At startup, Rehow opens and retains that directory, addresses it through the retained descriptor, and rejects state or runtime directories that contain, equal, or are contained by any configured Workspace. It does not imply broad, destructive, tested-approval, Attachment, or Git-write authority.

`guide_update` requires the launch handle, stable Guide ID, exact current revision, and a complete replacement set for title, summary, ordered Tags, ordered aliases, and UTF-8 Markdown body. The body is capped at 768 KiB so a call remains below the accepted 1 MiB transport-frame boundary after JSON/schema overhead. Rust applies all five typed mutations to one candidate and commits at most one existing journaled atomic replacement. Unknown metadata and fields outside this editable set remain preserved. A no-op preserves the revision. A conflicting revision writes nothing.

The client cannot provide status, verification timestamps/hashes, IDs, paths, operation IDs, or timestamps. Draft/stale state is retained; any verification-relevant change to a tested declaration follows the existing core rule and becomes effectively stale. MCP cannot set `tested`.

Valid domain failures are MCP tool errors containing the shared versioned `ErrorDto`; malformed JSON/schema/ID/revision inputs remain protocol invalid-parameter errors. If canonical bytes commit but the retained view cannot refresh, the success payload returns the committed revision with `refreshRequired: true` and omits status rather than claiming rollback or guessing current state.

## Consequences

- An explicitly authorized AI client can safely revise an existing Guide without receiving ambient path or status authority.
- The complete-field request is deliberate: it avoids ambiguous patch/null semantics and makes the effect reviewable.
- Category/Guide creation, partial patches, Category moves, lifecycle changes, Attachment metadata, broad/destructive operations, tested approval, and Git tools remain absent.
- A later creation gate must add a core-owned, writer-lock-checked Workspace snapshot rather than reusing a stale adapter view.
