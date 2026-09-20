# ADR 0037: Bounded MCP Guide validation and discovery

- Status: Accepted for Phase 6D1
- Date: 2026-09-20

## Decision

Expose three read-only, pathless MCP tools over the retained Workspace view: `guide_validate`, `guides_find_duplicates`, and `guides_find_related`.

Validation reports grouped Guide, Attachment, verification, and missing-related-Guide diagnostics without returning filesystem paths. A loaded Guide remains structurally valid; `healthy` distinguishes a valid Guide with warnings or errors.

Duplicate discovery is deliberately conservative: exact NFKC/lowercase title-title, title-alias, or alias-alias identity only. Related discovery ranks explicit forward/reverse relationships first, then shared aliases, Tags, meaningful title terms, and same-Category context. Results use authenticated view-bound pagination. Global duplicate generation is deterministically capped at 10,000 candidate pairs and 100,000 pair comparisons and reports truncation.

## Consequences

- Results are explainable through closed reason labels and stable scores.
- No fuzzy or embedding dependency, body disclosure, filesystem path, mutation, Git, shell, network, or ambient Workspace authority is added.
- More sophisticated similarity may replace this baseline only with measured ranking tests and a new decision.
