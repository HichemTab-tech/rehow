# Task Packet: Application Guide update proof

- **Objective:** Connect existing Guide title, summary, body, and stale-state edits to the accepted recovery executor through a crate-private application boundary.
- **Required reasoning level:** high; filesystem/revision boundary.
- **Prerequisites:** signed recovery closure `dbf4230`, ADRs 0003/0009/0014.
- **Allowed files or subsystem:** `rehow-core` application, domain mutation visibility, storage/recovery facade, focused tests, this packet, and progress.
- **Relevant specifications and ADRs:** architecture application/core boundary, content-model revisions/status rules, threat model, ADRs 0003/0009/0014.
- **Exact required behavior:** locate an already loaded Guide by stable ID; require its expected `rev:v1`; acquire the cooperating writer lock; descriptor-re-read and rebind current bytes; plan only a typed closed edit; preserve unknown metadata; treat exact edits as no-op; store exact old/new objects; execute one update journal; refresh the retained Workspace view after commit.
- **Important states and edge cases:** missing/duplicate Guide, stale view revision, external edit before lock, unsafe round trip, invalid value/body, tested-to-stale transition, lock busy, interrupted journal, committed write with refresh failure.
- **Security constraints:** no path input, no raw YAML input, no tested transition, no adapter command, no symlink/hardlink/special file, no shell/process/network, no overwrite on stale revision, and no content in errors.
- **Tests to add or update:** successful body/title update, no-op, stale revision/external edit, tested edit becomes stale, unknown metadata preservation, hostile target, and reload behavior using only temporary Workspaces/state.
- **Commands to run:** format; focused application/recovery tests; locked full core tests; core Clippy; contract/bootstrap/diff checks.
- **Required reviews and artifacts:** routine implementation uses no extra agent; a later adapter-exposure checkpoint receives the next combined security review.
- **Explicit non-goals:** Guide creation/move/delete, Category operations, Workspace initialization, Attachments, UI/Tauri/MCP mutation exposure, tested approval, Git.
- **Definition of done:** the private application boundary updates only the expected Guide through ADR 0009, refreshes its Workspace view, and all specified refusal/no-op cases are proven.
- **Expected commit boundary:** application Guide update proof, tests, and progress.
- **Conditions requiring escalation:** the accepted journal cannot express the update, capability identity cannot be derived without new authority, or commit state cannot be distinguished from refresh failure.
