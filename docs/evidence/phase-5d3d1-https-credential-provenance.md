# Phase 5D3D1 HTTPS credential-helper provenance evidence

Date: 2026-09-20

## Implemented boundary

This gate captures configuration and executable provenance only. It does not invoke a credential helper, speak Git's credential protocol, read or retain credential values, contact a remote, alter a Workspace, or add renderer/MCP authority.

- Native launch considers only `/etc/gitconfig`, the XDG global config, and the home global config in Git precedence order. Existing files must be stable bounded one-link regular files with expected ownership, non-writable modes, safe ancestry, and matching path/descriptor identity.
- Root-owned Git parses each retained byte snapshot on stdin with system/global discovery disabled and includes explicitly disabled. Empty `credential.helper` values reset prior selections; multiple effective or URL-scoped helpers fail closed.
- Accepted values are one argument-free named or absolute `git-credential-*` executable within `/usr/lib/git-core` or `/usr/bin`, or the exact `!/usr/bin/gh auth git-credential` declaration. The selected root-owned executable is retained by `O_PATH` inode capability after repeated metadata/path checks.
- Status and `Debug` reveal only `notConfigured`, `trusted`, or `unsafe`. Paths, config values, helper names, file identities, and bytes remain private.
- Parent directory identity no longer includes link count: unrelated sibling directory creation cannot forge a replacement because device/inode/owner/mode remain bound, while target config/socket/helper files still require and recheck exactly one link. This closes the previously observed parallel-test flake without weakening the target boundary.

## Adversarial evidence

Five focused core tests cover named, absolute, and exact reconstructed `gh` selection; public redaction; ordered reset; disabled hostile includes; multiple/scoped/shell/argument/relative/missing/malformed rejection; wrong ownership; writable, symlinked, hardlinked, oversized, and output-amplifying inputs; helper hardlinks; and deterministic config/helper replacement races. A native test proves launch retention without IPC or public data.

## Gate status

Focused tests, `pnpm check`, strict all-Workspace Clippy, 39 Vitest tests, 432 core tests with 2 intentional ignores, all contract/API/desktop/MCP targets, production build, optimized Tauri no-bundle build, Cargo formatting, bootstrap validation, and whitespace checks pass.

The single reused Security Reviewer found that the first scoped-key predicate omitted Git's valid empty subsection spelling `credential..helper`. The predicate now rejects that equal-length form and the adversarial corpus includes `[credential ""]`. Focused tests and strict core Clippy pass after the correction; the same reviewer returned **PASS**.
