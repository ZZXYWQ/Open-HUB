//! Translation of hidapi errors into [`gearwise_core::Error`].
//!
//! What `hidapi::HidError` exposes depends on the backend, so classification
//! works in three tiers, most reliable first:
//!
//! 1. **`HidError::IoError`** carries a real `std::io::Error`. The native
//!    Linux backend produces it for `read`/`write`/`poll` failures (via
//!    `nix::Errno`) and the native Windows backend produces it for every
//!    failed Win32 call (`GetLastError()` as a raw OS error). The raw code is
//!    looked up in a per-platform table; `io::ErrorKind` is the fallback.
//! 2. **`HidError::HidApiError`** is a message-only variant. The native Linux
//!    backend uses it for `open` ("failed to open device with path P: Permission
//!    denied (os error 13)") and for the report ioctls ("ioctl (GFEATURE):
//!    EPIPE: Broken pipe"), discarding the errno. We first try to recover the
//!    code from a trailing `(os error N)`, then match errno names
//!    (`nix::Errno`'s `Debug` form, whole-token) and a few stable English
//!    phrases. This string matching is a documented fallback: it only covers
//!    messages produced by hidapi 2.6.x itself, never localized OS text.
//! 3. Everything else is a plain transport error carrying the message.
//!
//! Ambiguity is resolved towards [`Error::Disconnected`] for `EPIPE` and
//! `ERROR_GEN_FAILURE`: they are what an unplugged device produces on some
//! paths, but also what a stalled control endpoint produces. The device
//! manager reacts to `Disconnected` by dropping the device and reopening it on
//! the next scan, which is cheap and self-healing; missing a real unplug would
//! leave a dead device in the UI.

use gearwise_core::Error;
use hidapi::HidError;

/// Coarse outcome of classifying a hidapi error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Class {
    /// The OS refused access (missing udev rule, device held exclusively).
    Permission,
    /// The device is gone.
    Disconnected,
    /// An OS-level deadline expired.
    Timeout,
    /// Anything else.
    Other,
}

/// Convert a hidapi error into the app-wide error type. `path` is the
/// interface the operation was performed on.
pub(crate) fn map_error(err: &HidError, path: &str) -> Error {
    match classify(err) {
        Class::Permission => Error::PermissionDenied(path.to_owned()),
        Class::Disconnected => Error::Disconnected,
        Class::Timeout => Error::Timeout,
        Class::Other => Error::Transport(err.to_string()),
    }
}

pub(crate) fn classify(err: &HidError) -> Class {
    match err {
        HidError::IoError { error } => classify_io(error),
        HidError::HidApiError { message } => classify_message(message),
        _ => Class::Other,
    }
}

fn classify_io(err: &std::io::Error) -> Class {
    use std::io::ErrorKind as K;

    if let Some(code) = err.raw_os_error() {
        let class = classify_os_error(code);
        if class != Class::Other {
            return class;
        }
    }
    match err.kind() {
        K::PermissionDenied => Class::Permission,
        K::NotFound
        | K::BrokenPipe
        | K::ConnectionReset
        | K::ConnectionAborted
        | K::NotConnected => Class::Disconnected,
        K::TimedOut => Class::Timeout,
        _ => Class::Other,
    }
}

/// Classify a raw OS error code using the table of the platform we run on.
fn classify_os_error(code: i32) -> Class {
    #[cfg(windows)]
    {
        classify_win32(code)
    }
    #[cfg(not(windows))]
    {
        classify_errno(code)
    }
}

/// POSIX errno values (the kernel's hidraw/usbhid return these).
#[cfg(not(windows))]
fn classify_errno(code: i32) -> Class {
    use libc::{
        EACCES, ECONNRESET, EIO, ENODEV, ENOENT, ENXIO, EPERM, EPIPE, EPROTO, ESHUTDOWN, ETIMEDOUT,
    };
    match code {
        EACCES | EPERM => Class::Permission,
        // hidraw: ENODEV from write/ioctl and EIO from read once the device is
        // gone; ENOENT when the node vanished before open; ESHUTDOWN,
        // ECONNRESET and EPROTO are how USB URBs complete on disconnect.
        ENODEV | ENXIO | ENOENT | EIO | EPIPE | ESHUTDOWN | ECONNRESET | EPROTO => {
            Class::Disconnected
        }
        ETIMEDOUT => Class::Timeout,
        _ => Class::Other,
    }
}

