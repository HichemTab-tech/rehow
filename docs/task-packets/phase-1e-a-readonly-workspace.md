# Task Packet: Read-only Workspace vertical slice

- **Objective:** Open a user-selected folder through a native dialog and show real schema-v1 Categories and Guides without granting path or write authority to the renderer.
- **Required reasoning level:** high; xhigh security review at the completed checkpoint.
- **Prerequisites:** Accepted ADRs 0002, 0003, 0009, 0011–0014; accepted parser/binders; Phase 2A shell.
- **Allowed files or subsystem:** `crates/rehow-core` storage/domain/protocol visibility, desktop Tauri adapter and UI, dependency lockfiles, this packet, content-model vector, and recovery ledger.
- **Relevant specifications and ADRs:** `docs/content-model.md` scan/revision rules, `docs/architecture.md` capability flow, `docs/threat-model.md`, ADR 0002 and ADR 0003.
- **Exact required behavior:** Native Rust opens the folder picker with no path input; the core canonicalizes and retains the root descriptor, enumerates with descriptor-relative `openat`, refuses symlinks/special files/hardlinks, enforces documented file/count/aggregate caps, parses current manifests/Categories/Guides, classifies empty/newer/Markdown/unrelated/malformed folders, reports orthogonal effective writability plus safe root/ancestor Git-marker facts, isolates invalid children, computes exact BLAKE3 revisions, and maps compatible metadata to a read-only UI session. Guide bodies are requested individually by validated Guide ID rather than included in the broad list response.
- **Important states and edge cases:** Cancellation; absent/non-directory/unreadable root; empty/unrelated/nested Markdown; malformed/newer manifest; missing/unsafe Categories; malformed/oversize/mis-categorized/duplicate Guides; hostile UTF-8 names; disappearing entries; several thousand Guides.
- **Security constraints:** No renderer path parameter or returned absolute path; retain descriptors; fixed-depth component opens with `O_NOFOLLOW`; regular single-link files only; bounded reads/enumeration; content-free errors; no write, shell, Git, link opening, Markdown HTML rendering, logging, or MCP authority.
- **Tests to add or update:** Temporary-folder classification, effective writability, root Git directory/worktree file, ancestor Git relationship, unsafe Git marker, compatible load, fixed revision vector/path sensitivity, newer/malformed manifests, nested Markdown, symlink/hardlink denial, Category mismatch, desktop DTO shape, UI empty/open/Git states, existing full regression suites.
- **Commands to run:** `cargo fmt --all --check`; Workspace Clippy/tests; `pnpm check`; `pnpm test`; `pnpm build`; native Tauri no-bundle build; bootstrap and diff checks.
- **Required reviews and artifacts:** One Security Reviewer/xhigh review of confinement, races, dependencies, IPC, disclosure, and adversarial coverage. No additional routine agents.
- **Explicit non-goals:** Workspace creation, import/adoption, writes/recovery journal, watchers, rendered Markdown, Git, editing, recent-Workspace persistence, MCP, or complete onboarding.
- **Definition of done:** Real compatible content replaces fixtures; no arbitrary-path IPC exists; caps and hostile filesystem cases are tested; required gates pass; docs and `PROGRESS.md` state limitations truthfully.
- **Expected commit boundary:** Core read-only scanner, native picker/session adapter, real-data UI, tests, dependency pins, and aligned documentation in one reviewable checkpoint; Orchestrator commits.
- **Conditions requiring escalation:** Confinement cannot be proven, native dialog requires path-bearing IPC, dependency introduces execution/credential behavior, mutation becomes necessary, or required gates fail outside this scope.

## Dependency decision

- `blake3 1.8.7` is required by the accepted revision format; pure Rust/std features avoid native SIMD/C build paths. License: CC0-1.0 OR Apache-2.0.
- `libc 0.2.189` was already locked transitively and exposes the Linux descriptor syscalls without a broader filesystem abstraction. License: MIT OR Apache-2.0.
- `rfd 0.17.2` keeps folder selection inside native Rust and supports Wayland/XDG portals. The JavaScript Tauri dialog path was rejected because it would disclose a path to the renderer and tempt path-bearing IPC. License: MIT.
- `@tauri-apps/api 2.11.1` supplies the typed invoke transport only; the renderer can invoke the named no-argument command but cannot choose a path. License: MIT OR Apache-2.0.
