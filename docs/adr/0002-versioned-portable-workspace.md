# ADR 0002: Versioned Portable Workspace

- Status: Accepted
- Date: 2026-09-05

## Context

Workspace content must be human-readable, Git-compatible, safely migratable, and recoverable. Category ordering/lifecycle and stable cross-Guide relationships need structure without a canonical database or central Guide list merge hotspot.

## Decision

Schema v1 uses `rehow.yaml`, one-level Category directories/descriptors under `categories/`, direct-child Markdown Guide files, and managed files under `attachments/<guide-id>/`. `.rehow/` is reserved for recoverable trash and verified migration/bulk backups—not cache or runtime journals. Derived indexes live solely under XDG cache; locks, journals, and before-images live under XDG runtime/state. The manifest does not enumerate Guides.

Workspace/Category/Guide/Attachment identities are UUIDv7 and independent of display names/paths. Guide frontmatter contains structured searchable metadata; the body uses CommonMark/GFM reserved H2 sections. Exact details are in `docs/content-model.md`.

Unknown valid metadata values and untouched concrete syntax (comments/order/quoting/BOM/line endings/final newline) are preserved. Metadata edits that cannot prove that contract are refused without changing bytes. Reading never creates IDs or migrates content. Old supported schemas require preview+verified backup+explicit migration; newer schemas are read-only.

## Consequences

- Humans can inspect/edit/version content without Rehow.
- Rename/move causes readable Git renames but identity remains stable.
- Filesystem scan/index resolves IDs; duplicate IDs become explicit conflicts.
- Unknown formatting/comments may require a concrete-syntax YAML implementation; a simple serializer is insufficient for the final preservation contract.

## Rejected

- SQLite or search index as canonical store.
- Manifest list of every Guide.
- MDX/executable Markdown.
- Read-time auto-adoption/migration.
