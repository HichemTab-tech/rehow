# ADR 0019: Descriptor-bound external-change watching

- Status: Accepted
- Date: 2026-09-19

## Context

Rehow must notice external Workspace edits without treating lossy filesystem events, caller paths, timestamps, or event payloads as content authority. The retained Workspace is already rooted in a no-follow directory capability, and a complete descriptor-confined refresh is the only accepted way to replace its view and disposable search index.

The architecture names `notify`; its stable `8.2.0` release supports Linux inotify, declares Rust 1.77 (with resolved `notify-types 2.1.0` declaring Rust 1.85), is CC0-1.0 licensed, and has no required serialization or macOS backend feature for Rehow. Upstream documents editor-dependent event sequences, inotify watch limits, network-filesystem gaps, and possible event loss for large trees. Those limitations prohibit precise-event semantics.

## Decision

Pin `notify = 8.2.0` in `rehow-core` with default features disabled. The resolved Linux-only new graph is `notify-types 2.1.0`, `inotify 0.11.5`, and `inotify-sys 0.1.8`; `mio` and `walkdir` were already resolved transitively. Core constructs only non-recursive watches for the already opened root and managed directories. Each watch path is `/proc/self/fd/<retained-directory-fd>` backed by a descriptor opened with no-follow rules; the watcher never recursively traverses a caller path or exposes an event path.

All non-access events are undifferentiated invalidation hints. A bounded single-slot wake channel and atomic signal bits coalesce bursts for 250 ms. Watch errors become a `degraded` signal. Neither signal identifies a filename or authorizes a read/write.

The desktop adapter emits only the Workspace ID, a native per-open watch-session generation, and `changed`/`degraded` kind. The renderer rejects stale ID/session pairs and may request the existing argument-free canonical refresh only for the currently open Workspace. Dirty forms defer automatic refresh without a confirmation dialog or data loss. Clean sessions refresh automatically, and a window refocus also requests reconciliation so common missed-background-event cases recover. Manual refresh remains permanently available.

After every successful canonical refresh, core rebuilds the exact managed-directory watch set. Failure to rebuild degrades watching but does not invalidate the successfully refreshed content. A failed canonical refresh retains the prior view/index and reports an actionable message.

## Consequences

- Events cannot bypass path confinement, revision comparison, parsing, limits, Attachment validation, or atomic view/index replacement.
- Git internals, recovery state outside the Workspace, arbitrary symlink targets, and event filenames never cross the native boundary.
- New managed directories are noticed through their watched parent and become watched after reconciliation.
- Network/removable filesystems and kernel watch exhaustion can degrade automatic detection. Rehow says so and preserves refocus/manual reconciliation instead of claiming perfect delivery.
- The current implementation remains Linux-only, matching the existing storage boundary. A future cross-platform storage port must replace the descriptor-path construction without weakening this ADR.

## Rejected alternatives

- Trusting event paths or event kinds: editor save patterns and overflow make them non-authoritative.
- Recursive watching of the selected absolute path: it broadens traversal to unrelated/hostile entries and races path replacement.
- Renderer-selected paths: violates the native capability boundary.
- Frequent whole-Workspace polling: reliable but imposes unnecessary repeated 5,000-Guide scans when inotify works.
- Direct custom inotify parsing: avoids a dependency but duplicates mature backend/error handling and increases unsafe platform code.
