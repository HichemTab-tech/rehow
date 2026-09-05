# Rehow Product Specification

Status: bootstrap baseline
Owner: Architect
Last updated: 2026-09-05

## Product statement

Rehow is a local-first desktop knowledge application for preserving and retrieving repeatable technical instructions. Its promise is: **Remember how you did it.** A user launches Rehow, searches immediately, opens a trustworthy Guide, copies inert command text, follows warnings and prerequisites, verifies the result, and has rollback/troubleshooting context when something fails.

## Confirmed requirements

### Product and platform

- Native Tauri v2 desktop application using React, TypeScript, Vite, Tailwind CSS, shadcn/ui, pnpm, and Vitest where applicable.
- Linux-first, with deliberate support for Omarchy, Arch Linux, Hyprland, and Wayland.
- No permanently running local web server for the desktop app.
- A separately usable MCP server is a required phase.
- Human-readable Markdown in a user-selected Workspace is the portable source of truth. Git compatibility is first-class and optional.
- Domain language is Workspace, Guide, Category, Tag, and Attachment.

### Core experience

1. First launch offers Create Workspace, Open Workspace, and Clone/Connect Git Workspace.
2. Returning launch restores the last valid Workspace or explains why it is unavailable.
3. Keyboard focus begins in search on normal launch.
4. Incremental results cover metadata, commands, paths, errors, and full content with deliberate ranking.
5. A Guide reader exposes prerequisites, warnings, steps, verification, rollback, troubleshooting, related Guides, references, Attachments, and one-action command copying.
6. Editing is revision-protected; external changes never overwrite unsaved work silently.
7. Content remains usable without Git, MCP, a network connection, or any AI service.

### Workspace inspection states

Inspection returns exactly one primary classification plus orthogonal Git/writability facts:

- nonexistent folder;
- empty folder;
- compatible current Workspace;
- older supported Workspace;
- newer unsupported Workspace;
- ordinary Markdown collection;
- unrelated nonempty folder;
- malformed Workspace;
- non-writable folder (orthogonal, because it may still be opened read-only);
- existing Git repository and nested Git repository (orthogonal).

No classification authorizes a write. Initialization, migration, adoption/import, and Git initialization are separate previewable actions.

### Content operations

- Categories: list, create, edit/rename, reorder, archive, restore, and safely remove when empty.
- Guides: create, edit, rename, move, change Category, manage Tags and aliases, archive, restore, and recoverably delete.
- Statuses: `draft`, `tested`, `stale`, `archived`.
- `tested` requires native user approval bound to the exact current Guide revision and verification hash; no MCP payload or generated content can prove that approval.
- Attachments are copied into managed storage after a collision/size/type preview; symlinks are rejected.
- Validation identifies the exact file/field and gives a recovery action.
- Unknown valid frontmatter keys survive read-modify-write cycles when they do not conflict with reserved keys.

### Search

- Keyboard-first incremental search across title, summary, aliases, Tags, Category, commands, paths, error messages, and full content.
- Ranking priorities are specified, versioned, and covered by relevance fixtures.
- Exact normalized title and alias matches outrank other fields; exact command/path/error tokens have explicit boosts.
- Index updates follow internal and external file changes. Malformed Guides produce diagnostic results without blocking healthy Guides.
- The index is disposable and rebuilt from canonical files.
- Production budgets at 5,000 Guides / 50 MiB Markdown on reference hardware:
  - warm query p95 ≤ 50 ms from keystroke to ranked backend response;
  - visible update p95 ≤ 100 ms including rendering;
  - single-Guide incremental reindex p95 ≤ 150 ms;
  - cold full rebuild ≤ 8 s and peak RSS ≤ 300 MiB;
  - typing remains responsive with no main-thread task over 50 ms.

Budgets must be measured in a production build; reference hardware and corpus generator are recorded with results.

### Git

