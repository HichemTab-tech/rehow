# ADR 0056: Actionable Workspace issue reporting

- Status: Accepted
- Date: 2026-09-23

## Context

The confined Workspace scanner already isolates malformed, unsafe, mismatched, duplicate, and interrupted-create residue, but the desktop exposes only an issue count. Users therefore cannot identify the affected relative location or follow a safe recovery step.

## Decision

The native Workspace payload includes at most the first 100 scanner-owned issue records as a closed kind plus inert relative location, along with the complete issue count and an exact truncation flag. The renderer validates the closed schema, bounds locations to 2,048 bytes, rejects display-control characters, and renders them only as escaped text.

The Reference Desk footer opens a native-feeling modal list with kind-specific explanation and recovery guidance. It never offers automatic deletion, repair, path navigation, or arbitrary filesystem access. Ambiguous content stays unchanged; users are told to back it up, correct or restore it outside Rehow, and refresh. The missing-Category-descriptor message explicitly covers interrupted Category creation. Large result sets remain bounded and disclose truncation.

## Consequences

- Workspace validation becomes actionable without expanding filesystem authority.
- Recovery remains conservative where intent cannot be proven.
- The panel may show only 100 issues at once; after those are corrected, refresh reveals the next bounded set.
- A future automatic recovery action requires a separate operation-bound plan, exact effect proof, and destructive-operation review.
