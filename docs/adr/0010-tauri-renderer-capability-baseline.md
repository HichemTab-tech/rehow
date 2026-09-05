# ADR 0010: Tauri Renderer Capability Baseline

- Status: Accepted for Phase 2 implementation
- Date: 2026-09-05

## Context

The Tauri webview is untrusted. A generic dialog result passed back as a renderer-controlled path, broad plugins, navigation, devtools, or permissive CSP would bypass Workspace confinement or leak content.

## Decision

- Native Rehow commands own Workspace/import/Attachment/destination dialogs, retain descriptor-bound selections, and return opaque single-purpose tokens. The renderer never supplies an absolute path to inspect, open, or mutate.
- No Tauri filesystem, shell, process, HTTP, global-shortcut, generic opener, or broad asset-protocol capability is exposed. Only named versioned Rehow commands are allowed for the main window.
- Block non-application navigation and new windows. External `http/https/mailto` opening requires a native confirmation naming scheme/host and an allowlisted native operation. Attachments are never externally opened.
- Production devtools are disabled. Production CSP baseline is `default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' blob:; font-src 'self'; connect-src ipc: http://ipc.localhost; object-src 'none'; frame-src 'none'; base-uri 'none'; form-action 'none'`, adjusted only to exact generated Tauri protocol requirements after a Security Reviewer diff. No remote origin, `unsafe-eval`, `data:` media, wildcard, or remote connect source.
- Safe raster bytes returned by typed IPC become bounded Blob URLs with byte-sniffed MIME and are revoked promptly. Any custom protocol later needs its own ADR, path tests, fixed `Content-Type`, `nosniff`, and no ambient asset scope.
- Development CSP exceptions are dev-only and cannot enter release bundles.

## Required gate evidence

- Snapshot exact Tauri capabilities/config/CSP and fail on forbidden plugins/scopes/directives.
- Negative IPC tests forge path-like values and opener requests.
- Production native test observes no network request for malicious Markdown/media/CSS.
- Verify navigation/new-window blocking, release devtools setting, Blob URL cleanup, and native confirmation/focus behavior.

## Consequences

- Dialog UX is implemented by Rust adapter commands rather than a generic frontend plugin call.
- Third-party component patterns using inline styles or external assets may need adaptation, not CSP relaxation.
