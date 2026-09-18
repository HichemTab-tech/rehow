# Task Packet: Active Category creation

- **Objective:** Let a user create an active Category in the open writable Workspace and see it immediately.
- **Required reasoning level:** high; descriptor-confined creation and UI mutation.
- **Prerequisites:** canonical Category emitter, retained Workspace capability, recovery store/writer lock, signed Workspace initialization checkpoint `b2dac91`.
- **Allowed files or subsystem:** core protocol/application/storage Category-create path, desktop Tauri command/session, Categories UI/styles/tests, this packet, and progress.
- **Relevant specifications and ADRs:** Category sections of the content/product specs, ADR 0009, ADR 0014, design direction, threat model.
- **Exact required behavior:** accept bounded name and optional summary only; generate ID/time/order/path internally; create a private Category directory without replacement; journal canonical `category.yaml` at `0600`; refresh Workspace; return the refreshed payload; focus and announce the result.
- **Important states and edge cases:** first and later Category order, Unicode/non-ASCII name slug fallback, long/invalid input, directory collision, external entry, read-only Workspace, busy writer, committed-refresh failure, empty Category directory after interrupted creation.
- **Security constraints:** no renderer path/ID/order/time/YAML/operation authority; no overwrite/adoption; one fixed descriptor filename; ASCII bounded slug plus UUID suffix; content-free structured errors; Git/MCP unchanged.
- **Tests to add or update:** canonical create/open, order selection/overflow, slug/suffix, invalid input no-write, collision/external entry refusal, modes, refreshed payload, structured IPC errors, accessible form semantics.
- **Commands to run:** focused checks while iterating; one final format, full test, Clippy, contracts, production build, bootstrap, and native Tauri build batch.
- **Explicit non-goals:** Category edit/reorder/archive/remove, default-Category manifest update, Guide creation, directory tidy, Git, MCP mutation.
- **Definition of done:** a writable open Workspace can create and display an active Category without accepting filesystem authority or overwriting content; tests/gates pass and progress records interruption behavior.
- **Expected commit boundary:** core/Tauri/UI Category create slice, then progress.
- **Conditions requiring escalation:** path/ordering authority cannot stay native, collision requires overwrite, or committed state is ambiguous.
