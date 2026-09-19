# Phase 5B2 Git Remote Connection Evidence

Date: 2026-09-19

## Accepted boundary

- The desktop can add one named remote to the currently open, writable, direct Git-backed Workspace.
- Planning accepts only a bounded remote name and secret-free HTTPS/SSH/SCP address. Apply accepts only the native-held plan token and native confirmation.
- The operation edits Git config only. It performs no DNS, socket, authentication, credential-helper, SSH, fetch, clone, branch, index, ref, or Workspace-content operation.
- Plan responses expose only the token, remote name, transport, and host. The full URL appears only in the user-entered field and effect-bound native confirmation; it does not appear in responses, structured errors, `Debug`, announcements, or logs.

## Storage and recovery evidence

- The retained Git directory must be owner-controlled and not group/world writable. `config` and optional `config.lock` must be owner-owned, single-link regular files with a supported safe mode and a 1 MiB bound; symlinks, hardlinks, special files, instability, and oversize inputs fail closed.
- Root-owned `/usr/bin/git` parses a memory-file snapshot with a cleared environment, fixed safe options, `--file`, `--no-includes`, NUL framing, and name-only output. Existing values—including credential-bearing legacy values—do not cross the process-output parser or renderer boundary.
- Candidate bytes retain the entire original config byte-for-byte and append one canonical quoted remote section. Original and candidate both parse before planning.
- The plan binds the launch capability, Workspace ID, manifest revision, exact config before-image/mode and domain-separated revision, candidate bytes/hash, remote identity, and a random single-use token.
- Apply publishes a fully written and fsynced `config.lock` no-replace, atomically exchanges it with `config`, validates the displaced exact before-image and inode orientation, fsyncs the directory, then moves the current lock no-replace to a random inert `rehow-config-before-v1-*` backup and fsyncs again. It never unlinks or performs a name-based rollback.
- Fault tests stop immediately after lock publication and immediately after exchange. Reopening and repeating the exact same request recognizes both byte orientations, requires confirmation again, completes once, and leaves no lock or duplicate section.
- Deterministic pre-exchange and pre-quarantine races prove that replaced entries are retained under active or inert backup names and the operation reports conflict; no unrelated inode is deleted. A stale config preserves the external bytes; an unrelated lock is preserved and reported as recovery-required. Malformed config, config hardlinks, and lock symlinks are rejected.

## Focused verification

- `cargo test -p rehow-core git::tests --locked`: passed, including read-only Git, initialization, remote grammar, stale/unrelated state, byte preservation, and interruption recovery.
- `cargo test -p rehow-desktop --locked`: passed; closed URL-free output payload and stable error mapping are covered.
- `pnpm --filter @rehow/desktop test`: 32 tests passed; closed plan/result guards and input normalization are covered.
- Focused all-target/all-feature Clippy with warnings denied and root `pnpm check` passed before the final gate.

## Interface review

The project-pinned Web Interface Guidelines skill fetched the current upstream rules on 2026-09-19 and reviewed `App.tsx` and `index.css`. The review corrected input spellcheck metadata, placeholder ellipsis, heading casing/wrapping, panel overscroll containment, and close-target size. Final static result: no finding. The interaction is a bounded status-bar disclosure and does not change the accepted shell geometry, so this checkpoint did not require a new representative screenshot set.

## Final gate and independent review

The single Security Reviewer/xhigh initially blocked on a name-bound rollback/unlink race and incomplete adversarial coverage. The implementation removed rollback and unlink, added inode-orientation checks plus no-replace quarantine, and added deterministic race, limit, special-file, empty-section, confirmation, semantic-output, and durability-state tests. The same reviewer passed the correction with no remaining blocker, high, or medium finding. Low residual: retained before-image backups can accumulate and preserve legacy secret-bearing config bytes; ADR 0022 documents the owner-mode-matched in-`.git` retention and defers pruning until it can be identity-safe.

Final gate passed:

- `pnpm check`
- `pnpm test`: 32 Vitest tests; 319 normal core tests passed with 1 intentional production-performance ignore; 18 error-contract, 13 protocol, 1 public-API/trybuild, 11 unknown-YAML, 19 desktop Rust, and 1 MCP test passed.
- `cargo fmt --all --check`
- Locked full-Workspace, all-target, all-feature Clippy with warnings denied.
- `pnpm build`
- `pnpm tauri:build`: optimized native binary at `target/release/rehow-desktop`; the existing `.app` suffix warning remains informational.
- Bootstrap validation: 17 required files, 22 ADRs, and 8 pinned agents.
- Contract drift and whitespace checks passed.
