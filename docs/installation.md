# Installation and Desktop Integration

Status: local `.deb`/`.rpm`/Arch release candidates implemented and smoke-tested; clean graphical release qualification remains open
Last updated: 2026-09-24

## Supported baseline

Rehow is Linux-first and targets a normal resizable, decorated desktop window on Wayland and X11. Current x86_64 development packages require glibc 2.39 or newer and use the system WebKitGTK 4.1 webview. Ubuntu 24.04 is the oldest verified binary baseline; Debian 13 and Fedora 44 package journeys also pass. Debian 12 and Ubuntu 22.04 are not supported by the current artifacts.

Initial package priority:

1. Native `.deb`, `.rpm`, and Arch packages with an enforced glibc 2.39 ABI ceiling and install tests on the documented families.
2. AppImage only after independent Wayland/XWayland and upgrade/uninstall testing.

Omarchy uses Arch Linux and installs the native `.pkg.tar.zst`; it does not require an Omarchy-specific build. Rehow does not modify Omarchy-managed files. The `.deb` and `.rpm` must not be force-installed or converted for Omarchy. Release automation and the platform policy are documented in `docs/release.md`.

Relevant Tauri references: [Linux prerequisites](https://v2.tauri.app/start/prerequisites/), [AppImage distribution](https://v2.tauri.app/distribute/appimage/), and [Linux graphics diagnostics](https://v2.tauri.app/develop/debug/linux-graphics/).

The development `.deb` and `.rpm` have completed dependency-resolving install and package-owned uninstall checks in ephemeral Debian 13 and Fedora 44 containers. The Arch artifact passes native `makepkg` construction, exact payload comparison, metadata checks, and a disposable `pacman` install/remove journey. This is compatibility evidence, not the minimum supported baseline; graphical menu launch and older baselines remain release gates.

## Local release-candidate packages

Build the Tauri packages, then the native Arch package, with the repository's active Node, pnpm, and Rust toolchain:

```sh
pnpm tauri:build
scripts/build-arch-package.sh
```

The Arch packaging step requires Docker. It runs `makepkg` as an unprivileged user in a digest-pinned Arch container with networking disabled; it does not install packages on the host.

This produces locally built release-candidate artifacts under:

- `target/release/bundle/deb/Rehow_0.1.0_amd64.deb`
- `target/release/bundle/rpm/Rehow-0.1.0-1.x86_64.rpm`
- `target/release/bundle/arch/rehow-0.1.0-1-x86_64.pkg.tar.zst`

The bundle hook builds the desktop binary, fixed Git helpers, standalone MCP server, and graphical MCP approval helper. It embeds the exact approval-helper BLAKE3 digest into `rehow-mcp`; the package smoke test verifies that binding and the one-link executable layout:

```sh
scripts/test-linux-bundles.sh
```

Version `0.1.0` and application identifier `me.hichemtab-tech.rehow` are the first-release defaults. Rehow is licensed under MIT. Signing and publication still require explicit authorization.

## Desktop application installation

Packages install:

- versioned Rehow binary and runtime resources;
- application icon at 32, 128, 256, and 512 px hicolor sizes;
- `rehow.desktop` with `Type=Application`, `Name=Rehow`, `Exec=rehow`, `Icon=rehow`, appropriate Knowledge/Utility categories, and no terminal;
- MIT license and package metadata.

Install a locally built package using the system package manager, which may request the user's normal authorization:

```sh
sudo apt install ./target/release/bundle/deb/Rehow_0.1.0_amd64.deb
```

or:

```sh
sudo dnf install ./target/release/bundle/rpm/Rehow-0.1.0-1.x86_64.rpm
```

On Arch Linux or Omarchy:

```sh
sudo pacman -U ./target/release/bundle/arch/rehow-0.1.0-1-x86_64.pkg.tar.zst
```

For the published build, download `rehow-0.1.0-1-x86_64.pkg.tar.zst` from the [v0.1.0 release](https://github.com/HichemTab-tech/rehow/releases/tag/v0.1.0), then run:

```sh
sudo pacman -U ~/Downloads/rehow-0.1.0-1-x86_64.pkg.tar.zst
```

No install is performed by the build or smoke-test scripts.

User Workspaces are never placed inside the application installation directory. Default preferences/cache/state follow XDG locations. Uninstall removes application-owned binaries/launchers/cache only; it preserves every user-selected Workspace and asks before removing local recovery state.

Prefer user-level installation when the package format supports it without weakening desktop integration. System package installation may require the user's normal package manager authorization; Rehow itself never elevates privileges.

## Runtime dependencies

Package metadata must declare the exact WebKitGTK 4.1, GTK, SSL, and other runtime libraries produced by the Tauri bundler/build baseline. Linux Git operations require root-owned `/usr/bin/git` and `/usr/bin/bwrap`; authenticated SSH clone, sync, and push additionally require root-owned `/usr/bin/ssh` plus a safe launch-inherited agent and trusted `known_hosts`. Missing tools or trust keep Rehow content usable and produce closed Git-unavailable guidance. Development-only packages are in `docs/development.md`. Do not tell users to install a permanently running Node server for the desktop application.

All three native packages place fixed `rehow-git-ssh` and `rehow-git-credential` binaries beside the application executable as one-link executables. Rehow does not install, enable, or alter a user's credential helper; it may use only an already configured helper that passes the documented provenance gate.

Authenticated HTTPS supports a trusted self-contained system helper or the exact `gh auth git-credential` configuration. The `gh` path requires its ordinary private `hosts.yml`; keyring/libsecret mediation is not yet supported. Rehow never asks for or stores a token.

## Wayland behavior

- Do not depend on global window coordinates, programmatic placement, always-on-top, shadows, blur, or compositor effects; support differs on Wayland.
- Preserve useful minimum/narrow layouts and allow the compositor to manage the window normally.
- Test GNOME Wayland, KDE Wayland, wlroots/Sway/Hyprland, and X11 with Intel/AMD/NVIDIA where available.
- Core UI does not require WebGL. If WebKitGTK graphics issues occur, show targeted diagnostics rather than setting global environment workarounds unconditionally.
- Document troubleshooting in order: current system packages/drivers; then narrowly scoped `__NV_DISABLE_EXPLICIT_SYNC`; then `WEBKIT_DISABLE_DMABUF_RENDERER`; last resort `WEBKIT_DISABLE_COMPOSITING_MODE`, only where the official Tauri guidance applies.

On the 2026-09-22 development host, a normal Wayland launch exited with a GDK protocol error while `WEBKIT_DISABLE_DMABUF_RENDERER=1 rehow` stayed alive cleanly for the five-second smoke window. An X11 launch also stayed alive but logged GBM allocation warnings. This is environment evidence, not a global launcher default: keep the workaround scoped to an affected invocation while driver/WebKitGTK qualification remains open.

The packaged `.desktop` entry was also launched successfully through `gtk-launch Rehow` on that Wayland session with the same scoped workaround. This verifies menu resolution without installing the package on the host; it does not replace clean graphical testing on another compositor.

## Upgrades and schema compatibility

- Package upgrades replace application files, never Workspace content.
- Opening an older supported Workspace remains read-only until an explicit migration preview, verified backup, and confirmation.
- A newer Workspace remains read-only and requires a newer Rehow version.
- Automatic application updates are deferred until artifact signing, channel policy, rollback, and package-manager interaction are specified.

## Application menu and icon

Release smoke tests launch Rehow from the desktop menu in a clean graphical login, not only a terminal. This verifies GUI `PATH`, theme/icon discovery, working directory independence, WebKitGTK runtime, and Git-unavailable behavior. Icon assets must be original, legible at 16–512 px, and reviewed in light/dark menus.

## Optional global shortcut

Rehow does not edit Hyprland/Omarchy configuration. Documentation may show an opt-in user binding after confirming the installed executable path, for example conceptually:

```text
bindd = SUPER, R, Open Rehow, exec, rehow
```

The final instructions must use current Omarchy/Hyprland guidance and tell the user which user-owned override file to edit. Never overwrite an Omarchy-managed file or claim a universal binding that conflicts with the user's setup.

## MCP installation plan

`rehow-mcp` is a separate executable/package entry point. It runs without the desktop over stdio and receives uniquely aliased Workspace roots in the MCP client's user-controlled configuration. Copyable Codex and generic local-client examples, capability profiles, verification, troubleshooting, removal, and the end-to-end “Add this to Rehow” workflow are in `docs/mcp-setup.md`.

The read-only development invocation is:

```sh
cargo run -p rehow-mcp -- --workspace personal=/absolute/path/to/workspace
```

Repeat `--workspace alias=/absolute/path` for up to 16 non-overlapping compatible Workspaces. Aliases and random launch handles—not paths—appear in tool results. Without another flag, read and preview tools provide bounded Workspace/Category/Guide/Attachment metadata, validation, search, duplicate/related discovery, confined Git status, refresh, and safe effect previews. `--allow-write personal` enables typed narrow mutations. `--allow-broad personal` additionally enables native-approved Category reorder/removal and Attachment removal. `--allow-git-write personal` additionally enables native-approved managed-only commit and separately approved safe sync/recovery. Broad and Git grants each require the matching write grant. Write mode requires a private XDG state base outside every configured Workspace; unsafe state/runtime overlap fails startup. The server supports MCP `2026-07-28` discovery and `2025-11-25` initialization.

Packagers must install `rehow-mcp` and `rehow-mcp-approve` as separate one-link sibling files; Cargo's hard-linked build artifacts are not an install layout and are intentionally rejected by the helper provenance gate.

Rehow never edits global MCP client configuration automatically and never asks for an API key. The server has no network listener in the initial release.

## Uninstall behavior

Remove the native package with the same package manager used to install it:

```sh
sudo apt remove rehow
```

or:

```sh
sudo dnf remove Rehow
```

Package removal deletes installed application/MCP binaries, desktop entry, and packaged icons. It does not delete any user-selected Workspace. Application preferences, cache, logs, and recovery state are outside the package payload and should be reviewed separately before manual removal.

Document separately:

- removing native package/application binaries;
- removing user-level MCP executable/client entry;
- optional removal of XDG cache (safe/rebuildable);
- optional removal of preferences/logs;
- recovery-state implications.

Workspaces are always preserved by default. Recovery/backups are not silently removed because they may be the only pre-migration copy.

## Release installation acceptance

- Clean supported system installs via each advertised artifact.
- Menu launch works in Wayland and X11 without terminal environment.
- Create/open/search/edit/restart works; Git missing degrades cleanly.
- Upgrade preserves preferences and all Workspace bytes unless user accepts an explicit content migration.
- Uninstall preserves Workspaces and accurately reports retained state.
- Desktop entry/icon validate; package dependency checks pass.
- MCP installs/runs independently and sample client configuration handshakes.
