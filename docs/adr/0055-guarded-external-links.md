# ADR 0055: Guarded external Guide links

- Status: Accepted
- Date: 2026-09-23

## Context

Rehow already parses Markdown links into a renderer-safe AST and retains only canonical ASCII `http`, `https`, and `mailto` destinations. The reader nevertheless renders every link as inert text. Allowing the WebView to navigate directly would bypass the native trust boundary, while accepting an arbitrary renderer URL would create a confused-deputy opener.

## Decision

A user may explicitly open one safe link that exists in the exact current Guide revision. Renderer IPC supplies the open Workspace session, Guide ID, exact Guide revision, and the displayed destination. Core rejects inputs over 2,048 bytes, locates the Guide, requires revision equality, parses its already-bound body through the safe Markdown AST, and requires an exact retained-link match. It then rereads the confined Guide file at that revision before returning an opaque, URL-redacting plan.

Execution requires OpenURI portal interface version 3 or newer and calls the fixed XDG Desktop Portal `org.freedesktop.portal.OpenURI.OpenURI` method through GIO on a blocking single-flight native worker with `ask=true`. The portal must therefore ask the user to choose an application for every request; closing or denying that native interaction launches nothing. The renderer cannot supply an approval flag or bypass this interaction. Only canonical `http`, `https`, or `mailto` links can reach the portal. No file URI, custom scheme, path, shell, caller-selected executable/argument, WebView navigation, prefetch, automatic open, or MCP authority is added. Errors and debug output never include the destination.

## Consequences

- Safe Guide links become useful through an explicit keyboard/click action while the Rehow window remains loaded.
- A changed Guide or Workspace session fails before handoff; the user must reopen the current Guide.
- The user chooses the browser or mail handler in the portal interaction. Rehow reports only that the chooser request was accepted; it does not claim a selection, delivery, or browser success.
- Relative links, files, images, embedded media, and arbitrary schemes remain inert.
