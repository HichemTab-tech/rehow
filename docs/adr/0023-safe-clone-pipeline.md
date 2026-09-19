# ADR 0023: Safe clone pipeline

- Status: Accepted; Phase 5B3A tree envelope and 5B3B1 destination plan implemented, fetch/materialization pending
- Date: 2026-09-19

## Context

Clone is Rehow's first operation that combines an untrusted network peer, credentials, Git object parsers, destination mutation, and portable Workspace content. Running `git clone` would let Git choose configuration and write an unvalidated checkout directly into the selected folder. That violates Rehow's filesystem, credential, recovery, and Markdown trust boundaries.

## Decision

Clone is one user operation implemented through three internal gates. No gate may weaken the final behavior.

### 5B3A — incoming snapshot envelope

Core parses only NUL-framed `git ls-tree -r -l -z --full-tree` output tied to one advertised commit. SHA-1 and SHA-256 object IDs are accepted only as canonical lowercase full-width hex and every entry must use the same object format. Records must be strictly byte-sorted and unique. Only regular non-executable blobs (`100644 blob`) are accepted; trees are implicit and symlinks, executable blobs, gitlinks/submodules, sparse entries, alternate object types, abbreviated IDs, missing sizes, malformed framing, and non-UTF-8 paths are refused.

The accepted file set is exactly:

- `rehow.yaml`;
- `categories/<valid-directory>/category.yaml`;
- `categories/<valid-directory>/<valid-guide>.md`;
- `attachments/<canonical-guide-id>/<valid-attachment-name>`; and
- optional root `.gitignore`.

Every other root file, nested file, `.git*` control file, `.gitattributes`, `.gitmodules`, `.rehow` recovery content, traversal spelling, non-NFC name, control/bidi name, or schema-invalid component is refused. Empty `categories/` and `attachments/` directories are recreated by Rehow because Git does not track empty directories. File, count, managed-byte, and Attachment-byte caps are the same as a normal Workspace scan; the tree listing itself is capped at 32 MiB. `rehow.yaml` is mandatory. A domain-separated plan hash binds the advertised commit and every path/object/size tuple. Debug output exposes only the entry count and object format, never paths, object IDs, or the plan hash.

5B3A has no filesystem, process, network, credential, renderer, or mutation authority. Its boundary remains crate-private until the fetch gate supplies trusted output.

### 5B3B — confirmed isolated fetch and content validation

The native app selects and retains an existing empty writable directory. Planning binds its device/inode/capability identity, exact empty inventory, URL hash, validated transport/host, random one-use token, and launch epoch. Apply consumes the native-held plan only after a native confirmation displays the exact secret-free URL. It repeats the empty/ancestor/writability checks before mutation.

The accepted 5B3B1 slice exposes only `git_clone_plan_selected_workspace`: it uses the already retained native empty-folder capability, accepts one bounded secret-free URL, replaces any older pending clone plan, and returns only a random plan token, transport, and host. Core revalidates emptiness, root identity/capability, effective writability, ancestor repository state, and Git-marker safety. The selected path, URL, hashes, and root identity never cross renderer IPC or `Debug`. This planning slice performs no confirmation, process, network, credential, marker, Git, or filesystem write; those remain in the apply gate.

Rehow creates a private direct `.git` scaffold itself and records a versioned `rehow-clone` provenance marker before network access. The marker contains no URL or credential: it binds the destination identity, URL hash, object format, operation ID, and current pipeline stage. Only exact marker-bound residue can resume after cancellation or interruption; unrelated residue is preserved and refused. Forgetting an interrupted clone is a future separately confirmed identity-safe quarantine operation, never automatic deletion.

Git receives the canonical URL explicitly and never reads repository, worktree, environment-selected, or included Git configuration. The fixed fetch flow is:

