# Design Direction

Status: Reference Desk approved for implementation
Last reviewed with web-design-guidelines: 2026-09-18 (Phase 2A implemented shell; closure pass)

## Experience principles

- Search is ready immediately and never feels like navigation away from the library.
- The interface resembles a serious local reference tool, not a SaaS dashboard or command palette stretched into an application.
- Stable panes, dividers, typography, and information density do more work than cards or decoration.
- Long commands, paths, errors, warnings, and recovery choices remain legible.
- Keyboard and pointer interaction are equally complete; assistive technology gets semantic structure and timely status.
- Workspace truth and conflict state are visible without creating alarm fatigue.

## Directions considered

### A. Reference Desk — selected

A compact three-region desktop workspace:

- Workspace rail: All Guides, Categories, Tags, recent/archived scopes.
- Results pane: persistent search, filters, ranked Guide rows, index status.
- Reader/editor: title, trust metadata, section outline, body, copyable commands, and contextual actions.

Above 1100 px all regions remain visible. At medium widths the Workspace rail becomes a drawer/toggle while results and reader stay split. Below 760 px, results and reader become a one-pane navigation stack that preserves query, selection, and scroll position.

Why it fits: users can search, compare results, and read without losing context; it supports dense libraries and native desktop spatial memory. Risk: the reader can become cramped, so it must never be squeezed below roughly 42 characters and must collapse panes deliberately.

### B. Search Lens

A restrained launch/search surface becomes a full-screen result list; opening a Guide transitions to a dedicated reader. A global search overlay is available everywhere.

Strength: fastest narrow-window behavior and minimal chrome. Rejected as the base because it hides browsing/Workspace orientation, complicates focus restoration, and turns repeated search/read transitions into navigation churn. Its overlay interaction is retained as a later quick-search mode, not the application shell.

### C. Manual Index

Hierarchical Category, Guide, matched-section, and reader columns mirror a desktop reference browser.

Strength: hierarchy and filesystem character are obvious. Rejected because four columns degrade command readability and narrow layouts, while global search becomes visually secondary.

## Layout specification

Wide (`≥1100 px`):

```text
┌─ 216 Workspace ─┬── 344 Search & Results ──┬──────── Guide Reader / Editor ────────┐
│ Categories      │ Search…                  │ Guide title              status      │
│ Tags            │ result                   │ summary / applicability              │
│ Archive         │ result                   │ warning / prerequisites              │
│                 │ result                   │ steps + code                         │
├─────────────────┴──────────────────────────┴───────────────────────────────────────┤
│ Workspace path · index state · Git state · actionable conflict/health message     │
└────────────────────────────────────────────────────────────────────────────────────┘
```

- Application toolbar: 44 px; result rows: 40–48 px.
- Workspace rail: 216 px default; results: 344 px; reader consumes remaining width.
- Use borders and surface changes, not a card around each region.
- Reader prose measure: 68–78 characters; code/table containers may use full width.
- Sticky controls must not cover focused content; headings have scroll margin.

Medium (`760–1099 px`): rail collapses to an accessible drawer; results width is user-adjustable within safe limits. Narrow (`<760 px`): Search/Results → Reader → Editor navigation with a visible Back to Results action and state restoration. No horizontal app scroll; code/table scroll stays local and labeled.

## Visual system

### Typography

- UI: `system-ui, "Inter", "Noto Sans", sans-serif`; do not require a network font.
- Code: `ui-monospace, "JetBrains Mono", "DejaVu Sans Mono", monospace`.
- Metadata 12/16; controls/results 13/18; body 15/23.
- Guide title 22/28; section headings 18/24 and 15/21; semibold, balanced wrapping.
- Use tabular numerals for counts, progress, and comparable timestamps.

### Color

Light:

| Token | Value |
| --- | --- |
| app | `#F5F6F4` |
| sidebar | `#ECEFEB` |
| reader | `#FFFFFF` |
| text | `#202421` |
| muted | `#626963` |
| divider | `#D5D9D5` |
| accent | `#28634E` |
| selection | `#DCEBE4` |
| focus | `#197052` |
| error | `#A52A22` |
| warning/conflict | `#805A00` |

Dark:

| Token | Value |
| --- | --- |
| app | `#191C1A` |
| sidebar | `#202421` |
| reader | `#242825` |
| text | `#EDF1ED` |
| muted | `#ABB3AC` |
| divider | `#3B413C` |
| accent/focus | `#79C8A7` |
| selection | `#29483B` |
| error | `#FF948A` |
| warning/conflict | `#EDB95F` |

