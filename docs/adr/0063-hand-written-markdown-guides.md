# ADR 0063: Hand-written Markdown Guides

- Status: Accepted; core implemented, desktop/MCP adapters pending
- Date: 2026-10-14

## Context

A Workspace is meant to be portable, human-edited Markdown, but the v1 scan accepted only Rehow-written files: slug-only filenames, complete frontmatter, and a mandatory blank line after the closing `---`. A Markdown file written by hand inside a Category directory was reported as `<unsupported>` or `MalformedGuide` with no reason.

## Decision

1. **Separator.** The blank line after the closing `---` is optional; the body may start on the next line, or the file may end at the delimiter. The separator range is empty in that case; edits keep the layout and never insert the blank line.
2. **Filenames.** A Guide filename is any safe Attachment-style filename (one NFC component, no separators, controls, bidi controls, BOM, or noncharacters, at most 255 bytes) that ends in `.md`, does not start with `.`, and does not end in `~`. Scan, candidate counting, recovery journals, and trash paths share this one predicate. Rejected names are shown with the inert percent-escaped display. Links, hardlinks, FIFOs, and sockets remain unsafe.
3. **Diagnostics.** A malformed Guide issue carries a closed, content-free diagnostic: code, optional known field, and optional file-relative position.
4. **Unadopted Guides.** A file with no frontmatter, an unterminated or non-mapping `---` block, a comment-only block, or a valid mapping that lacks required fields is listed as *unadopted*. Missing metadata is derived (first `# ` heading outside fences or the humanized filename; first paragraph; `draft`; file mtime; empty tags/aliases). Its id is a path-bound UUIDv7-shaped value with a zero timestamp, so a rename changes it. Present fields are validated strictly. An invalid reserved field or a YAML error is still a malformed Guide. Unadopted Guides are searchable with effective status `draft` and readable. Listing never writes.
5. **Adoption.** The first explicit save adopts the file inside the existing journaled, revision-checked document update. Rehow keeps any BOM and adds canonical frontmatter plus one blank line before the unchanged body bytes. For partial frontmatter, it inserts only the missing required keys before the closing delimiter. Flow-style or indented root mappings are refused. The derived id is never written: the file's own `id` or a fresh UUIDv7 is used and returned to the caller. The file is never renamed. Unadopted Guides cannot be approved as `tested`.

## Consequences

- Hand-written Markdown works without an import step, and nothing is adopted silently: only an explicit save writes.
- Callers must handle a Guide id changing on its first save (`GuideUpdateOutcome::adopted_id`), and derived ids changing on rename.
- Category directories still need `category.yaml` and keep slug names. Category moves of unadopted Guides need a save first.
- The deferred in-place adoption of whole folders (ADRs 0050/0051) is unchanged. This ADR covers per-file adoption inside an existing Workspace only.
