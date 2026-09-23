# Task Packet: Safe text Attachment preview

- **Objective:** Let the desktop preview a declared UTF-8 Attachment as inert text through a pathless, identity-checked native command.
- **Required reasoning level:** high for core/security boundary; low for the frozen UI wiring.
- **Prerequisites:** Signed Attachment inventory/import/removal checkpoints; ADR 0052; retained Workspace capability and current Guide revisions.
- **Allowed files or subsystem:** `crates/rehow-core/src/{storage.rs,application.rs}`, `apps/desktop/src-tauri/src/lib.rs`, `apps/desktop/src/{App.tsx,app.test.ts,index.css}`, ADR/task/evidence/progress documents. No dependency or capability-manifest change.
- **Relevant specifications and ADRs:** `docs/content-model.md`, `docs/threat-model.md`, ADRs 0004, 0009, 0049, and 0052.
- **Exact required behavior:** Accept open-Workspace generation, Guide ID, Attachment ID, and expected Guide revision only. Recheck the exact on-disk Guide revision before and after reading; resolve the declared Attachment internally; read at most 1 MiB through the retained root; require regular/single-link/stable content; validate optional size and SHA-256; require UTF-8; replace unsafe display scalars and report whether replacement occurred. Return only title, bounded text, size, and `sanitized`. Show it in an inert modal read-only text control with Close/Escape and focus restoration.
- **Important states and edge cases:** Missing Guide/Attachment, stale Guide, changed Guide during read, missing/linked/hardlinked/special/oversized/binary Attachment, size/hash drift, unsafe controls, long lines, narrow window, duplicate requests, and command failure.
- **Security constraints:** No caller path, file URL, arbitrary read, HTML/Markdown rendering, MIME trust, process execution, external opener, write, log content, or error content disclosure. Keep renderer payload at or below 1 MiB.
- **Tests to add or update:** Core success plus stale, symlink/hardlink/special, oversize, binary, digest/size drift, and unsafe-scalar cases; native closed/pathless payload/error tests; React modal/error/focus/static-inert-render tests.
- **Commands to run:** Focused Rust and Vitest while editing; then `cargo test --workspace --all-features`, strict Workspace Clippy, Cargo formatting, `pnpm check`, desktop Vitest, `pnpm build`, and whitespace checks.
- **Required reviews and artifacts:** One final Security Reviewer PASS. No screenshot gate unless shell geometry changes materially.
- **Explicit non-goals:** Binary/image/PDF preview, syntax highlighting, export, reveal, open-with, file URLs, MCP reads, edits, downloads, new dependencies, or schema changes.
- **Definition of done:** Pathless preview works; hostile/stale content fails closed; content stays inert; accessibility behavior is tested; full gate and security review pass; signed commit and `PROGRESS.md` are current.
- **Expected commit boundary:** ADR, core/native/UI/tests/evidence as one signed feature checkpoint; progress ledger follows.
- **Conditions requiring escalation:** Any need for a new dependency/capability, renderer path/URL, external process, attachment execution, broad filesystem access, or ambiguous content-size policy.
