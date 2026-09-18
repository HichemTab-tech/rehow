# ADR 0009: Recovery Journal and Backup State Machine

- Status: Accepted
- Date: 2026-09-05

## Context

Atomic rename is per-file, stale checks cannot universally lock external editors, and a Workspace-controlled `.rehow` directory is not trustworthy enough for the only before-image/journal. Multi-file moves, Attachment changes, migrations, and future Git materialization require deterministic recovery after failure at any durability boundary.

## Decision

### State locations and identity

- Cross-process locks use `$XDG_RUNTIME_DIR/rehow/<capability-hash>.lock` when available, otherwise a mode-0700 lock directory under XDG state.
- Journals and content-addressed before/new objects use `$XDG_STATE_HOME/rehow/workspaces/<workspace-id>-<root-hash>/`. Directories are `0700`, files `0600`, opened no-follow, and rejected if ownership/type/mode is unsafe.
- The retained capability identity includes canonical root, root device/inode at open, Workspace ID, and random open epoch. Journals bind the stable ID/root hash and record device/inode; a moved/replaced root requires explicit recovery remapping.
- `.rehow/backups/` contains portable migration/bulk backups but is untrusted until its inventory/objects revalidate. It is excluded from its own backup inventory.

### Journal format

A bounded canonical CBOR record uses RFC 8949 deterministic encoding and contains `journal_version`, monotonic `generation`, UUIDv7 operation ID, operation kind, Workspace ID/root hash/device/inode, created time, accepted plan/effect-set hash, ordered targets, current stage, and completion marker. Integers use their shortest encoding, maps use deterministic bytewise key ordering, timestamps are RFC 3339 UTC text, and indefinite-length items, floats, tags, duplicate keys, and unknown required fields are rejected. Each target records normalized relative path, expected state (`absent` or old `rev:v1`), exact old-object BLAKE3 and mode when present, intended new state/object hash/mode or deletion/trash destination, same-directory temp name, and step state (`prepared | swapped | directory_synced | complete`). Hashes are verified before use; paths resolve again beneath the retained root. Unknown, hostile, oversized, or corrupt journals never execute automatically and yield `RECOVERY_REQUIRED`. Fixed byte vectors freeze every schema version.

Journal v1 uses only definite maps with ascending unsigned-integer keys, which makes encoded-key byte order canonical without text-key ambiguity. The top map has keys `0..12` for version, generation, operation ID, operation kind, Workspace ID, 32-byte root hash, root device, root inode, created timestamp, 32-byte plan/effect hash, ordered targets, stage, and completion. A target map has keys `0..4` for relative path, expected state, intended state, temp component, and step. State maps start with key `0` as a closed discriminant; present-object members follow in ascending keys. The complete closed discriminants and byte vector live with `rehow-core::storage::recovery` and are versioned with this ADR. A journal is at most 4 MiB, has at most 1,024 targets, each managed path is at most 1,024 bytes, and each temp component is at most 255 bytes. Generation starts at one. `completion=true` is valid only with committed stage and all targets complete; committed stage requires that same state. Duplicate target paths, noncanonical paths/revisions/IDs/timestamps, special mode bits, trailing bytes, non-shortest integers, reordered/extra/missing keys, and every unsupported CBOR major/additional form are corrupt.

Closed unsigned discriminants are: operation `create=0`, `update=1`, `delete=2`, `multi_file=3`, `recovery=4`, `migration=5`, `git_materialize=6`; journal stage `prepared=0`, `applying=1`, `committed=2`, `conflict=3`; target step `prepared=0`, `swapped=1`, `directory_synced=2`, `complete=3`; expected state `absent=0`, `existing=1`; intended state `absent=0`, `object=1`, `trash=2`. Existing state keys `1..3` are revision text, 32-byte old-object hash, and mode. Object state keys `1..2` are 32-byte new-object hash and mode. Trash adds key `3`, whose destination is exactly `.rehow/trash/<operation-id>/<active-managed-path>`. Active managed paths use only the schema-v1 manifest, Category/Guide, or Attachment grammars; modes are `0..0777` with no special bits.

