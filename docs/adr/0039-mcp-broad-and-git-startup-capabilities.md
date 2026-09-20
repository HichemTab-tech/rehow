# ADR 0039: Explicit MCP broad and Git-write startup capabilities

- Status: Accepted for Phase 6E1A
- Date: 2026-09-20

## Decision

Add repeatable `--allow-broad <alias>` and `--allow-git-write <alias>` startup grants beside `--allow-write <alias>`. Each grant is bound only to a configured Workspace alias. Unknown aliases, duplicate grants, malformed aliases, and broad/Git-write grants without write authority for the same alias reject the entire launch.

The server reports the effective `read`, `write`, `broad`, and `gitWrite` capability tuple for each random launch-scoped Workspace handle. These flags grant no path access and expose no new mutation by themselves. Every future broad, destructive, tested, or Git-write operation must additionally use an operation-specific short-lived plan, current-state revalidation, and the fixed out-of-band native approval boundary.

## Consequences

- Read-only remains the default and capability order on the command line is irrelevant.
- Broad and Git-write authority cannot exist without the recovery prerequisites already required for writes.
- This checkpoint adds no tools, helper process, plan token, mutation, Git subprocess, network, or tested-status authority.
- Native approval and in-memory plan execution remain separate security-reviewed gates.
