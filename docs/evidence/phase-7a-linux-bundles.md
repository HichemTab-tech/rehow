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

## Fedora clean-container package smoke

A fresh official Fedora 44 container was run ephemerally as root with the `.rpm` bind-mounted read-only. DNF installed Rehow with weak dependencies disabled, resolving 154 GTK/WebKit/runtime dependencies. Verification passed for:

- the installed RPM record and all five executable entries;
- complete desktop binary dynamic linkage;
- desktop `Exec` and icon fields;
- standalone MCP refusal without a configured Workspace;
- DNF removal of Rehow-owned binaries; and
- byte-identical preservation of an unrelated Workspace sentinel.

The container was removed automatically. The Fedora 44 base image remains in Docker's ordinary local image cache. No host package was installed. As with Debian, a headless container does not prove graphical application-menu launch.

## Oldest supported binary baseline

All five packaged executables were inspected with `readelf --version-info`. The desktop, both Git helpers, and MCP server require at most `GLIBC_2.39`; the approval helper requires `GLIBC_2.34`. A package install/link/remove journey passes on Ubuntu 24.04 with glibc 2.39.

Phase 7 therefore sets Ubuntu 24.04 / glibc 2.39 as the oldest supported binary baseline for the current artifacts. Debian 12 and Ubuntu 22.04 are not advertised for these builds. `scripts/check-linux-abi-floor.sh` is invoked for every executable extracted from both package formats and fails if a future build raises the floor above `GLIBC_2.39`.

## Packaged application-menu launch

The `.deb` was extracted to an ignored temporary tree and exposed through temporary `PATH`/`XDG_DATA_DIRS`. On the current Wayland session, `gtk-launch Rehow` resolved the packaged `Rehow.desktop`, started the packaged `/usr/bin/rehow` payload, and kept it alive through the three-second smoke window with empty stdout/stderr when the host-specific `WEBKIT_DISABLE_DMABUF_RENDERER=1` workaround was scoped to that invocation. The exact process was terminated and the temporary tree deleted. Follow-up ADR 0060 moves the cheaper confirmed `__NV_DISABLE_EXPLICIT_SYNC=1` correction into Linux Wayland startup; the rebuilt binary passes without any launcher-supplied variable.

This verifies the packaged desktop ID, `Exec`, runtime lookup, and icon-data layout through the application-menu launch mechanism on the current host. A second clean graphical compositor remains part of the final runtime matrix.

## Open release gates

- Clean-machine install/menu-launch/uninstall testing on the chosen oldest supported baseline.
- Wayland/X11 runtime matrix and WebKitGTK diagnostics.
- Release version, production application identifier, project license, artifact signing, and publication decisions.
- Full install-to-recovery journey, visual/accessibility review, and final security/docs audit.