Every stage update serializes the entire next generation to a same-directory mode-`0600` temporary file, fsyncs that file, atomically replaces the journal path, and fsyncs the journal directory. It never appends or rewrites a live journal in place. A torn temporary file is ignored when the prior journal remains valid; a corrupt installed generation yields read-only `RECOVERY_REQUIRED`. Tests cover fixed encodings, truncated bytes at every boundary, corrupt generations, stale temporaries, and failure before and after each write, file fsync, replace, and directory fsync.

### Preparation and write order

1. Acquire the cooperating-writer lock and revalidate capability, plan, and effect set.
2. Read every target descriptor-relatively; require regular file and link count 1; persist exact old/new objects in XDG state.
3. Write/fsync the prepared journal and fsync its directory.
4. Create/validate/fsync same-directory target temp files and advance the journal with the whole-generation replacement protocol above.
5. Recheck expected revision/identity. Creation uses `renameat2(RENAME_NOREPLACE)`. Existing replacement uses `renameat2(RENAME_EXCHANGE)` where supported, retaining displaced bytes at the temp name.
6. Verify the displaced old revision. A mismatch preserves both versions, attempts only an atomic exchange-back while identities remain safe, records conflict evidence, and returns `RECOVERY_REQUIRED`—never success. An editor holding an old inode may still write it; universal external CAS is not claimed.
7. Fsync the target directory, advance the target step with a whole journal generation, repeat in order, then persist the committed generation with the same file-and-directory fsync sequence.
8. Emit invalidation only after durable commit. After committed classification, atomically quarantine verified update and ordinary-delete before-images with a no-replace rename from the same-directory temp to the exact `.rehow/trash/<operation-id>/<active-managed-path>` destination. Never unlink an ambiguous path; interruption on either side of the quarantine rename is replayable. Retained before-images follow the visible retention policy.

If no-replace, exchange, file-fsync, or directory-fsync semantics are unavailable, the core reports degraded filesystem capability and **must refuse every mutation class that depends on the missing primitive**. There is no plain-rename or best-effort fallback. An alternative is permitted only after a separate accepted ADR and Security Reviewer approval prove equivalent collision, rollback, and durability behavior. Cross-filesystem moves are never implicit.

### Startup recovery

With the lock held, validate journal/objects and classify every target against recorded old/new revisions:

- all old: no target step applied; safely cancel or resume only the previously accepted plan;
- all new: finish missing directory/journal sync and mark committed;
- mix of old/new with no `neither`: idempotently complete the already accepted operation in order, then report recovery;
- any `neither`, missing/corrupt object, root mismatch, hostile journal, or unsafe path: open read-only with an exact recovery plan; never guess or overwrite.

Recovery actions create a new journal. Tests inject failure before/after every object, journal, temp, rename/exchange, directory, and commit sync and inspect exact aftermath.

### Portable backups and restore

Before migration/broad transforms, copy/reflink every affected canonical regular file/Attachment to `.rehow/backups/<timestamp>-<operation-id>/objects/` without following links. The inventory records version, operation, source schema, normalized path, mode, size, digest, and capability identity for every entry. Write/fsync all objects, inventory, completion marker, and directories; re-read every digest before mutation.

Default restore requires a user-selected new empty target capability and reconstructs without overwrite. In-place restore needs a separate current inventory/effect plan, first backs up current content, and uses this journal. Migration backups are never silently pruned. Ordinary-save before-images may follow a visible count/age policy only after a committed state and when no pending recovery depends on them.

## Consequences

- Exact before-images exist; hashes are not mistaken for backups.
- The common external-write race preserves displaced bytes on supported Linux filesystems but cannot control an editor holding an old inode.
- XDG state is required for write mode. Losing it never loses canonical content, but may remove convenience recovery after already committed ordinary edits.
- Significant fault-injection and hostile-journal work is required before Phase 1 storage passes.