- Detect executable availability, repository location/state, branch, working tree, remotes, divergence, conflicts, and authentication errors.
- Support explicit init, remote connection, clone, fetch/pull-style safe synchronization, commit, and push.
- Optional sync-on-start and scheduled sync are opt-in, observable, and disabled by default.
- Use the user's SSH agent/credential helper only through explicitly trusted system/user configuration isolated from repository-controlled config. Never request, store, copy, display, or log tokens/private keys.
- No force push, hard reset, change discard, automatic conflict resolution, history rewriting, or automatic hook installation.

### MCP

- A separately executable local server supports the Workspace, Category, Guide, Attachment-metadata, search/validation, and safe Git operations in `docs/mcp-spec.md`.
- It uses strict schemas, stable structured errors, configured Workspace confinement, no arbitrary paths/shell, revision protection, atomic writes, unknown-metadata preservation, dry-run/confirmation for broad or destructive mutations, and adversarial tests.

## Architectural decisions

The authoritative decisions are the accepted ADRs. At baseline:

- A monorepo separates UI adapters, a reusable privileged core, MCP transport, and generated contracts.
- A versioned root manifest, Category descriptors, Markdown Guide files, managed Attachments, recoverable trash, and portable backups make up a Workspace. Disposable indexes and runtime recovery state live in XDG cache/state outside it.
- Stable UUIDv7 identifiers are independent of titles and filenames.
- Markdown body is CommonMark + GFM; raw HTML is not rendered.
- Writers use capability-confined paths, atomic replacement, and content-derived revision tokens.
- Git commands use fixed argument vectors without a shell and with hooks/pagers/editors disabled.
- Search is a rebuildable field-aware index; the exact engine may change behind the core API without changing content.

See `docs/architecture.md` and `docs/adr/`.

## Assumptions

- One human usually edits a Workspace at a time, but external editors and MCP/desktop concurrency are normal and must be detected.
- Initial Workspaces contain fewer than 1,000 Guides; 5,000 Guides is the supported performance target.
- UTF-8 is required for managed YAML/Markdown; binary Attachments are allowed.
- English ships first, but UI construction must not preclude localization.
- Linux packages are the initial release target; other desktop platforms are future compatibility work, not an excuse for Linux-specific unsafe shortcuts.
- A normal decorated, resizable single window is acceptable; no global-coordinate overlay behavior is promised under Wayland.

## Open questions

These do not block Phase 1 and should be resolved at their named gates:

- Exact distribution matrix (`deb`, `rpm`, AppImage) after oldest-distro/Wayland validation — Phase 7.
- Whether optional scheduled sync ships in v1 or remains deferred after Git usability testing — Phase 5.
- Whether in-place Markdown adoption is safe enough to ship in v1, versus copy-only import — Phase 2 security gate.
- Whether remote Streamable HTTP MCP transport is useful; v1 requires local stdio only — post-v1 unless requested.
- Default maximum Attachment size and optional MIME allowlist — Phase 3, before implementation.

## Deferred features

- Cross-device automation beyond user-initiated Git.
- Rich WYSIWYG editing; initial editing is structured metadata plus Markdown source/preview.
- Saved searches, Favorites, and custom status taxonomies.
- Remote MCP transport, multi-user locking, encrypted Workspaces, and content signing.
- Windows/macOS release certification.
- Automatic update delivery until signing and rollback are designed.

## Non-goals

Rehow is not a hosted SaaS, collaboration platform, password/secret manager, general command runner, cloud drive, Git host, embedded AI chat, mobile app, office suite, full code editor, or marketing website.

## Acceptance criteria

### First usable slice

- A production Tauri window starts on Linux without a web server.
- A temporary compatible Workspace can be initialized, reopened, validated, and left entirely human-readable.
- At least one Category and Guide can round-trip without losing unknown metadata.
- Stale writes are rejected and interrupted-write recovery is tested.
- Unit/type/lint/build/Rust gates pass for the slice.

### Product completion

- A new user can install Rehow, create/open/inspect/import a Workspace, organize and edit Guides, retrieve them within the budgets, use Git safely, configure the local MCP server, restart without data loss, and recover through documented malformed/conflict/interruption states.
- Required unit, integration, accessibility, visual, production-build, packaging, and security gates pass.
- Documentation matches observed behavior; a verified checkpoint commit and exact restart action are recorded in `PROGRESS.md`.
