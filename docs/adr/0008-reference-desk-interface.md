# ADR 0008: Reference Desk Interface

- Status: Accepted
- Date: 2026-09-05

## Context

Rehow must make search and long technical reading fast without looking like a generic dashboard. Search Lens maximizes narrow simplicity but loses spatial continuity; Manual Index exposes hierarchy but compresses commands and makes search secondary.

## Decision

Adopt the Reference Desk: Workspace rail, Search/Results pane, and Guide reader/editor on wide windows; collapse the rail at medium widths and use a state-preserving single-pane Results → Reader flow below 760 px. Use restrained neutral/green surfaces, deliberate typography, 4 px spacing grid, thin dividers, small radii, visible focus, and minimal reduced-motion-compatible transitions.

Apply the project-pinned web-design guideline skill during planning, shell, interaction, visual, and completion gates. Capture the state/size/theme matrix in `docs/design-direction.md` and record file:line findings.

## Consequences

- Excellent search/read continuity and desktop information density.
- Requires careful pane collapse, focus restoration, virtualization, and reader minimum measure.
- Global Search Lens may be added later as a shortcut, not as base navigation.

## Rejected

- Card-centric dashboard, marketing hero, decorative gradient/glow/glass, four-column Manual Index as base, and full-app modal search as the only navigation.
