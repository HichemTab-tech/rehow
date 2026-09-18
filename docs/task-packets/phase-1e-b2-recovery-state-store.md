# Task Packet: Durable recovery state store

- **Objective:** Persist verified recovery objects and whole journal generations outside the Workspace, and serialize cooperating writers.
- **Required reasoning level:** xhigh security/filesystem boundary.
- **Prerequisites:** signed journal-codec checkpoint `e810658`, ADRs 0003/0009, Linux descriptor helpers and BLAKE3.
- **Allowed files or subsystem:** `rehow-core::storage::recovery::store`; recovery model visibility/helpers; this packet; progress and compile-fail snapshots only if private types deterministically alter diagnostics.
- **Relevant specifications and ADRs:** architecture Atomic writes, threat model concurrent/partial writes, ADRs 0003/0009.
- **Exact required behavior:** beneath an explicitly supplied XDG state-home descriptor, create/open `rehow/workspaces/<workspace-id>-<root-hash>/{objects,journals}`; every created directory is 0700 and file 0600. Store BLAKE3-addressed immutable objects with write/fsync/re-read verification. Install generation 1 journals only when absent; replace only the exact next generation for the same immutable operation, via same-directory exclusive temp, file sync, atomic rename, directory sync, and re-read/decode. Acquire a nonblocking `flock` on `<capability-hash>.lock` in secured runtime `rehow/` when supplied, otherwise secured state `rehow/locks/`.
- **Important states and edge cases:** first/existing object; corrupt/wrong-mode/hardlinked/symlink/special entries; absent/current/stale/skipped journal generation; torn/stale temp; lock held/free; unsafe/missing base; I/O and durability failure stages.
- **Security constraints:** descriptor-relative `openat`/`mkdirat` with `O_NOFOLLOW`; current-euid ownership; exact managed modes; regular files with link count one; no shell/network/logging; content-free errors; input paths are application-owned XDG bases, never renderer/MCP arguments; bounds precede allocation.
- **Tests to add or update:** secure layout/modes, symlink and mode substitution, idempotent/corrupt/hardlink object, journal generation CAS and stale temp, exact decode after install, lock contention/release, and injected failures around write/file-sync/rename/directory-sync with aftermath checks.
- **Commands to run:** format; focused store tests; locked full core tests; core Clippy warnings denied; bootstrap/diff checks.
- **Required reviews and artifacts:** no extra agent during implementation; combined Security Reviewer after B3.
- **Explicit non-goals:** Workspace temp/exchange, mutation planning, startup target classification/execution, retention/pruning, UI/MCP, portable backups.
- **Definition of done:** objects and journal generations are durable/verified in isolated tests, unsafe state entries fail closed, lock contention is deterministic, full core gates pass, and no Workspace write exists.
- **Expected commit boundary:** private state store/lock + fault tests + packet.
- **Conditions requiring escalation:** equivalent atomic/durability primitives are unavailable, ownership/mode cannot be proven, a new dependency is needed, or an unsafe state entry would need automatic deletion.
