# ADR 0023: Safe clone pipeline

- Status: Accepted; anonymous-HTTPS native and renderer pipeline implemented; authenticated transports pending
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

The accepted 5B3B2/5B3B3 slices implement anonymous HTTPS through a single-use native apply command. A parented native dialog displays the exact already-validated secret-free URL; declining consumes the plan without network or staging. Confirmed work runs off the UI thread, exposes one pathless cancellation flag, and retains only an opaque validated session for 5B3C. The selected destination capability is held out of renderer control and never bound into the Git child. Every success and recoverable failure revalidates destination emptiness; changed or uncertain destinations require reselection.

Before network access, Rehow creates a versioned private staging repository under its owner-only XDG state root, not inside the selected folder. Its provenance record contains no URL or credential: it binds the selected destination identity/capability hash, URL hash, operation ID, object format once known, and current pipeline stage. The selected destination must remain exactly empty through discovery, fetch, and validation. Only exact provenance-bound staging state can resume after cancellation or interruption; unrelated state is preserved and refused. Superseded staging is moved by descriptor identity to an inert owner-only quarantine rather than name-deleted.

Git receives the canonical URL explicitly and never reads repository, worktree, environment-selected, or included Git configuration. The fixed fetch flow is:

1. discover the remote `HEAD` symbolic ref and full advertised OID with protocol v2 and bounded machine output;
2. initialize the matching local SHA-1 or SHA-256 object format without templates;
3. shallow-fetch only that advertised OID into an operation-specific staging ref with fsck, a maximum-blob filter, forced pack retention, a hard file-size rlimit, and a descriptor-confined aggregate staging limit; tags, submodules, checkout, hooks, filters, fsmonitor, alternates, arbitrary refspecs, and `FETCH_HEAD` remain disabled;
4. require that exact ref to name a commit rather than another tree-ish type, then list its complete tree and pass it through 5B3A;
5. stream each declared blob through `cat-file --batch`, requiring exact OID/type/size, independently recomputing its SHA-1 or SHA-256 Git object ID, and enforcing bounded EOF behavior;
6. parse and bind the manifest, every Category, every Guide, and all declared Attachment metadata before any Workspace content is published; reject malformed/newer schema, duplicate IDs, Category mismatches, missing/orphan Attachments, hash/size mismatches, or any scan issue.

The Git child runs under Bubblewrap with a read-only system image, a private synthetic home/config, a writable bind only for the provenance-bound XDG staging metadata, no destination/worktree bind, no shell-selected command, a fixed process group, bounded output/time/storage, and explicit network sharing. Staging descriptors remain close-on-exec except for the exact descriptor enabled in the selected child's pre-exec boundary. Cancellation terminates the whole process group and reports `cancelled`; it never claims rollback and leaves only recognized resumable staging metadata. A crash before final materialization therefore leaves the selected destination empty.

SSH uses only the launch-inherited `SSH_AUTH_SOCK` after no-follow socket/type/owner checks. A compiled Rehow SSH proxy accepts only Git's documented upload-pack argument shape and invokes root-owned `/usr/bin/ssh` by argv with a synthetic home, batch mode, strict host checking, fixed timeouts, no user SSH config, and only bounded owner-safe `known_hosts` data plus root-owned system host keys. Rehow never reads private key files.

HTTPS is anonymous unless an audited credential helper can be reconstructed from root-owned system or owner-safe user Git config read with includes disabled. Accepted helpers resolve to a root-owned executable in `/usr/bin` or `/usr/lib/git-core` with no free-form arguments; the exact root-owned `gh auth git-credential` argv is also recognized. Shell helpers, relative/owner-writable executables, URL-scoped executable overrides, includes, askpass, prompts, proxies, and repository helpers are rejected. A compiled credential proxy forwards the normal Git credential protocol through pipes; secret fields never enter plans, renderer IPC, errors, Debug, logs, evidence, or Rehow storage. Missing/unsupported helpers and host-key/auth failures return stable structured states.

### 5B3C — journaled materialization and ref installation

