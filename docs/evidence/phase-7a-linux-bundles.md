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

## Current-host runtime smoke

The release binary was launched with isolated repository-local XDG config/cache/data/state directories:

- default Wayland: exited with `Gdk-Message: Error 71 (Protocol error) dispatching to Wayland display`;
- Wayland with `WEBKIT_DISABLE_DMABUF_RENDERER=1`: remained alive for the five-second smoke window with no stderr;
- X11: remained alive for the five-second smoke window and logged two GBM allocation warnings.

The exact temporary smoke directories were deleted afterward. No package was installed. This proves the binary reaches a live desktop window path on the current host, but does not satisfy the clean-machine or multi-compositor gate.

## Debian clean-container package smoke

An existing Debian 13 (Trixie) development-container image was run ephemerally as root with the `.deb` bind-mounted read-only. No host package or tool was installed.

- `apt-get install --no-install-recommends /tmp/rehow.deb` resolved the declared `libwebkit2gtk-4.1-0` and `libgtk-3-0` dependencies and reached `Setting up rehow (0.0.0)` successfully.
- A harness quoting error occurred only in the first post-install `dpkg-query` assertion; it was not a package failure, and the container was removed automatically.
- A separate network-disabled clean container unpacked the package, verified all five executable entries and the desktop entry, removed `rehow`, verified its binaries were gone, and verified an unrelated Workspace sentinel remained byte-identical.

This establishes Debian 13 installation/dependency resolution and package-owned uninstall behavior. It does not establish the oldest supported Debian baseline or graphical application-menu launch because the clean container has no compositor/session bus.

## Open release gates

- Clean-machine install/menu-launch/uninstall testing on the chosen oldest supported baseline.
- Wayland/X11 runtime matrix and WebKitGTK diagnostics.
- Release version, production application identifier, project license, artifact signing, and publication decisions.
- Full install-to-recovery journey, visual/accessibility review, and final security/docs audit.
