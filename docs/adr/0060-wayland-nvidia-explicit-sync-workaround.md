# ADR 0060: Scoped Wayland NVIDIA Explicit-Sync Workaround

Status: accepted

Date: 2026-09-24

## Context

The installed `v0.1.0` Arch package exits immediately on the release host's Hyprland/Wayland session with `Gdk-Message: Error 71 (Protocol error) dispatching to Wayland display.` There is no coredump, signal, or memory pressure. The host uses an NVIDIA RTX 3070 plus an AMD integrated GPU. A normal launch exits with status 1; both `__NV_DISABLE_EXPLICIT_SYNC=1` and the more expensive `WEBKIT_DISABLE_DMABUF_RENDERER=1` keep the application alive.

Current official [Tauri Linux graphics guidance](https://v2.tauri.app/develop/debug/linux-graphics/) identifies this exact error as a WebKitGTK/driver interoperability failure. It recommends disabling NVIDIA explicit sync before disabling the faster DMABUF renderer, and permits setting the workaround in `main()` before webview creation once the application is verified to be affected.

## Decision

- On Linux only, before Tauri, GTK, WebKitGTK, or application threads initialize, Rehow checks for a nonempty `WAYLAND_DISPLAY`.
- If `__NV_DISABLE_EXPLICIT_SYNC` is absent, Rehow sets it to `1` for its own process and descendants.
- Any explicit user value, including `0` or an empty value, is preserved.
- Sessions without a Wayland display and every non-Linux target remain unchanged.
- Rehow does not set `WEBKIT_DISABLE_DMABUF_RENDERER` by default. It remains a documented invocation-scoped fallback if explicit-sync disabling is insufficient on another system.
- Rehow does not edit Hyprland, Omarchy, shell, or global environment configuration.

## Consequences

The confirmed Wayland failure is avoided while retaining WebKitGTK's faster DMABUF path. The NVIDIA-specific variable is inert on other graphics drivers, and its Wayland-only application keeps ordinary X11 startup unchanged. Users retain an explicit escape hatch for future driver/WebKitGTK improvements or regressions.

The current `v0.1.0` release tag and assets remain immutable. Shipping the correction requires a new signed patch release; replacing an existing asset with a binary from a different commit is not allowed.

## Evidence

Three decision-unit tests cover Wayland activation, absent/empty Wayland refusal, and explicit-value preservation. The production binary remains alive for an eight-second normal Wayland smoke and an eight-second X11 smoke. With `__NV_DISABLE_EXPLICIT_SYNC=0`, it again exits with status 1 and the exact prior GDK protocol error. Production `.deb`, `.rpm`, and Arch packages pass their full shared payload, helper binding, metadata, ABI, and disposable pacman install/remove gates.
