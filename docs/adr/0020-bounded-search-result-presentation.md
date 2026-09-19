# ADR 0020: Bounded search result presentation

- Status: Accepted
- Date: 2026-09-19
- Extends: ADRs 0004 and 0018

## Context

Phase 4 ranking returned only ordered Guide IDs and matched-field reasons. The product requires useful excerpts/highlights, archived search by explicit scope, a 50 ms warm backend p95, a 100 ms visible-update p95, and no main-thread task over 50 ms. ADR 0004 requires plain excerpts plus spans rather than HTML. Adding the complete presentation payload reproducibly moved the 5,000-Guide/50 MiB release benchmark p95 from the Phase 4B 5.382 ms baseline to 7.668 ms. This exceeds ADR 0018's 10% investigation trigger but remains 6.5 times inside the product budget; a one-result comparison also measured 7.664 ms, showing that returning 100 bounded excerpts is not the scaling cause.

## Decision

Keep the dependency-free precomputed index. Core returns at most 100 ordered hits containing Guide ID, matched fields, one plain normalized excerpt of at most 220 Unicode scalar values plus at most two ellipses, one excerpt-field reason, and at most 32 sorted non-overlapping highlight ranges. Ranges use Unicode-scalar offsets, not bytes or UTF-16 units. Core selects the highest-priority matched field, then bounds and highlights its value only after ranking/truncation.

The Tauri adapter serializes only those fields. The renderer strictly validates shape, field names, excerpt/range/count limits, and renders text nodes with semantic `<mark>` elements; it never accepts or renders result HTML. Status scopes are Active by default, Tested, Draft, Stale, Archived, and All. The adapter passes the closed scope value to core, which filters status before ranking and the 100-result cap. Query and non-default scope are restorable URL state.

Use a 20 ms debounce, leaving measured headroom for the backend and rendering budgets. Retain `content-visibility` for lists over 50 results. A production Chromium run over 40 query changes must remain below 100 ms visible p95, and the deterministic 5,000-Guide scope plus 100-excerpt transformation must remain below one 50 ms main-thread budget.

## Consequences

- Search results explain why they matched without creating an HTML or Markdown rendering boundary.
- Unicode offsets have one explicit interpretation across Rust and JavaScript.
- The investigated backend regression is accepted because required presentation is now included in the measured production contract and absolute headroom remains large. Future comparisons use this Phase 4E baseline; ADR 0018's absolute and 10% regression triggers still apply.
- Original casing is retained in Guide titles/summaries but index excerpts are normalized. A future original-case excerpt mapper needs its own bounded correspondence proof.

## Rejected alternatives

- HTML snippets: unnecessary injection boundary.
- UTF-8 byte or JavaScript UTF-16 offsets: ambiguous across the IPC boundary.
- Client-side body searching: duplicates ranking policy and requires broad content IPC.
- A new inverted-index dependency solely to recover about 2.3 ms while p95 remains under 8 ms.
- Returning excerpts for every candidate before truncation: avoidable unbounded work.
