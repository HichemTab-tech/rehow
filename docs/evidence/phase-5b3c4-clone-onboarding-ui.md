# Phase 5B3C4 clone onboarding UI evidence

Date: 2026-09-19

## Boundary

The empty-folder onboarding state now offers either new Workspace creation or anonymous-HTTPS clone. Clone URL entry is bounded and rejects credentials, query text, fragments, noncanonical spelling, and non-HTTPS transports before IPC. The renderer sends only the URL to the existing native plan command; every returned plan, apply, cancellation, and completion payload is checked as a closed path-free shape before use.

Validation remains a native-confirmed, cancellable operation. The UI states explicitly that the selected folder remains unchanged, exposes a named cancellation action, and ignores stale cancellation responses after validation resolves. A successful validation moves to a distinct no-write state. Completion requires the existing second native confirmation, cannot be cancelled after finalization starts, restores the validated UI after decline/error, and hands the bounded completed Workspace to the existing search-first shell on success.

SSH, credentials, authentication prompts, commit, pull, push, sync, MCP mutation, and percentage progress remain outside this slice.

## Design and accessibility

The project-pinned `web-design-guidelines` skill and its current upstream rules were read before review. The implementation uses a labeled URL input with `type="url"`, `inputMode`, disabled spellcheck, autocomplete metadata, bounded length, example placeholder with an ellipsis, inline actionable errors with focus, semantic forms/buttons/progress, polite live status, explicit busy text, deterministic focus restoration, wrapping actions, and long-host overflow protection. The submit action remains enabled until submission so invalid input receives an inline explanation.

The interaction stays inside the existing Reference Desk empty state and introduces no dashboard cards, modal imitation, gradient/glow/glass treatment, decorative animation, or renderer-side filesystem authority. An ephemeral Playwright/Tauri-command fixture captured and visually inspected the real React states without adding a dependency or production fixture:

- `screenshots/clone-entry-wide-light.png` — 1200×800 labeled URL entry with visible focus;
- `screenshots/clone-validating-wide-light.png` — 1200×800 progress, destination-safety copy, and cancellation;
- `screenshots/clone-ready-narrow-dark.png` — 480×800 validated state, concise header, and wrapped completion actions.

The initial review found 2 high state-reconciliation gaps and 2 medium evidence/bounds gaps. The UI now blocks conflicting setup/navigation in an explicit recovery state after ambiguous validation, reconciles ambiguous completion through the pathless native refresh before adopting it, and bounds every completed Workspace field. The same Design Reviewer passed the corrected diff with no remaining blocker/high/medium finding. The later consolidated Phase 5 visual gate still owns native dialogs and end-to-end native interaction.

## Verification

- `pnpm exec biome check apps/desktop/src/App.tsx apps/desktop/src/index.css apps/desktop/src/app.test.ts`: passed.
- `pnpm --filter @rehow/desktop typecheck`: passed.
- `pnpm --filter @rehow/desktop test`: 34 passed.
- `pnpm --filter @rehow/desktop build`: passed.
- `git diff --check`: passed.
- Independent Design Reviewer/high: PASS after correction; no remaining blocker/high/medium finding.
- Screenshot SHA-256: entry `f6593b4f40a6a92f3fc5599c392d6eb80902b99451e0785b3b4a009842262e10`; validating `83e2cb959a2dbdf2e5954131407f0f237edfed343e4e2d91b9928eed344d5038`; ready `c9077d3385f80feb965003f3c7b31cef1bd0d91c58d6b088625592ec1188cda5`.
