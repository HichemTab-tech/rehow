# Phase 3 Guide Context Reader Evidence

Date: 2026-09-23

The selected-Guide read payload now carries canonical `created_at`, last-verification time, applicability, prerequisites, warnings, files affected, related Guide IDs, and References. These fields remain out of the Workspace list payload, so large-Workspace IPC and retained renderer state do not expand for unopened Guides. React renders every value as escaped text; files use inert code styling, missing relationships are explicit, and related Guides use native buttons. Reference actions reuse the revision/session-bound portal flow, extended to accept only URLs still present in canonical Reference metadata or the safe Markdown AST.

The current upstream Web Interface Guidelines were fetched and applied. Static review found no remaining issue: headings are hierarchical, warnings remain prominent without card nesting, controls are semantic and keyboard-visible, async Reference activity is announced by the existing live region, long content wraps, narrow layout reflows without horizontal overflow, and the prior inert navigation buttons were removed in favor of working Active/Archive filters.

Synthetic closed Tauri payloads were used only for visual capture; no personal Workspace content was read:

- `screenshots/guide-context-wide-light.png` — 1200×900 light, three-pane reader.
- `screenshots/guide-context-narrow-dark.png` — 480×900 dark, scrollable reader.

Both captures preserve Reference Desk hierarchy, density, contrast, and fixed status access. The narrow capture intentionally demonstrates the scroll continuation rather than shrinking text or hiding metadata.

Verification:

- 51 Vitest tests pass, including inert hostile context rendering.
- 493 core tests run: 491 pass and the 2 explicit network/performance tests remain ignored.
- 36 desktop Rust tests pass.
- `pnpm check`, strict core/desktop Clippy, Cargo formatting, contract drift, and the production Vite build pass on the existing Node 26.2.0/pnpm 11.20.0 toolchain.
