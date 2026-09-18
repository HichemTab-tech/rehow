# Task Packet: Delete, trash, and committed cleanup

- **Objective:** Complete journal execution for deletion/trash and quarantine verified displaced temps only after durable commit.
- **Required reasoning level:** xhigh security/recovery boundary.
- **Prerequisites:** ADR 0009 and signed checkpoint `1dd646e`.
- **Allowed files or subsystem:** Linux confined storage helpers, `storage::recovery`, this packet, and progress.
- **Relevant specifications and ADRs:** content model lifecycle rules, threat model destructive operations, ADRs 0003/0009.
- **Exact required behavior:** atomically move an expected active file to its exclusive temp for ordinary deletion or to its exact `.rehow/trash/<operation>/<active-path>` destination; verify bytes/mode and absence; journal every durable step; after committed generation, atomically move ordinary/update before-images with no-replace from their same-directory temp to that same exact per-operation trash path; replay idempotently from either side of every rename.
- **Important states and edge cases:** active old/absent/neither; temp or trash missing/exact/conflicting; pre-existing symlink/hardlink/special destination; interrupted move; committed cleanup retry; unsafe `.rehow` hierarchy.
- **Security constraints:** descriptor-relative component traversal, no-follow, owned mode-0700 trash directories, no replace, no recursive deletion, no destructive temp cleanup without atomic identity binding, regular single-link verification, and preservation on ambiguity.
- **Tests to add or update:** ordinary delete, post-move replay, trash move, conflicting trash destination, committed cleanup retry, and hostile destination hierarchy.
- **Commands to run:** format; focused recovery tests; locked full core tests; core Clippy; contract/bootstrap/diff checks.
- **Required reviews and artifacts:** one combined Security Reviewer after this gate.
- **Explicit non-goals:** portable migration backups, retention/pruning, adapter exposure, Git, MCP.
- **Definition of done:** deletion/trash commit or resume without overwrite; ambiguity persists conflict; committed before-images are removed from active directories without unlinking and retained at an exact operation-bound trash path.
- **Expected commit boundary:** delete/trash executor, cleanup, tests, and progress.
- **Conditions requiring escalation:** safe no-replace relocation or directory durability is unavailable, or cleanup identity cannot be bounded safely.
