# ADR 0022: Staged Git onboarding

- Status: Accepted for local initialization and remote connection; clone continues in ADR 0023
- Date: 2026-09-19

## Context

Phase 5 must initialize an open Workspace, connect an existing repository to a remote, and clone a Git-backed Workspace. These operations do not share one safe primitive: initialization is local metadata creation, connecting mutates untrusted repository configuration, and cloning combines destination mutation, network credentials, hostile objects, validation, and materialization.

## Decision

Split onboarding into three independently accepted subgates without weakening the required final scope.

Phase 5B1 initializes only the currently open, writable, compatible Workspace. A pathless plan records a random single-use token, Workspace identity and manifest revision, launch-scoped retained-root capability identity, the fixed `main` initial branch, and either an absent `.git` or an exact marker-bound canonical partial initialization. Apply requires the native-held plan, a matching renderer token, unchanged state, a fresh writable/ancestor inspection, and an out-of-band native confirmation. Rust creates a minimal direct Git directory itself; it does not run Git, templates, hooks, helpers, config, or network code. Directories are private and files are single-link regular files published no-replace from fsynced unnamed same-filesystem inodes. A private provenance marker makes an interrupted canonical initialization resumable and remains inert after completion, avoiding any name-based deletion race; unknown or differing residue is refused. Every created file and directory is flushed before success.

Phase 5B2 connects an existing direct repository without contacting the remote. The request contains a remote name and URL; the native apply request contains only a random single-use plan token. Names are 1–64 lowercase ASCII characters, start alphanumeric, contain only alphanumeric, `.`, `_`, or `-`, cannot contain consecutive dots, and cannot end in `.lock`. URLs are at most 2,048 ASCII bytes and use one of three exact secret-free forms: canonical `https://host[:port]/path`, canonical `ssh://[user@]host[:port]/path`, or strict `[user@]host:path` SCP syntax. HTTPS userinfo, every password, query, fragment, percent escape, whitespace/control/backslash, empty/root paths, and `.`/`..` path components are rejected. SSH usernames and hosts use conservative ASCII grammar. Rehow returns only the transport and host to the renderer; it never returns or logs the URL.

Planning opens the direct repository through its retained Workspace capability, reads a bounded owner-controlled single-link regular `config`, validates it with root-owned Git in `--file`, `--no-includes`, name-only mode, and rejects an existing same-name remote. Candidate construction appends one canonical quoted `[remote "name"]` section while retaining every existing byte exactly. Both original and candidate are parsed before the plan is accepted. The plan binds Workspace identity, launch capability, config revision, candidate hash, remote inputs, and a random token. Debug output redacts the URL.

Apply reopens and rechecks the repository and both exact byte states after native confirmation. It publishes a fully written, fsynced `config.lock` no-replace, atomically exchanges it with `config`, verifies the planned inode/byte orientation, and fsyncs the Git directory. It then moves whichever inode occupies `config.lock` no-replace to a random inert `rehow-config-before-v1-*` name and syncs again; it never unlinks or rolls a name back after verification. The expected before-image becomes an owner-mode-matched local backup on success. If an uncooperative writer replaces either name in a check/action race, every observed inode is preserved under `config` or an inert backup and the operation reports conflict rather than performing a second destructive exchange. A crash before or after exchange is resumed only when the user repeats the same remote request and confirms again: the two standard names must equal the exact old/candidate pair in one of the two known orientations. Any unrelated lock, malformed config, stale revision, or ambiguous state is preserved and refused. No copy leaves `.git`; automatic backup pruning is deferred until an identity-safe retention policy exists.

Duplicate remote names are an error rather than an update. Remote edit/rename/removal is deferred to a separately planned operation. Phase 5B2 performs no DNS, network, fetch, authentication, credential-helper, SSH, hook, or repository-config execution.

Later network phases must run from a synthetic config-free Git environment and pass a freshly revalidated canonical remote URL explicitly. Repository configuration can never choose credential helpers, transports, proxies, SSH commands, hooks, aliases, filters, or executables. SSH may use only the launch-inherited agent socket after descriptor/type/ownership checks. HTTPS credentials may use only separately audited system/user Git credential configuration and the normal Git credential protocol; secrets remain in process pipes, never enter renderer IPC, logs, plans, errors, or Rehow storage. Exact provenance and hostile-config tests remain a required acceptance gate for the first network operation.

ADR 0023 supplies the Phase 5B3 clone decision. It requires a natively selected unchanged empty destination, fetch without checkout into confined Git metadata, complete incoming tree/path/type/size/schema validation, core-journaled materialization, expected-OID ref CAS, fixed SSH-agent and audited credential-helper behavior, marker-bound recovery, cancellation, authentication states, and hostile-remote evidence. Git never checks out directly into active content.

## Consequences

- Safe local initialization can ship without prematurely granting config, credential, network, checkout, or content-write authority to Git.
- Remote connection configures metadata only; “connected” never implies reachable or authenticated.
- Clone remains explicit required work, not an omission or external-tool substitute.
- Each later delta receives one security review at its mutation checkpoint.
