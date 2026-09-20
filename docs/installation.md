# Installation and Desktop Integration

Status: desktop packaging planned; Phase 6C3 MCP development binary available
Last updated: 2026-09-20

## Supported baseline

Rehow is Linux-first and targets a normal resizable, decorated desktop window on Wayland and X11. The exact minimum distributions are finalized from packaged production tests. Tauri v2 uses the system WebKitGTK 4.1 webview on Linux, so release builds must run on an old-enough supported build image to preserve glibc compatibility.

Initial package priority:

1. Native `.deb` and `.rpm` packages built/tested on the documented oldest baseline.
2. AppImage only after independent Wayland/XWayland and upgrade/uninstall testing.
3. An Arch user package recipe may follow, but Rehow will not modify Omarchy-managed files.

Relevant Tauri references: [Linux prerequisites](https://v2.tauri.app/start/prerequisites/), [AppImage distribution](https://v2.tauri.app/distribute/appimage/), and [Linux graphics diagnostics](https://v2.tauri.app/develop/debug/linux-graphics/).

## Desktop application install plan

Packages install:

- versioned Rehow binary and runtime resources;
- application icon at standard hicolor sizes plus scalable source;
- `rehow.desktop` with `Type=Application`, `Name=Rehow`, `Exec=rehow`, `Icon=rehow`, appropriate Knowledge/Utility categories, and no terminal;
- license and package metadata.

User Workspaces are never placed inside the application installation directory. Default preferences/cache/state follow XDG locations. Uninstall removes application-owned binaries/launchers/cache only; it preserves every user-selected Workspace and asks before removing local recovery state.

Prefer user-level installation when the package format supports it without weakening desktop integration. System package installation may require the user's normal package manager authorization; Rehow itself never elevates privileges.

## Runtime dependencies

Package metadata must declare the exact WebKitGTK 4.1, GTK, SSL, and other runtime libraries produced by the Tauri bundler/build baseline. Linux Git operations require root-owned `/usr/bin/git` and `/usr/bin/bwrap`; authenticated SSH clone, sync, and push additionally require root-owned `/usr/bin/ssh` plus a safe launch-inherited agent and trusted `known_hosts`. Missing tools or trust keep Rehow content usable and produce closed Git-unavailable guidance. Development-only packages are in `docs/development.md`. Do not tell users to install a permanently running Node server for the desktop application.

Desktop packaging must place the fixed `rehow-git-ssh` and `rehow-git-credential` binaries beside the application executable as non-writable one-link executables. Rehow does not install, enable, or alter a user's credential helper; it may use only an already configured helper that passes the documented provenance gate.

Authenticated HTTPS supports a trusted self-contained system helper or the exact `gh auth git-credential` configuration. The `gh` path requires its ordinary private `hosts.yml`; keyring/libsecret mediation is not yet supported. Rehow never asks for or stores a token.

## Wayland behavior

- Do not depend on global window coordinates, programmatic placement, always-on-top, shadows, blur, or compositor effects; support differs on Wayland.
- Preserve useful minimum/narrow layouts and allow the compositor to manage the window normally.
- Test GNOME Wayland, KDE Wayland, wlroots/Sway/Hyprland, and X11 with Intel/AMD/NVIDIA where available.
- Core UI does not require WebGL. If WebKitGTK graphics issues occur, show targeted diagnostics rather than setting global environment workarounds unconditionally.
- Document troubleshooting in order: current system packages/drivers; then narrowly scoped `__NV_DISABLE_EXPLICIT_SYNC`; then `WEBKIT_DISABLE_DMABUF_RENDERER`; last resort `WEBKIT_DISABLE_COMPOSITING_MODE`, only where the official Tauri guidance applies.

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

`rehow-mcp` is a separate executable/package entry point. It runs without the desktop over stdio and receives uniquely aliased Workspace roots in the MCP client's user-controlled configuration. Installation docs provide copyable examples for supported clients, read-only default, per-alias `--allow-write`/`--allow-broad`/`--allow-git-write`, the fixed server-launched native `rehow-mcp-approve` companion, headless limitations, version check, stderr logging, and removal.

The implemented Phase 6C3 development invocation is:

```sh
cargo run -p rehow-mcp -- --workspace personal=/absolute/path/to/workspace
```

Repeat `--workspace alias=/absolute/path` for up to 16 non-overlapping compatible Workspaces. Aliases and random launch handles—not paths—appear in tool results. Without another flag, the nine content tools are read-only. To enable canonical Category creation, draft-Guide creation, and existing-Guide updates for one alias, add `--allow-write personal`; this exposes no delete, broad, tested, Attachment, or Git-write authority. Write mode also requires an existing private XDG state base outside every configured Workspace; unsafe state/runtime overlap fails startup. `--allow-broad` and `--allow-git-write` are not implemented. The server supports MCP `2026-07-28` discovery and `2025-11-25` initialization.

Rehow never edits global MCP client configuration automatically and never asks for an API key. The server has no network listener in the initial release.

## Uninstall behavior

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