1. discover the remote `HEAD` symbolic ref and full advertised OID with protocol v2 and bounded machine output;
2. initialize the matching local SHA-1 or SHA-256 object format without templates;
3. fetch only that advertised OID into an operation-specific `refs/rehow/clone/<token>` staging ref, with tags, submodules, checkout, hooks, filters, fsmonitor, alternates, shallow negotiation, arbitrary refspecs, and `FETCH_HEAD` disabled;
4. list the complete committed tree and pass it through 5B3A;
5. stream each declared blob through `cat-file --batch`, requiring exact OID/type/size and bounded EOF behavior;
6. parse and bind the manifest, every Category, every Guide, and all declared Attachment metadata before any Workspace content is published; reject malformed/newer schema, duplicate IDs, Category mismatches, missing/orphan Attachments, hash/size mismatches, or any scan issue.

The Git child runs under Bubblewrap with a read-only system image, a private synthetic home/config, a writable bind only for the marker-bound Git metadata, no worktree bind, no shell-selected command, a fixed process group, bounded output/time, and explicit network sharing. Cancellation terminates the whole process group and reports `cancelled`; it never claims rollback and leaves only recognized resumable metadata.

SSH uses only the launch-inherited `SSH_AUTH_SOCK` after no-follow socket/type/owner checks. A compiled Rehow SSH proxy accepts only Git's documented upload-pack argument shape and invokes root-owned `/usr/bin/ssh` by argv with a synthetic home, batch mode, strict host checking, fixed timeouts, no user SSH config, and only bounded owner-safe `known_hosts` data plus root-owned system host keys. Rehow never reads private key files.

HTTPS is anonymous unless an audited credential helper can be reconstructed from root-owned system or owner-safe user Git config read with includes disabled. Accepted helpers resolve to a root-owned executable in `/usr/bin` or `/usr/lib/git-core` with no free-form arguments; the exact root-owned `gh auth git-credential` argv is also recognized. Shell helpers, relative/owner-writable executables, URL-scoped executable overrides, includes, askpass, prompts, proxies, and repository helpers are rejected. A compiled credential proxy forwards the normal Git credential protocol through pipes; secret fields never enter plans, renderer IPC, errors, Debug, logs, evidence, or Rehow storage. Missing/unsupported helpers and host-key/auth failures return stable structured states.

### 5B3C — journaled materialization and ref installation

Validated blob bytes are stored as recovery objects and published only by one `GitMaterialize` journal whose targets all expect absence. Core creates only the proven Category and Attachment directory skeleton, with exact private modes and no-follow descriptors, after the journal is durable. The manifest is included in the same journal rather than used as an early success marker. Resume uses the marker's operation/plan hash and the normal journal state machine; external files produce a conflict without overwrite.

After every content target is durable, Rehow opens the resulting Workspace through the normal scanner and requires an issue-free view whose Workspace ID, revisions, inventory, and Attachment hashes match the validated plan. Only then does it append the canonical `origin` config, create the local branch and remote-tracking ref with expected-absent OID compare-and-swap, point `HEAD` at that branch, fsync Git metadata, and transition the marker to complete. It never checks out, merges, resets, force-updates, runs hooks/filters, or deletes unrelated data. Ref/config races preserve existing state and return conflict.

The renderer receives only stage, transport, host, bounded progress counts, and stable error codes. It never receives the destination path, URL after submission, ref/object IDs, credentials, Git stderr, filenames, or file content.

## Consequences

- Clone can accept only a canonical Rehow Workspace snapshot; ordinary Markdown adoption remains a different explicit workflow.
- Repositories containing executable files, symlinks, submodules, `.gitattributes`, `.gitmodules`, extra documentation, or portable `.rehow` trash are refused rather than partially cloned.
- First-time SSH hosts must already have trusted host-key material. Rehow does not introduce an in-app TOFU prompt in this phase.
- Unsupported credential helpers degrade to anonymous HTTPS and an actionable authentication-required result; Rehow never asks for a token.
- Downloaded Git objects may remain in exact marker-bound residue after interruption. They are inert and resumable; automatic cleanup is intentionally deferred until identity-safe quarantine is implemented.
- Phase 5B3 is not accepted until 5B3B and 5B3C pass the hostile-remote, credential-canary, cancellation, race, recovery, native interaction, and single Security Reviewer gates.