Use system theme and `color-scheme`. State always includes icon/text/structure, never color alone. Verify final token pair contrast rather than assuming these planning values pass in every composition.

### Geometry and motion

- 4 px spacing grid: 4, 8, 12, 16, 24, 32.
- Radius: controls 4 px, menus/dialogs 6 px, code 3–4 px. Pills only for genuinely compact status when text is unavoidable.
- Focus: 2 px ring with 2 px offset, visible on every interactive surface.
- Optional transitions: 90–140 ms, opacity or 2–4 px transform only; direct selection is immediate.
- No animated layout reflow, panel swooping, pulsing skeletons, decorative motion, `transition: all`, gradients, glow, or glass.
- Disable optional transitions under `prefers-reduced-motion`; interruption never waits for animation.

## Interaction model

- `Ctrl+K` focuses/summons search; `/` focuses search when not editing text.
- Arrow keys move results, `Enter` opens, `Esc` returns focus, and focus restoration is deterministic after dialogs/drawers.
- All actions use semantic buttons/links. Icon-only actions have an accessible name and visible tooltip.
- Copy shows a polite, non-disruptive status. It never changes status to tested and never executes content.
- Destructive actions use a precise confirmation or dependable undo; broad changes preview targets.
- Navigation warns on unsaved edits. External changes show Reload, Compare, and safe draft-preservation options.
- Lists over 50 visible items are virtualized; search work does not run expensively on the UI thread per keystroke.
- Desktop state (query, filters, Guide, pane layout, reader position) is restorable/deep-linkable in an internal route/state model.

## Required states

- **No Workspace:** normal app chrome, local-storage explanation, Choose Workspace/Create/Clone actions, recent valid roots.
- **Empty Workspace:** exact Workspace path, “Create Guide,” import option, and change Workspace.
- **No results:** query remains visible; show active scope and Clear Filters/Search Archived actions.
- **Indexing:** already indexed results stay usable; determinate counts when known in `aria-live="polite"`; no full-screen spinner for local work.
- **Malformed Guide:** row/reader identifies safe relative file and offers validation/open-underlying-file/copy-details actions.
- **Workspace missing/non-writable/newer:** distinguish moved, unmounted, permission, and version cases with safe read-only/retry paths.
- **External change/conflict:** reader-top persistent banner with timestamps and Reload/Compare/Keep Draft semantics; never overwrite silently.
- **Git unavailable/auth/diverged/conflicted:** content remains fully usable; status strip explains the next safe action.
- **Very long content:** long title/path/error/command, deep outline, tables, 200% text scaling, and translation expansion remain operable.

## Web Interface Guidelines gate

Before every formal code review, fetch the current rules named by `.codex/skills/web-design-guidelines/SKILL.md`. At minimum verify:

- semantic landmarks/headings/controls and a skip link;
- labels/autocomplete/names and inline actionable form errors;
- visible `:focus-visible`, no hidden keyboard-only gaps, and focus not covered by sticky UI;
- accessible names on icon controls and `aria-hidden` decorative icons;
- polite live announcements for index/copy/save/validation changes;
- reduced motion; transforms/opacity only; no `transition: all`;
- long-content truncation/wrapping, `min-w-0`, deliberate empty states, and list virtualization;
- destructive confirmation/undo, state restoration, and unsaved-change protection;
- locale-aware dates/counts, true ellipses (`…`), active voice, specific action labels, and actionable errors;
- explicit image dimensions/lazy loading if images appear; safe managed assets only;
- system dark `color-scheme`, theme color, touch behavior, zoom, and 200% scaling.

Phase 2A review result: the implemented fixture-backed Reference Desk shell passed the current upstream rules after correcting accessible search naming, narrow focus restoration, responsive Workspace navigation, Arrow/Escape behavior, result announcements, light-theme contrast, keyboard-accessible command text, and visual evidence coverage. Real Workspace states and editor flows still require their own review before Phase 2 acceptance.

## Screenshot and manual review matrix

Capture 1440×900, 1024×768, 720×900, and 480×800 in light/dark themes for:

- first launch, empty Workspace, populated library;
- 1/many/no search results and very long query;
- Guide with long title/path, warnings, tables, deep headings, and 200-character command;
- keyboard focus in navigation/search/results/copy/links;
- indexing start/partial/complete;
- missing/non-writable Workspace, corrupt Guide, stale index, and external conflict;
- pane collapse with query/selection/scroll preserved;
- 200% scaling, long translated strings, and reduced motion.

Also run screen-reader landmarks/headings/names/live-region/focus-restoration checks, automated contrast/accessibility checks, and inspect that horizontal scrolling is confined to code/table regions.
