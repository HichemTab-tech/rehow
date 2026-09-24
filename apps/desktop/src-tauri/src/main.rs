#[cfg(target_os = "linux")]
use std::ffi::OsStr;

#[cfg(target_os = "linux")]
const NVIDIA_EXPLICIT_SYNC_ENV: &str = "__NV_DISABLE_EXPLICIT_SYNC";

#[cfg(target_os = "linux")]
fn should_disable_nvidia_explicit_sync(
    wayland_display: Option<&OsStr>,
    existing_override: Option<&OsStr>,
) -> bool {
    wayland_display.is_some_and(|display| !display.is_empty()) && existing_override.is_none()
}

#[cfg(target_os = "linux")]
fn configure_wayland_graphics() {
    let wayland_display = std::env::var_os("WAYLAND_DISPLAY");
    let existing_override = std::env::var_os(NVIDIA_EXPLICIT_SYNC_ENV);

    if should_disable_nvidia_explicit_sync(wayland_display.as_deref(), existing_override.as_deref())
    {
        // SAFETY: this is the first operation in main, before Tauri, GTK,
        // WebKitGTK, or any application thread is initialized.
        unsafe { std::env::set_var(NVIDIA_EXPLICIT_SYNC_ENV, "1") };
    }
}

fn main() {
    #[cfg(target_os = "linux")]
    configure_wayland_graphics();

    rehow_desktop::run();
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::should_disable_nvidia_explicit_sync;
    use std::ffi::OsStr;

    #[test]
    fn enables_workaround_for_wayland_without_an_override() {
        assert!(should_disable_nvidia_explicit_sync(
            Some(OsStr::new("wayland-1")),
            None,
        ));
    }

    #[test]
    fn leaves_non_wayland_sessions_unchanged() {
        assert!(!should_disable_nvidia_explicit_sync(None, None));
        assert!(!should_disable_nvidia_explicit_sync(
            Some(OsStr::new("")),
            None,
        ));
    }

    #[test]
    fn preserves_an_explicit_user_setting() {
        assert!(!should_disable_nvidia_explicit_sync(
            Some(OsStr::new("wayland-1")),
            Some(OsStr::new("0")),
        ));
        assert!(!should_disable_nvidia_explicit_sync(
            Some(OsStr::new("wayland-1")),
            Some(OsStr::new("")),
        ));
    }
}
