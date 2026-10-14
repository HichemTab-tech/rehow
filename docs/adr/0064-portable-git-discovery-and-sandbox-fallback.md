# ADR 0064: Portable Git discovery and observable sandbox fallback

- Status: Accepted and implemented
- Date: 2026-10-14
- Supersedes in part: ADR 0021 fixed-path consequence; ADR 0023/0025 fixed `/usr/bin/ssh`

## Context

Git, OpenSSH, and Bubblewrap were resolved only as root-owned `/usr/bin/git`, `/usr/bin/bwrap`, and `/usr/bin/ssh`. On NixOS, Guix-style profiles, Flatpak, and systems without Bubblewrap, every Git feature reported "Git unavailable" or an unsafe repository. ADR 0021 deferred this to "a separately reviewed trusted-executable/sandbox policy".

## Decision

### Executable discovery

`git::toolchain` resolves `git`, `bwrap`, and `ssh` only from this fixed, ordered list: `/usr/bin`, `/bin`, `/usr/local/bin`, `/run/current-system/sw/bin`, `/nix/var/nix/profiles/default/bin`, `$HOME/.nix-profile/bin` (only for an absolute, traversal-free launch `HOME`), and `/app/bin`. `PATH` and repository state are never consulted. `/usr/bin` stays first, so existing systems resolve exactly the historical executables.

Each candidate is canonicalized and the canonical path is what runs. The final file is held with `O_PATH | O_NOFOLLOW` and must be a regular, executable file with no group/other write bit, owned by root or the effective user, or on a read-only mount (Flatpak runtimes, read-only Nix stores). Every ancestor directory must be a non-symlink directory that is owned by the effective user without group/other write, owned by root without group/other write, a root-owned sticky directory (`/tmp`, a group-writable `/nix/store`), or on a read-only mount. Ancestor identities and the held inode are rechecked after opening. A dangling, unsafe, or missing candidate is skipped in favour of the next root.

Existing provenance checks remain and are applied to the discovered path: the HTTPS credential snapshot still captures Git by inode with the root-owner/one-link rule, and the SSH client is still captured by `capture_system_ssh_client` (root owner, one link). The shared parent check now accepts any root-owned sticky ancestor, not only world-writable sticky ones.

Bubblewrap views expose only `/usr`. When the canonical Git or SSH client lives under `/nix/store`, `/nix/store` is additionally bound read-only so the loader and libraries resolve. Executables elsewhere (for example a non-merged `/bin` or `/opt`) cannot be exposed and are treated as having no usable sandbox.

### Sandbox state and fallback

Bubblewrap is usable only when it is discovered, the Git toolchain can be exposed, and a probe (`bwrap … -- git --version` with the base view) succeeds; success is cached per canonical pair. The result is reported as `GitSandbox::{Bubblewrap, Unsandboxed}` in `GitStatus::sandbox()`, with `GitStatus::executable_source()` reporting the `GitExecutableSource` family. `inspect_git_environment()` returns the same facts without a Workspace.

With `Unsandboxed`, only the read-only status path runs. It executes Git directly with the identical cleared environment allowlist and status hardening (`GIT_CONFIG_NOSYSTEM`, `GIT_CONFIG_GLOBAL=/dev/null`, `HOME=/dev/null`, prompts/pagers/editors/askpass disabled, `GIT_OPTIONAL_LOCKS=0`, `core.hooksPath=/dev/null`, `core.fsmonitor=false`, untracked cache and external diff disabled), plus `protocol.allow=never`, an empty credential helper, and `GIT_NO_LAZY_FETCH=1`. `GIT_DIR` is an owner-private `mkdtemp` directory that mirrors the sandbox view: no `config`, a bounded copy of `HEAD`, descriptor-backed links for `refs`, index, packed refs, shallow, loose-object fan-out, and `pack`, and an empty private `objects/info` that masks alternates and commit graphs exactly as the tmpfs mount does. The directory is removed afterward without following links.

Every mutating or network operation still requires Bubblewrap and fails closed before any process starts.

### Degraded without a usable Bubblewrap

| Feature | Without Bubblewrap |
| --- | --- |
| Git status, branch, upstream divergence | Works (unsandboxed, read-only) |
| Git initialize, remote connect, identity setup | Unchanged (core file operations; `git config` parsing runs unsandboxed as before) |
| Commit plan/apply/recovery verification | `GitCommitPlanError::UnsupportedRepository`; desktop copy explains Bubblewrap is required |
| Sync inspect/fast-forward, push | `GitSyncError::SandboxUnavailable` |
| Clone discovery/fetch/validation/materialization | `GitClone*Error::SandboxUnavailable` (`Io` for index preparation) |
| SSH and HTTPS authenticated transport | Unavailable (needs sealed descriptor mounts) |

Additional degradations independent of Bubblewrap: HTTPS credential helpers are still accepted only from `/usr/lib/git-core` or `/usr/bin` (Nix-provided helpers report not configured/unsafe), and the root-owner/one-link rules for the credential snapshot and SSH client reject Nix store files that are hard-linked by store optimisation or owned by a single-user Nix installation.

## Consequences

- Existing Arch/Ubuntu behavior is unchanged; tests assert the historical paths and empty extra mounts.
- NixOS with Bubblewrap installed gets full functionality; Flatpak and namespace-less containers get read-only status with a visible `unsandboxed` state instead of "Git unavailable".
- Unsandboxed status loses mount/network/user namespace isolation and read-only enforcement of the object store; the config-free synthetic Git directory, transport ban, and hook/filter/fsmonitor disabling keep repository-controlled programs from running.
- Core exposes only closed values (`GitSandbox::as_str`: `bubblewrap`/`unsandboxed`; `GitExecutableSource::as_str`: `usrBin`, `nixSystemProfile`, …), never a path. Surfacing them in the desktop `GitStatusPayload` requires the renderer's exact-key validator (`isGitStatusPayload`) to be extended in the same change; until then desktop sync/clone/commit errors name Bubblewrap explicitly.
