# Phase 6E1B2 MCP approval launcher and plan evidence

Date: 2026-09-20

The server now captures only its fixed sibling after exact build-digest and stable safe-metadata checks, copies it into a private non-writable sealed executable memfd, and retains only that snapshot's `O_PATH` descriptor. Launch uses the immutable snapshot, private fixed pipes, a minimal bounded graphical environment, parent-death/no-new-privileges controls, a new process session, `/dev/null` standard streams, and `execveat(AT_EMPTY_PATH)`. Approval requires exactly byte `1`, EOF, and zero exit before the monotonic deadline; cancellation, expiry, malformed/trailing output, nonzero exit, and every transport failure deny and kill/reap the process group.

Temporary compiled protocol fixtures prove neither path replacement nor an in-place same-inode overwrite can change the retained executable, exact positive/negative behavior, trailing-output/nonzero rejection, and prompt expiry/active cancellation of a ten-second child. A deterministic barrier test proves a cancellation that returns success wins before approval finalization. Unit tests cover digest/metadata/symlink/hardlink/set-ID rejection, environment and descriptor exclusion, random exact-bound single-use tokens, fixed operation-kind mapping, wrong-binding consumption, pending cancellation, maximum lifetime, and the 64-plan cap. The existing real-helper tests continue to prove hostile-`PATH` non-execution and headless refusal. The existing protocol schema test proves no MCP tool was added.

No Workspace, Guide, Category, Attachment, Git, tested, broad, destructive, or public approval operation consumes this infrastructure yet.

Independent Security Reviewer result: **PASS** after two high findings were corrected. The source helper is now copied into a sealed immutable executable snapshot before retention, with a same-inode-overwrite regression. Approval finalization now checks cancellation and removes the exact active plan under one lock, with a deterministic completion-versus-cancel regression. The correction also exposed and fixed a low-numbered `/dev/null` descriptor collision in the fork child.
