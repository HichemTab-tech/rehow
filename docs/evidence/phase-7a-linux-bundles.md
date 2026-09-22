# Phase 7A Linux bundle evidence

Date: 2026-09-22

## Implemented

- Tauri builds native `.deb` and `.rpm` packages with the `rehow` main binary name.
- Both packages install Rehow, the two fixed Git helpers, `rehow-mcp`, and `rehow-mcp-approve` under `/usr/bin`.
- Both packages install a non-terminal `Utility` desktop entry and 32/128/256/512 px original Rehow icons.
- The release preparation hook embeds the exact packaged approval-helper BLAKE3 digest into the MCP server.
- The smoke test extracts packages into a temporary directory; it does not install them or require elevated privileges.

## Verification

Passed on the existing machine toolchain without installing or switching Node or pnpm:

- `pnpm tauri:build`
- `scripts/test-linux-bundles.sh`
- `pnpm check`
- `git diff --check`

Generated artifacts:

- `target/release/bundle/deb/Rehow_0.0.0_amd64.deb`
- `target/release/bundle/rpm/Rehow-0.0.0-1.x86_64.rpm`

The extraction test verifies executable mode/link count, desktop-entry fields, icon dimensions, approval-helper digest binding, and empty-configuration MCP refusal for both formats.

The 512 px source and 32 px rendered asset were visually inspected. The open reference notebook and rust return/bookmark mark remain recognizable at launcher size without text, glow, or gradient styling.

## Open release gates

- Clean-machine install/menu-launch/uninstall testing on the chosen oldest supported baseline.
- Wayland/X11 runtime matrix and WebKitGTK diagnostics.
- Release version, production application identifier, project license, artifact signing, and publication decisions.
- Full install-to-recovery journey, visual/accessibility review, and final security/docs audit.