Validated blob bytes are stored as recovery objects and published only by one `GitMaterialize` journal whose targets all expect absence. Immediately before that journal, Rehow revalidates the selected empty capability and creates an exact `rehow-clone` finalization marker inside a private direct `.git` scaffold. Core creates only the proven Category and Attachment directory skeleton, with exact private modes and no-follow descriptors, after the journal is durable. The manifest is included in the same journal rather than used as an early success marker. Resume uses the XDG provenance plus finalization marker's operation/plan hash and the normal journal state machine; external files produce a conflict without overwrite.

After every content target is durable, Rehow opens the resulting Workspace through the normal scanner and requires an issue-free view whose Workspace ID, revisions, inventory, and Attachment hashes match the validated plan. Only then does it append the canonical `origin` config, create the local branch and remote-tracking ref with expected-absent OID compare-and-swap, point `HEAD` at that branch, and fsync Git metadata. The original marker remains immutable recovery provenance; an operation/plan-bound completion seal is published last with no-replace semantics. This append-only state transition avoids overwriting or quarantining trusted recovery evidence. Rehow never checks out, merges, resets, force-updates, runs hooks/filters, or deletes unrelated data. Object/ref/config races preserve existing state and return conflict.

The renderer receives only stage, transport, host, bounded progress counts, and stable error codes. It never receives the destination path, URL after submission, ref/object IDs, credentials, Git stderr, filenames, or file content.

The accepted 5B3C1 slice implements the portable-content half of this gate. It re-reads every staging file through no-follow private descriptors, independently rechecks its Git blob ID, stores exact bytes as recovery objects, and publishes the complete accepted tree through one `GitMaterialize` journal. Decoded clone journals permit only expected-absent `0600` object creation. The journal is durable before the proven `0700` Category/Attachment directory skeleton is created. An exact `.git/rehow-clone` marker and empty private scaffold bind the operation ID, destination, and clone-tree plan hash. A crash during scaffold construction may complete only missing canonical directories after a mutation-free preflight proves the exact marker and rejects every foreign or unsafe entry; a missing marker is never recreated. Interrupted journal application resumes only after the persisted intent matches and its current inventory contains no foreign entries. Final descriptor-confined inventory and Git blob-ID checks cover every accepted file, including `.gitignore`, and the canonical scanner must reproduce the validated Workspace fingerprint with no issues. Git object transfer, config, `HEAD`, branch, and remote-tracking refs remain the separate 5B3C2 finalization gate, so this slice is not exposed as completed clone success.

The accepted core 5B3C2 slice copies only bounded, paired, stable pack/index files from the retained staging capability; builds a fresh operation-scoped staging index; and installs exact shallow, config, local-ref, remote-ref, and `HEAD` metadata with no-replace-or-verify semantics. Confined status must report the expected clean zero-divergence branch and upstream. Exact portable inventory is rechecked immediately before and after the completion seal so ignored concurrent residue cannot be mistaken for success.

The accepted native 5B3C3 slice consumes only the opaque retained validation after a second native confirmation, restores the exact validated retry authority after decline or failure, and installs the completed issue-free Workspace as the active watched session. Finalization cannot be cancelled after writes begin. The accepted renderer 5B3C4 slice exposes the lifecycle only after native empty-folder selection: bounded anonymous-HTTPS entry, native-confirmed validation, cancellation while validation is active, a clear validated/no-write state, explicit native-confirmed completion, and completed Workspace handoff. Renderer payloads remain path-free, URL-free after planning, closed, and bounded.

## Consequences

- Clone can accept only a canonical Rehow Workspace snapshot; ordinary Markdown adoption remains a different explicit workflow.
- Repositories containing executable files, symlinks, submodules, `.gitattributes`, `.gitmodules`, extra documentation, or portable `.rehow` trash are refused rather than partially cloned.
- First-time SSH hosts must already have trusted host-key material. Rehow does not introduce an in-app TOFU prompt in this phase.
- Unsupported credential helpers degrade to anonymous HTTPS and an actionable authentication-required result; Rehow never asks for a token.
- Downloaded Git objects may remain in exact provenance-bound XDG staging after interruption. They are inert and resumable; automatic deletion is forbidden and identity-safe quarantine is part of the accepted cleanup path. Destination residue begins only at the final journaled materialization stage and is marker-bound.
- Phase 5B3 is not accepted until 5B3B and 5B3C pass the hostile-remote, credential-canary, cancellation, race, recovery, native interaction, and single Security Reviewer gates.