/// Win32 error codes (`winerror.h`); numeric so the table is testable on every
/// host.
#[cfg_attr(not(windows), allow(dead_code))]
fn classify_win32(code: i32) -> Class {
    const ERROR_FILE_NOT_FOUND: i32 = 2;
    const ERROR_PATH_NOT_FOUND: i32 = 3;
    const ERROR_ACCESS_DENIED: i32 = 5;
    const ERROR_INVALID_HANDLE: i32 = 6;
    const ERROR_BAD_COMMAND: i32 = 22;
    const ERROR_GEN_FAILURE: i32 = 31;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    const ERROR_DEV_NOT_EXIST: i32 = 55;
    const ERROR_SEM_TIMEOUT: i32 = 121;
    const WAIT_TIMEOUT: i32 = 258;
    const ERROR_NO_SUCH_DEVICE: i32 = 433;
    const ERROR_OPERATION_ABORTED: i32 = 995;
    const ERROR_IO_DEVICE: i32 = 1117;
    const ERROR_DEVICE_NOT_CONNECTED: i32 = 1167;
    const ERROR_TIMEOUT: i32 = 1460;

    match code {
        // A vendor app holding the device exclusively shows up as a sharing
        // violation on open or access denied on read/write.
        ERROR_ACCESS_DENIED | ERROR_SHARING_VIOLATION => Class::Permission,
        ERROR_FILE_NOT_FOUND
        | ERROR_PATH_NOT_FOUND
        | ERROR_INVALID_HANDLE
        | ERROR_BAD_COMMAND
        | ERROR_GEN_FAILURE
        | ERROR_DEV_NOT_EXIST
        | ERROR_NO_SUCH_DEVICE
        | ERROR_OPERATION_ABORTED
        | ERROR_IO_DEVICE
        | ERROR_DEVICE_NOT_CONNECTED => Class::Disconnected,
        ERROR_SEM_TIMEOUT | WAIT_TIMEOUT | ERROR_TIMEOUT => Class::Timeout,
        _ => Class::Other,
    }
}

/// Fallback for `HidError::HidApiError`, which only has a message.
fn classify_message(msg: &str) -> Class {
    // Tier 2a: a trailing "(os error N)" from `io::Error`'s Display.
    if let Some(code) = parse_os_error(msg) {
        let class = classify_os_error(code);
        if class != Class::Other {
            return class;
        }
    }

    // Tier 2b: errno names from `nix::Errno`'s Display ("EPIPE: Broken
    // pipe"), matched as whole tokens so "EIO" does not match inside a word.
    let has_token = |names: &[&str]| {
        msg.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .any(|token| names.contains(&token))
    };
    if has_token(&["EACCES", "EPERM"]) {
        return Class::Permission;
    }
    if has_token(&[
        "ENODEV",
        "ENXIO",
        "ENOENT",
        "EIO",
        "EPIPE",
        "ESHUTDOWN",
        "ECONNRESET",
        "EPROTO",
    ]) {
        return Class::Disconnected;
    }
    if has_token(&["ETIMEDOUT", "WaitTimedOut"]) {
        return Class::Timeout;
    }

    // Tier 2c: stable phrases (strerror text, hidapi's own messages).
    let lower = msg.to_ascii_lowercase();
    let has_phrase = |phrases: &[&str]| phrases.iter().any(|p| lower.contains(p));
    if has_phrase(&[
        "permission denied",
        "operation not permitted",
        "access is denied",
        "access denied",
    ]) {
        Class::Permission
    } else if has_phrase(&[
        "no such device",
        "no such file or directory",
        "broken pipe",
        "input/output error",
        "device disconnected",
        "device not connected",
    ]) {
        Class::Disconnected
    } else if has_phrase(&["timed out"]) {
        Class::Timeout
    } else {
        Class::Other
    }
}

