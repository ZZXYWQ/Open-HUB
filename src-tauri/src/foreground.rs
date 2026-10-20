//! Foreground application lookup and change notifications for profile
//! auto-switching. Like G HUB's process watcher, this uses SetWinEventHook,
//! so Windows tells us about foreground changes instead of us polling.

/// Calls `on_change` (on a worker thread) whenever another application's
/// window comes to the foreground. Our own windows are skipped.
#[cfg(windows)]
pub fn watch(on_change: impl Fn() + Send + 'static) {
    use std::sync::mpsc::{channel, Sender};
    use std::sync::OnceLock;

    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetMessageW, EVENT_SYSTEM_FOREGROUND, MSG, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
    };

    static TX: OnceLock<Sender<()>> = OnceLock::new();

    unsafe extern "system" fn on_foreground(_: HWINEVENTHOOK, _: u32, _: HWND, _: i32, _: i32, _: u32, _: u32) {
        if let Some(tx) = TX.get() {
            let _ = tx.send(());
        }
    }

    let (tx, rx) = channel();
    if TX.set(tx).is_err() {
        return;
    }
    // Out-of-context hooks are delivered through this thread's message loop.
    std::thread::Builder::new()
        .name("foreground-hook".into())
        .spawn(|| {
            // SAFETY: the hook procedure has the WINEVENTPROC signature and only
            // touches a channel; the loop keeps the hook alive for the process.
            unsafe {
                let hook = SetWinEventHook(
                    EVENT_SYSTEM_FOREGROUND,
                    EVENT_SYSTEM_FOREGROUND,
                    std::ptr::null_mut(),
                    Some(on_foreground),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                );
                if hook.is_null() {
                    eprintln!("foreground hook unavailable; auto-switch disabled");
                    return;
                }
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {}
            }
        })
        .expect("could not start foreground hook");
    std::thread::Builder::new()
        .name("foreground-watch".into())
        .spawn(move || {
            for () in rx {
                on_change();
            }
        })
        .expect("could not start foreground watcher");
}

#[cfg(not(windows))]
pub fn watch(_on_change: impl Fn() + Send + 'static) {}

#[cfg(windows)]
pub fn foreground_exe() -> Option<String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    // SAFETY: plain Win32 calls with checked handles; the buffer length is
    // passed in and updated by QueryFullProcessImageNameW.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(process);
        (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}

#[cfg(not(windows))]
pub fn foreground_exe() -> Option<String> {
    None
}
