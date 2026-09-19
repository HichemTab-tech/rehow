# Phase 5D2D Reference Desk sync UI evidence

Date: 2026-09-19

## Implemented interaction

- A ready writable Workspace with a configured upstream exposes one compact **Sync** action in the existing status bar.
- Inspection opens a bounded Reference Desk panel, moves focus to cancellation, announces async changes, and supports Escape/click cancellation through the pathless native command.
- The renderer validates the exact seven-field inspection payload and closed cancel/apply/recovery results. URLs, refs, object IDs, paths, Git output, credentials, content, and staging authority are rejected.
- Up-to-date, local-ahead, dirty fast-forward, diverged, history-limit, and clean fast-forward states each have specific next-step copy. Only the clean proven fast-forward renders **Apply Fast-Forward**.
- Successful apply/recovery installs the already validated native Workspace payload, reconciles Guide selection, watcher health, search/status refresh, announcements, and focus. Decline/error/cancellation preserve explicit retry paths.
- Generic recovery first asks the pathless sync recovery command whether a sync intent exists; `recoveryNotRequired` alone falls through to the existing commit recovery flow. This avoids presenting the wrong native confirmation while keeping recovery details out of IPC.

## Guideline and visual review

The project-pinned `web-design-guidelines` skill and the current upstream rules fetched from `https://raw.githubusercontent.com/vercel-labs/web-interface-guidelines/main/command.md` were applied. Static review found no blocker/high/medium issue in the new surface: controls are semantic and specifically named; visible focus and deterministic restoration are present; async/error text uses existing polite/alert channels; headings are hierarchical; user-controlled labels are bounded and wrap; actions use Title Case and real ellipses; fixed-panel scrolling is contained; narrow actions wrap; and no new motion, image, navigation, form, or unsafe HTML behavior was introduced.

Representative captures were generated through a browser-only mocked Tauri boundary using the exact closed payload shapes, then visually inspected:

- `docs/evidence/screenshots/git-sync-wide-light.png` — 1200×800 light.
- `docs/evidence/screenshots/git-sync-narrow-dark.png` — 480×800 dark.

The 440px maximum panel remains above the status bar, becomes inset at 480px, preserves useful density, wraps long content, keeps actions visible, and retains accessible contrast in both system themes.

The single reused Design Reviewer found one medium issue in the first forced-dark capture: white primary-button text measured about 1.98:1 against the mint background. The forced-dark selector now mirrors the accepted system-dark `#102d21` foreground; the narrow screenshot was regenerated and visually rechecked. Final verdict: `PASS` with no remaining blocker/high/medium finding.

## Verification

- `pnpm --filter @rehow/desktop test`: 38 passed.
- `pnpm check`: passed (engine pins, Biome, TypeScript, contract drift).
- `pnpm build`: passed after the contrast correction (contracts, production Vite bundle, locked Cargo Workspace build).
- `git diff --check`: passed.

The exact signed checkpoint is recorded in `PROGRESS.md` after acceptance.