/// Extract `N` from a trailing `(os error N)`.
fn parse_os_error(msg: &str) -> Option<i32> {
    const MARK: &str = "(os error ";
    let rest = &msg[msg.rfind(MARK)? + MARK.len()..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    fn api(msg: &str) -> HidError {
        HidError::HidApiError {
            message: msg.to_owned(),
        }
    }

    #[test]
    fn open_permission_denied_message() {
        let e =
            api("failed to open device with path /dev/hidraw3: Permission denied (os error 13)");
        assert_eq!(classify(&e), Class::Permission);
        match map_error(&e, "/dev/hidraw3") {
            Error::PermissionDenied(p) => assert_eq!(p, "/dev/hidraw3"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn permission_phrase_without_errno() {
        // Same text as above but without the code: phrase fallback.
        assert_eq!(
            classify(&api("open failed: Permission denied")),
            Class::Permission
        );
        assert_eq!(classify(&api("Access is denied.")), Class::Permission);
        assert_eq!(
            classify(&api("ioctl (GFEATURE): EACCES: Permission denied")),
            Class::Permission
        );
    }

    #[test]
    fn node_vanished_before_open() {
        let e = api(
            "failed to open device with path /dev/hidraw9: No such file or directory (os error 2)",
        );
        assert_eq!(classify(&e), Class::Disconnected);
        assert!(matches!(map_error(&e, "/dev/hidraw9"), Error::Disconnected));
    }

    #[test]
    fn report_ioctl_errno_names() {
        assert_eq!(
            classify(&api("ioctl (GFEATURE): EPIPE: Broken pipe")),
            Class::Disconnected
        );
        assert_eq!(
            classify(&api("ioctl (SOUTPUT): ENODEV: No such device")),
            Class::Disconnected
        );
        assert_eq!(
            classify(&api("ioctl (GFEATURE): EIO: Input/output error")),
            Class::Disconnected
        );
        assert_eq!(
            classify(&api("ioctl (GFEATURE): ETIMEDOUT: Connection timed out")),
            Class::Timeout
        );
        assert_eq!(
            classify(&api("ioctl (GFEATURE): EINVAL: Invalid argument")),
            Class::Other
        );
        assert_eq!(
            classify(&api(
                "ioctl (GINPUT): ENOTTY: Inappropriate ioctl for device"
            )),
            Class::Other
        );
    }

    #[test]
    fn errno_names_match_whole_tokens_only() {
        assert_eq!(classify(&api("BEIO happened")), Class::Other);
        assert_eq!(classify(&api("EIOX happened")), Class::Other);
        assert_eq!(classify(&api("EPERMANENT")), Class::Other);
    }

    #[test]
    fn poll_hangup_is_disconnect() {
        assert_eq!(
            classify(&api("unexpected poll error (device disconnected)")),
            Class::Disconnected
        );
    }

    #[test]
    fn windows_timeout_message() {
        assert_eq!(classify(&api("WinError: WaitTimedOut")), Class::Timeout);
        assert_eq!(classify(&api("WinError: Win32(IoPending)")), Class::Other);
    }

    #[test]
    fn unrelated_messages_stay_transport() {
        let e = api("hidapi error: something odd");
        match map_error(&e, "p") {
            Error::Transport(m) => assert!(m.contains("something odd")),
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(classify(&HidError::InvalidZeroSizeData), Class::Other);
        assert_eq!(
            classify(&HidError::IncompleteSendError { sent: 1, all: 2 }),
            Class::Other
        );
        assert_eq!(classify(&HidError::HidApiErrorEmpty), Class::Other);
    }

    #[test]
    fn io_error_kind_fallback_without_raw_code() {
        let io = |k| HidError::IoError {
            error: io::Error::new(k, "x"),
        };
        assert_eq!(
            classify(&io(io::ErrorKind::PermissionDenied)),
            Class::Permission
        );
        assert_eq!(
            classify(&io(io::ErrorKind::BrokenPipe)),
            Class::Disconnected
        );
        assert_eq!(classify(&io(io::ErrorKind::NotFound)), Class::Disconnected);
        assert_eq!(classify(&io(io::ErrorKind::TimedOut)), Class::Timeout);
        assert_eq!(classify(&io(io::ErrorKind::InvalidInput)), Class::Other);
    }

    #[cfg(not(windows))]
    #[test]
    fn linux_errno_table() {
        let raw = |c| HidError::IoError {
            error: io::Error::from_raw_os_error(c),
        };
        assert_eq!(classify(&raw(libc::EACCES)), Class::Permission);
        assert_eq!(classify(&raw(libc::EPERM)), Class::Permission);
        assert_eq!(classify(&raw(libc::ENODEV)), Class::Disconnected);
        assert_eq!(classify(&raw(libc::ENOENT)), Class::Disconnected);
        assert_eq!(classify(&raw(libc::EIO)), Class::Disconnected);
        assert_eq!(classify(&raw(libc::EPIPE)), Class::Disconnected);
        assert_eq!(classify(&raw(libc::ESHUTDOWN)), Class::Disconnected);
        assert_eq!(classify(&raw(libc::ETIMEDOUT)), Class::Timeout);
        assert_eq!(classify(&raw(libc::EINVAL)), Class::Other);
        assert_eq!(classify(&raw(libc::EBUSY)), Class::Other);
    }

    #[test]
    fn win32_table() {
        assert_eq!(classify_win32(5), Class::Permission); // ERROR_ACCESS_DENIED
        assert_eq!(classify_win32(32), Class::Permission); // ERROR_SHARING_VIOLATION
        assert_eq!(classify_win32(1167), Class::Disconnected); // ERROR_DEVICE_NOT_CONNECTED
        assert_eq!(classify_win32(2), Class::Disconnected); // ERROR_FILE_NOT_FOUND
        assert_eq!(classify_win32(433), Class::Disconnected); // ERROR_NO_SUCH_DEVICE
        assert_eq!(classify_win32(31), Class::Disconnected); // ERROR_GEN_FAILURE
        assert_eq!(classify_win32(1460), Class::Timeout); // ERROR_TIMEOUT
        assert_eq!(classify_win32(87), Class::Other); // ERROR_INVALID_PARAMETER
    }

    #[test]
    fn parses_os_error_suffix() {
        assert_eq!(
            parse_os_error("x: Permission denied (os error 13)"),
            Some(13)
        );
        assert_eq!(parse_os_error("no code here"), None);
        assert_eq!(parse_os_error("(os error )"), None);
    }
}
