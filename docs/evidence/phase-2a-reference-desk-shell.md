# Phase 2A Reference Desk shell evidence

Date: 2026-09-18 (Africa/Algiers)

Status: implementation checkpoint passed its combined Design Reviewer closure. This is a fixture-backed application shell, not acceptance of Workspace integration or the complete Phase 2 workflow.

The inert scaffold now renders the approved Reference Desk direction with stable navigation, search/results, Guide reader, status strip, system light/dark tokens, and deliberate wide/medium/narrow behavior. Search, selection, narrow reader navigation, keyboard search focus, empty results, command copy, copy failure, and live announcements are implemented without adding dependencies or native authority.

Verification:

- current Web Interface Guidelines were fetched from the project-pinned skill source before review;
- semantic header/nav/aside/main/footer structure, hierarchical headings, labeled search, skip link, native controls, visible focus, accessible icon buttons, polite copy status, locale-formatted dates, reduced-motion handling, text overflow, and local command scrolling are present;
- the first independent Design Reviewer gate found inaccessible search naming, lost narrow focus, unreachable responsive navigation, missing result-key navigation/live status, weak light metadata contrast, and inaccessible horizontal command scrolling; all were corrected in source;
- Biome, TypeScript, 6 Vitest checks, and the Vite production build pass. Tests cover filtering, result announcements, wrapped result navigation, and server-rendered shell semantics; native focus/copy paths remain manual interaction checks;
- Chromium captures now cover dark and light themes, 1440×900, 1024×768 with the navigation drawer, 720×900 with visible search focus, 720×900 empty results, 480×800 narrow reader/back state, and a 2× scale/effective 720×450 reflow view;
- screenshots are stored under `docs/evidence/screenshots/reference-desk-*.png`.
- the final independent closure reports no remaining blocker, high, medium, or low finding.

Deferred intentionally: real Workspace data and actions, editor/save flows, durable internal-route restoration, list virtualization above 50 visible rows, first-launch/workspace/error/conflict variants, and a manual screen-reader pass. These remain required before Phase 2 acceptance.
