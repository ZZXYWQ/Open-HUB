//! Windows focus detection: foreground window → process → image path.

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// Room for any path Windows allows (32,767 UTF-16 units).
const PATH_CAP: usize = 32_768;

pub(super) fn focused_app() -> Option<String> {
    // SAFETY: plain Win32 calls; every pointer passed is valid for the call
    // and the process handle is closed on all paths after it is opened.
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
        let mut buf = vec![0u16; PATH_CAP];
        let mut len = PATH_CAP as u32;
        let ok = QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(process);
        if ok == 0 {
            return None;
        }
        base_name(&String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// Lowercase file name of a Windows path.
fn base_name(path: &str) -> Option<String> {
    let name = path.rsplit(['\\', '/']).next()?.trim();
    (!name.is_empty()).then(|| name.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_name_keeps_the_last_path_component_in_lowercase() {
        assert_eq!(
            base_name(r"C:\Games\Foo\Game.EXE").as_deref(),
            Some("game.exe")
        );
        assert_eq!(base_name("notepad.exe").as_deref(), Some("notepad.exe"));
        assert_eq!(base_name(""), None);
    }
}
