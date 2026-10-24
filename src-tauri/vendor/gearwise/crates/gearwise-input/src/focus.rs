//! Which application has the keyboard focus, for per-app profile switching.

#[cfg(windows)]
mod win32;
#[cfg(target_os = "linux")]
mod x11;

/// Lowercase executable base name of the focused window's process, for
/// example `"game.exe"` on Windows or `"blender"` on Linux. Cheap enough to
/// call about once a second.
///
/// Returns `None` when there is no focused window or it cannot be
/// identified.
///
/// **Linux limitations.** Detection goes through the X11 protocol (a cached
/// connection to `$DISPLAY`), so it covers X11 sessions and XWayland
/// clients. On a Wayland session only XWayland windows are visible: while a
/// native Wayland window has focus the result is `None` or the last
/// XWayland window, because Wayland offers no portable way for one client
/// to ask which window is focused. With `$DISPLAY` unset it always returns
/// `None`.
///
/// **Windows limitations.** UWP apps report `applicationframehost.exe`, and
/// processes that deny `PROCESS_QUERY_LIMITED_INFORMATION` yield `None`.
pub fn focused_app() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        x11::focused_app()
    }
    #[cfg(windows)]
    {
        win32::focused_app()
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        None
    }
}
