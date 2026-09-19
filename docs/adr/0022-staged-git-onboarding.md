# ADR 0022: Staged Git onboarding

- Status: Partially Accepted — local initialization only; connect and clone remain proposed
- Date: 2026-09-19

## Context

Phase 5 must initialize an open Workspace, connect an existing repository to a remote, and clone a Git-backed Workspace. These operations do not share one safe primitive: initialization is local metadata creation, connecting mutates untrusted repository configuration, and cloning combines destination mutation, network credentials, hostile objects, validation, and materialization.

## Decision

Split onboarding into three independently accepted subgates without weakening the required final scope.

Phase 5B1 initializes only the currently open, writable, compatible Workspace. A pathless plan records a random single-use token, Workspace identity and manifest revision, launch-scoped retained-root capability identity, the fixed `main` initial branch, and either an absent `.git` or an exact marker-bound canonical partial initialization. Apply requires the native-held plan, a matching renderer token, unchanged state, a fresh writable/ancestor inspection, and an out-of-band native confirmation. Rust creates a minimal direct Git directory itself; it does not run Git, templates, hooks, helpers, config, or network code. Directories are private and files are single-link regular files published no-replace from fsynced unnamed same-filesystem inodes. A private provenance marker makes an interrupted canonical initialization resumable and remains inert after completion, avoiding any name-based deletion race; unknown or differing residue is refused. Every created file and directory is flushed before success.

Phase 5B2 connect will receive a follow-up accepted delta before code exists. It must define secret-free HTTPS/SSH URL grammar, remote-name rules, config revision/CAS, lossless config mutation, atomic recovery, duplicate handling, and the exact trusted credential provenance used by later network operations. Repository configuration cannot select helpers or commands.

Phase 5B3 clone will receive a follow-up accepted delta before code exists. It must use a natively selected unchanged empty destination, fetch without checkout into confined Git metadata, validate the complete incoming tree/path/type/size/schema set, materialize accepted Workspace bytes only through the core recovery journal, and install refs with expected-OID CAS. It must define fixed SSH-agent and audited credential-helper behavior, cleanup/recovery, cancellation, authentication, and hostile remote evidence. Git never checks out directly into active content.

## Consequences

- Safe local initialization can ship without prematurely granting config, credential, network, checkout, or content-write authority to Git.
- Connect and clone remain explicit required work, not omissions or external-tool substitutes.
- Each later delta receives one security review at its mutation checkpoint.
