# ADR 0040: Native MCP approval helper endpoint

- Status: Accepted for Phase 6E1B1
- Date: 2026-09-20

## Decision

Package a separate `rehow-mcp-approve` binary with no command-line interface. It accepts one request only on inherited pipe descriptor 3 and returns one button result only on inherited pipe descriptor 4. Both descriptors must be FIFOs with the exact read/write direction. The helper redirects stdin, stdout, and stderr to `/dev/null`, closes every descriptor above 4 with `close_range`, reads with a five-second per-progress timeout, and refuses headless sessions before opening a dialog.

The version-1 request is a twelve-byte header followed by one exact UTF-8 summary: `RHA1`, a closed operation-kind byte (`broad`, `destructive`, `tested`, or `git-write`), three zero reserved bytes, and a big-endian `u32` byte length. Summaries are 1–4,096 bytes, at most 32 nonempty lines and 200 Unicode scalars per line, with no per-line leading/trailing whitespace, control characters other than line feeds, Unicode visual separators, bidi/invisible format controls, noncharacters, trailing data, or silent truncation. The native dialog adds fixed Rehow identity and consent text. It returns byte `1` only for Yes and byte `0` only for No; protocol, pipe, display, and output failures return no result byte.

The helper is still unreachable from MCP in this checkpoint. Phase 6E1B2 must derive/open the sibling without `PATH`, verify owner/type/mode/link count and the packaged build digest, execute the held inode, create the two private pipes, enforce plan expiry/cancellation, and treat every EOF/nonzero/timeout as denial. Until that launcher is accepted, no approval request or mutation may call the helper.

## Consequences

- Malformed, spoofable, oversized, trailing, partial, stalled, and headless requests fail closed and produce no protocol output.
- The helper owns no Workspace, filesystem, Git, network, MCP, token, or mutation authority.
- Exact-pinned `libc 0.2.189` and `gtk-sys 0.18.2` become direct MCP package dependencies; both were already locked and used by Rehow's Linux desktop graph, so no third-party version changed. The helper calls GTK in-process and never uses `rfd`'s `PATH`-resolved Zenity fallback.
- Real positive/negative GUI-button automation remains a manual/portal test; automated tests cover framing, hostile text, headless refusal, stdio isolation, and the exact inherited descriptor set.
