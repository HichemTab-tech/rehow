# Task Packet: Guarded Attachment parent reveal

- **Objective:** Reveal a verified Attachment's containing folder through a descriptor-bearing desktop-portal handoff without opening the Attachment or exposing a path to the renderer.
- **Required reasoning level:** high for filesystem/external-opener boundary; low for frozen UI wiring.
- **Prerequisites:** ADR 0054; signed Attachment preview/export checkpoints; retained Workspace capability.
- **Allowed files or subsystem:** `crates/rehow-core/{Cargo.toml,src/storage.rs,src/application.rs}`, lockfile if dependency-edge metadata changes, `apps/desktop/src-tauri/src/lib.rs`, `apps/desktop/src/{App.tsx,app.test.ts,index.css}`, ADR/task/evidence/progress docs. GIO 0.18.4 is already locked/transitively compiled; no new package version or Tauri capability.
- **Relevant specifications and ADRs:** `docs/content-model.md`, `docs/architecture.md`, `docs/threat-model.md`, ADRs 0052–0054.
- **Exact required behavior:** Accept only Workspace session, Guide ID, exact Guide revision, and Attachment ID. Revalidate the exact Guide and Attachment bytes/size/digest; resolve the parent through retained no-follow descriptors; duplicate that directory descriptor into XDG Desktop Portal `OpenFile`; return `{ revealed: true }` only after accepted handoff. Run blocking verification/handoff off the async IPC executor and allow one reveal request at a time.
- **Important states and edge cases:** stale/missing Guide or Attachment, linked/special/changed/oversized content, parent or intermediate symlink substitution, post-preparation pathname replacement, unavailable portal/handler, Workspace switch, duplicate request.
- **Security constraints:** No Attachment external open, renderer path/URI/bytes, arbitrary process/executable/argument, shell, MIME trust, command execution, logging, mutation, or MCP authority.
- **Tests to add or update:** Core preparation success plus source/Guide/intermediate-symlink refusal and retained-descriptor identity across pathname swaps without launching an app; native closed/pathless result/errors and single-flight recovery; React closed payload/action/busy feedback.
- **Commands to run:** Focused Cargo/Vitest while editing; one full Workspace test/Clippy/format + pnpm check/test/build/bootstrap/whitespace gate.
- **Required reviews and artifacts:** Current web-interface guideline review and one final Security Reviewer PASS. No screenshot unless shell geometry changes materially.
- **Explicit non-goals:** Open Attachment, open-with chooser, select/highlight guarantee, execute, reveal arbitrary path, MCP reveal, schema change, or new package version.
- **Definition of done:** Verified parent folder handoff works pathlessly; unsafe/stale states fail before handoff; no Attachment content reaches an opener; tests/full gate/reviews pass; signed feature and ledger commits exist.
- **Expected commit boundary:** ADR, dependency edge, core/native/UI/tests/evidence; progress ledger follows.
- **Conditions requiring escalation:** Any Attachment URI/open, pathname-based native handoff, caller-selected process/argument, renderer path, shell, handler that executes content, or unresolved data disclosure.
