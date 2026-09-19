# Phase 5D3B2A fixed SSH proxy evidence

Date: 2026-09-19

## Implemented boundary

- `rehow-git-ssh` is a dedicated compiled binary with one public entry point and no product caller.
- It accepts only Git's upload-pack SSH argv, reconstructs canonical destination/port/command values, and exits 255 without its own diagnostic content for every other grammar.
- It invokes only trusted `/usr/bin/ssh` by argv under a cleared fixed environment and disables free-form configuration, prompts, password/keyboard authentication, private-key files, forwarding, proxies, local/known-host commands, control sockets, and TTY behavior.
- Fixed agent and known-host paths are placeholders for later descriptor mounts; no live SSH/network operation or product integration exists in this slice.

## Verification

- Three focused tests cover the exact accepted Git vector, adversarial option/destination/command/path/non-UTF-8 input, exact argv/environment policy, absence of a shell, and successful `ssh -G` parsing of the full fixed policy.
- A config-free `git ls-remote` diagnostic using `/usr/bin/false` confirmed Git 2.55.0 emits `-o SendEnv=GIT_PROTOCOL -p 2222 git@example.invalid "git-upload-pack '/team/rehow.git'"`; it performed no network connection.
- Invalid standalone execution exits 255 with zero output. Full locked Workspace tests pass: core 421 with 2 intentional network/performance ignores; 18 error-contract, 13 protocol, public-API/trybuild, 11 unknown-YAML, 24 desktop, and 1 MCP test.
- Strict core all-target/all-feature Clippy, Cargo formatting, `pnpm check`, contract drift, TypeScript, Biome, production Vite/Cargo builds, explicit proxy-binary build, bootstrap validation, and whitespace checks pass.

## Review

No extra agent is used for this still-unreachable component. The one Security Reviewer is deferred to the completed SSH invocation boundary so proxy grammar, host-key/agent descriptor mounts, sandbox visibility, cancellation, output redaction, and credential canaries are reviewed together.
