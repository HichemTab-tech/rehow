# Task Packet: Ordinary Markdown import preview

- **Objective:** Replace the inert ordinary-Markdown classification with a descriptor-retained, bounded, pathless import preview; add no destination write.
- **Required reasoning level:** high.
- **Prerequisites:** ADR 0050; existing descriptor-confined Workspace inspection and onboarding UI.
- **Allowed files or subsystem:** `crates/rehow-core/src/storage.rs`, `apps/desktop/src-tauri/src/lib.rs`, `apps/desktop/src/App.tsx`, their existing tests, generated contracts only if needed, ADR/evidence/progress documentation.
- **Relevant specifications and ADRs:** `docs/product-spec.md` Workspace inspection; `docs/content-model.md` Adoption and import; ADR 0011 limits; ADR 0050.
- **Exact required behavior:** `WorkspaceInspection::OrdinaryMarkdown` retains the selected source capability. A core preview recursively enumerates deterministic sorted entries through retained descriptors, never path joins; returns exact included/malformed-or-unsafe/oversized/non-Markdown counts, aggregate bytes, normalization count, and at most 100 inert display rows. Tauri retains the source only for this onboarding lifecycle and returns the bounded preview. React presents the counts/list plus a clearly disabled apply step until the destination phase exists.
- **Important states and edge cases:** cancellation; empty-after-rescan; source replacement/change; depth four boundary; entry/candidate/aggregate caps; `.md` case sensitivity; invalid UTF-8 filename/content; BOM/CRLF/bare-CR/trailing-newline normalization; empty/NUL/control content; symlink/hardlink/FIFO/socket/device; oversized file; long/hostile display path; existing Git facts remain orthogonal.
- **Security constraints:** no renderer path/bytes/descriptors; `O_NOFOLLOW`; single-link regular files only; bounded allocation/work; content-free errors; source remains read-only; preview labels are inert text; selection change revokes prior source.
- **Tests to add or update:** core deterministic preview and every edge case above; Tauri payload/capability lifecycle; React payload guard, preview state, accessibility labels, cancellation, and stale-result protection.
- **Commands to run:** focused Rust/React tests; `pnpm check`; strict relevant Clippy; `git diff --check`; full gates only at the combined apply acceptance boundary.
- **Required reviews and artifacts:** self-review now; combine one Security Reviewer with the authority-bearing apply checkpoint. No screenshot gate unless onboarding geometry changes materially.
- **Explicit non-goals:** destination selection, writes, Category mapping/apply, in-place adoption, migration, Git, Attachments, MCP, source metadata promotion.
- **Definition of done:** deterministic bounded preview works from real native selection, source bytes remain unchanged in tests, renderer receives no authority, focused gates pass, evidence and progress are accurate.
- **Expected commit boundary:** ADR/packet checkpoint, then one preview implementation/evidence checkpoint.
- **Conditions requiring escalation:** new dependency, arbitrary-path IPC, need to follow links, inability to bound resource use, or a required source mutation.
