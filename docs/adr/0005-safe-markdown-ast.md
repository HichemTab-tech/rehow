# ADR 0005: Safe Markdown AST Rendering

- Status: Accepted
- Date: 2026-09-05

## Context

Markdown may come from malicious repositories/imports. Rendering raw HTML or arbitrary file/remote media inside a privileged Tauri webview could enable script execution, data disclosure, tracking, or unsafe external actions.

## Decision

Parse supported CommonMark/GFM in Rust and return a versioned typed block/inline AST. React maps only known nodes to components, inserts text normally, and never uses `dangerouslySetInnerHTML`. Raw-HTML source becomes an escaped literal/unsupported AST node; there is no rendered HTML/HAST sanitizer path. No MDX, scripts, iframes, remote images, SVG inline rendering, macros, Mermaid, or executable command controls in v1.

Allow external `http`, `https`, and `mailto` links only after normalization; block `javascript`, `data`, `file`, `blob`, controls, and custom schemes. Opening is explicit through a guarded backend. Managed raster Attachments resolve by opaque ID through typed IPC into bounded, byte-sniffed bytes; the renderer creates short-lived Blob URLs and revokes them promptly. A custom protocol is deferred and would require its own ADR. CSP denies remote connections and inline/eval scripts except narrowly generated build requirements.

## Consequences

- Strong predictable rendering and no hidden Markdown execution.
- Some Markdown extensions/embedded HTML display as unsupported/literal.
- Typed AST versioning and React component coverage are required.
- Attachment formats/limits need explicit Phase 3 review.

## Rejected

- `dangerouslySetInnerHTML`, raw HTML passthrough, remote media by default, broad Tauri opener/asset scopes, and Run-in-terminal features.
