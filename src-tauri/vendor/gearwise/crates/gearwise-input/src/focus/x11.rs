//! X11 focus detection: `_NET_ACTIVE_WINDOW` → `_NET_WM_PID` → `/proc`.

use std::fs;
use std::sync::{Mutex, PoisonError};

use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt, Window};
use x11rb::rust_connection::RustConnection;

/// A connection plus the atoms and root window the polls need.
struct X11 {
    conn: RustConnection,
    root: Window,
    active_window: Atom,
    wm_pid: Atom,
}

/// Kept open between polls; dropped when the server connection breaks.
static CONNECTION: Mutex<Option<X11>> = Mutex::new(None);

impl X11 {
    fn connect() -> Result<Self, Box<dyn std::error::Error>> {
        let (conn, screen) = RustConnection::connect(None)?;
        let root = conn.setup().roots[screen].root;
        let active_window = conn
            .intern_atom(false, b"_NET_ACTIVE_WINDOW")?
            .reply()?
            .atom;
        let wm_pid = conn.intern_atom(false, b"_NET_WM_PID")?.reply()?.atom;
        Ok(Self {
            conn,
            root,
            active_window,
            wm_pid,
        })
    }

    /// First 32-bit value of a property, if the window has one.
    fn property(
        &self,
        window: Window,
        property: Atom,
        kind: AtomEnum,
    ) -> Result<Option<u32>, ReplyError> {
        let reply = self
            .conn
            .get_property(false, window, property, kind, 0, 1)?
            .reply()?;
        Ok(reply.value32().and_then(|mut values| values.next()))
    }

    /// PID owning the active window, if there is one and it advertises it.
    fn active_pid(&self) -> Result<Option<u32>, ReplyError> {
        let window = self.property(self.root, self.active_window, AtomEnum::WINDOW)?;
        match window {
            Some(w) if w != 0 => self.property(w, self.wm_pid, AtomEnum::CARDINAL),
            _ => Ok(None),
        }
    }
}

pub(super) fn focused_app() -> Option<String> {
    if std::env::var_os("DISPLAY").is_none_or(|d| d.is_empty()) {
        return None;
    }
    let mut slot = CONNECTION.lock().unwrap_or_else(PoisonError::into_inner);
    if slot.is_none() {
        match X11::connect() {
            Ok(x) => *slot = Some(x),
            Err(e) => {
                log::debug!("focus: cannot connect to the X server: {e}");
                return None;
            }
        }
    }
    match slot.as_ref()?.active_pid() {
        Ok(pid) => process_name(pid?),
        Err(ReplyError::ConnectionError(e)) => {
            log::debug!("focus: X connection lost: {e}");
            *slot = None;
            None
        }
        // The window vanished between the two requests.
        Err(ReplyError::X11Error(_)) => None,
    }
}

/// Name for a PID from `/proc`: the executable's file name, or the process
/// name when the executable is unreadable.
fn process_name(pid: u32) -> Option<String> {
    let exe = fs::read_link(format!("/proc/{pid}/exe"))
        .ok()
        .and_then(|path| path.file_name().map(|n| n.to_string_lossy().into_owned()));
    let comm = || fs::read_to_string(format!("/proc/{pid}/comm")).ok();
    app_name(exe.as_deref(), comm)
}

/// Pick and normalise the name; `comm` is only read when needed.
///
/// Wine and Proton run every Windows program under a `wine*` loader binary,
/// so for those the process name (the `.exe` file, truncated by the kernel
/// to 15 bytes) is the useful one.
fn app_name(exe: Option<&str>, comm: impl FnOnce() -> Option<String>) -> Option<String> {
    let exe = exe.map(|e| e.strip_suffix(" (deleted)").unwrap_or(e));
    let name = match exe {
        Some(e) if !e.starts_with("wine") => e.to_owned(),
        _ => comm().or_else(|| exe.map(str::to_owned))?,
    };
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_name_is_lowercased() {
        assert_eq!(
            app_name(Some("Blender"), || unreachable!()).as_deref(),
            Some("blender")
        );
    }

    #[test]
    fn replaced_binaries_lose_the_deleted_suffix() {
        assert_eq!(
            app_name(Some("firefox (deleted)"), || unreachable!()).as_deref(),
            Some("firefox")
        );
    }

    #[test]
    fn wine_loaders_use_the_process_name() {
        let comm = || Some("Game.exe\n".to_string());
        assert_eq!(
            app_name(Some("wine64-preloader"), comm).as_deref(),
            Some("game.exe")
        );
        assert_eq!(
            app_name(Some("wine64-preloader"), || None).as_deref(),
            Some("wine64-preloader")
        );
    }

    #[test]
    fn falls_back_to_the_process_name_without_an_exe_link() {
        assert_eq!(
            app_name(None, || Some("steam\n".into())).as_deref(),
            Some("steam")
        );
        assert_eq!(app_name(None, || None), None);
        assert_eq!(app_name(None, || Some("\n".into())), None);
    }

    #[test]
    fn current_process_has_a_name() {
        let name = process_name(std::process::id()).expect("own /proc entry");
        assert_eq!(name, name.to_ascii_lowercase());
        assert!(!name.is_empty());
    }
}
