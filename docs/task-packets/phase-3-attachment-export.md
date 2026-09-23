# Task Packet: Safe Attachment export

- **Objective:** Export an exact declared Attachment through a native-selected, absent-only destination without renderer path authority.
- **Required reasoning level:** high for the filesystem boundary; low for frozen UI wiring.
- **Prerequisites:** ADR 0053; signed Attachment preview/inventory/import checkpoints; retained Workspace capability.
- **Allowed files or subsystem:** `crates/rehow-core/src/application.rs`, `apps/desktop/src-tauri/src/lib.rs`, `apps/desktop/src/{App.tsx,app.test.ts,index.css}`, ADR/task/evidence/progress documents. No dependency, capability-manifest, schema, or MCP change.
- **Relevant specifications and ADRs:** `docs/content-model.md`, `docs/threat-model.md`, ADRs 0009, 0052, and 0053.
- **Exact required behavior:** Accept only Workspace session, Guide ID, exact Guide revision, and Attachment ID through IPC. Native presents Save using the managed filename. Core resolves and validates the source, verifies size/SHA-256 and pre/post Guide revision, opens a no-symlink destination parent, creates and flushes an unnamed `0600` file, publishes absent-only, flushes the parent, and returns only `cancelled` or `exported`.
- **Important states and edge cases:** cancel, missing/stale Guide or Attachment, source drift/link/special/oversize/hash mismatch, existing destination, destination symlink, unsafe ancestry, unsupported filesystem, write/flush/publication failure, same source and destination, Workspace switch, duplicate request.
- **Security constraints:** No renderer path/file URL, overwrite, named partial, shell/process, opener, MIME trust, content logging, recursive filesystem access, or MCP authority. Never delete a destination.
- **Tests to add or update:** Core success/cancel-independent helper, existing/symlink destination, source drift/link/special, unsafe parent, exact bytes/mode; native closed request/result/error mapping; React action/busy/result validation.
- **Commands to run:** Focused Cargo/Vitest while editing; final Workspace Cargo tests/Clippy/format, `pnpm check`, desktop Vitest, production build, Bootstrap validation, and whitespace check.
- **Required reviews and artifacts:** One final Security Reviewer PASS. Current web-interface guideline review; no screenshot unless shell geometry changes materially.
- **Explicit non-goals:** Overwrite, reveal, open-with, execution, preview expansion, Attachment mutation, arbitrary destination API, MCP export, new dependency, or schema change.
- **Definition of done:** Exact bytes export atomically to a new native-selected file; hostile/stale/raced states fail closed; renderer remains pathless; tests/full gate/reviews pass; signed feature and progress commits exist.
- **Expected commit boundary:** ADR, core/native/UI/tests/evidence in one signed commit; progress ledger follows.
- **Conditions requiring escalation:** A renderer path, overwrite/delete, external process, unsupported primitive fallback that can leave partial output, new dependency/capability, or unresolved publication ambiguity.
