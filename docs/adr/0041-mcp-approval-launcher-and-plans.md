# ADR 0041: MCP approval launcher and in-memory plans

- Status: Accepted for Phase 6E1B2
- Date: 2026-09-20

## Decision

The MCP server captures only a sibling named `rehow-mcp-approve`, derived from its own executable directory. Production packaging must compile the server with `REHOW_MCP_APPROVAL_HELPER_BLAKE3` set to the exact lowercase BLAKE3 digest of the already-built helper. Missing or malformed build-time identity makes native approval unavailable. Capture opens the sibling with `O_NOFOLLOW | O_CLOEXEC`, requires a bounded regular executable with one link, effective-user or root ownership, no set-ID bits, and no group/world write bits, copies and hashes the held bytes into a private executable memfd, rechecks the source identity and metadata after reading, removes write permission, applies immutable size/write/seal seals, and retains only an `O_PATH` descriptor to that exact snapshot.

Each launch duplicates the retained sealed-snapshot descriptor above the protocol range, creates two anonymous close-on-exec pipes, forks, maps only the child pipe ends to descriptors 3 and 4, closes every other child descriptor except the executable, and calls `execveat(..., AT_EMPTY_PATH)` on that held descriptor. It never resolves `PATH`, invokes a shell, or reopens the sibling path. The helper receives no arguments and only a fixed allowlist of bounded graphical-session environment variables; loader, GTK-module, path, credential, and effect variables are not inherited. The parent writes the exact Phase 6E1B1 frame, closes the request channel, and accepts approval only when it reads exactly byte `1`, observes EOF, and reaps a zero-exit child before the monotonic deadline. Cancellation, expiry, malformed output, timeout, signal, nonzero exit, EOF, or pipe failure kills and reaps the helper process group and denies approval.

Approval plans remain process-local and unreachable from MCP in this phase. At most 64 plans may exist. Each has a random opaque token, a closed operation with one fixed approval kind, Workspace-handle binding, keyed hash of the exact server-owned effect binding, already-validated plain-text effect summary, and a maximum 90-second monotonic lifetime. Execution atomically changes a matching pending plan to active; replay, wrong binding, wrong operation, expiry, or concurrent execution fails. Cancellation removes a pending plan or flags an active plan so its helper is killed. Every attempted execution consumes the plan regardless of outcome.

## Consequences

- Neither a swapped sibling path nor an in-place source overwrite can change the immutable snapshot executed after capture; a different packaged build cannot pass the embedded digest check.
- The MCP client receives neither an approval channel nor authority to construct approval text, environment, executable paths, or consent results.
- Same-user process tracing, memory modification, and GUI automation remain outside the hard boundary already documented in the threat model.
- No broad, destructive, tested, Attachment, or Git mutation becomes available in Phase 6E1B2.
- Release packaging must build/hash the helper before compiling the server and must install the pair as siblings. Phase 7 will automate and verify that ordering.
