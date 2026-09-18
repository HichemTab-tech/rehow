# Task Packet: Atomic Workspace target primitives

- **Objective:** Add the descriptor-confined Linux primitives required to create and replace one managed Workspace file without an overwrite race.
- **Required reasoning level:** xhigh security/filesystem boundary.
- **Prerequisites:** ADR 0009; recovery journal/store checkpoints `e810658` and `f26a868`.
- **Allowed files or subsystem:** `rehow-core::storage` Linux capability helpers, `storage::recovery`, this packet, and verified progress.
- **Relevant specifications and ADRs:** architecture Atomic writes, threat model concurrent/partial writes, ADRs 0003/0009.
- **Exact required behavior:** resolve only schema-managed paths beneath the retained root; create and fsync exclusive same-directory temps; install absent targets with `RENAME_NOREPLACE`; replace existing targets with `RENAME_EXCHANGE`; re-read exact bytes/mode and retain the displaced inode until directory durability is proven.
- **Important states and edge cases:** target absent/regular/symlink/hardlink/special; temp absent/exact/conflicting; expected revision match/mismatch; unsupported rename primitive; failure before/after rename and directory sync.
- **Security constraints:** component-only `openat`, `O_NOFOLLOW`, regular single-link files, no plain-rename fallback, no caller-selected absolute path, no shell, and no deletion of unverified evidence.
- **Tests to add or update:** atomic create/update, stale-revision conflict, symlink/hardlink/temp collision refusal, and injected failure aftermath.
- **Commands to run:** format; focused storage tests; core Clippy; contract/diff checks.
- **Required reviews and artifacts:** combine Security Reviewer with the completed recovery executor checkpoint.
- **Explicit non-goals:** UI/Tauri mutation exposure, deletion/trash, portable backup, multi-target recovery orchestration, Git, MCP.
- **Definition of done:** the private primitive either durably installs the intended bytes or returns a typed non-success while preserving enough evidence to classify the target.
- **Expected commit boundary:** confined atomic target primitive and adversarial tests.
- **Conditions requiring escalation:** rename primitives unavailable on the test filesystem, target identity cannot be proven, or implementation would require a plain overwrite fallback.
