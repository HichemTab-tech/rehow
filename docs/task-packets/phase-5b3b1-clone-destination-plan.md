# Task Packet: Phase 5B3B1 clone destination plan

- **Objective:** Bind a previously selected empty native destination and secret-free remote address into a replaceable one-use clone plan without adding network or write authority.
- **Required reasoning level:** medium.
- **Prerequisites:** Signed Phase 5B3A checkpoint `3a71e48`; accepted ADR 0023; existing native empty-folder capability and strict Phase 5B2 URL grammar.
- **Allowed files or subsystem:** `crates/rehow-core/src/{git,storage}.rs`, `apps/desktop/src-tauri/src/lib.rs`, ADR 0023, this packet/evidence, and progress bookkeeping. No React/MCP/dependency/recovery changes.
- **Relevant specifications and ADRs:** Product onboarding/Git requirements, architecture IPC/Git sections, threat model Git/credential boundaries, ADRs 0021–0023.
- **Exact required behavior:** Plan only against the retained empty target; recheck empty inventory, directory identity/capability, writability, ancestor/nested repository state, and unsafe Git marker; validate the exact secret-free URL grammar; generate a random token; retain the URL/native plan only in Rust; return token/transport/host only; invalidate an older plan when selection, cancellation, or local initialization changes the target.
- **Important states and edge cases:** No selected folder, invalid URL, folder changed after selection, nested repository, read-only/unsafe/unavailable folder, replaced plan, and lock/session failure.
- **Security constraints:** No renderer path or URL response, no confirmation yet, no filesystem write/process/network/credential access, custom redacted `Debug`, and no plan reuse across destination changes.
- **Tests to add or update:** Core valid/redacted/current/changed/nested/invalid-URL tests; native closed URL/path-free payload and stable error-copy test; focused Clippy.
- **Commands to run:** Cargo formatting; focused core/native tests; locked all-target/all-feature Clippy for both crates; bootstrap and whitespace checks.
- **Required reviews and artifacts:** Focused evidence. No extra reviewer for this read-only plan slice; the one Phase 5B3 Security Reviewer remains at the authority-bearing apply/fetch checkpoint.
- **Explicit non-goals:** Native confirmation, fetch/discovery, credentials, clone marker, blob reading, Workspace parsing/materialization, ref/config writes, cancellation, React UI, or MCP.
- **Definition of done:** Pathless native command and crate core plan meet the closed behavior, focused tests pass, and docs state that clone remains unavailable until apply/fetch/materialization exist.
- **Expected commit boundary:** Core destination snapshot/plan, native command/state/error payload, tests, and aligned ADR/evidence in one signed checkpoint.
- **Conditions requiring escalation:** Any path IPC, dependency, write/process/network authority, relaxed URL/folder rules, or need to expose secrets/identities.
