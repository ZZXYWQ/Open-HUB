//! Real HID backend for Gearwise, built on the [`hidapi`] crate.
//!
//! * [`HidapiBackend`] implements [`gearwise_core::HidBackend`]: cheap,
//!   permission-free enumeration filtered by [`VendorFilter`], and `open`.
//! * [`HidapiTransport`] implements [`gearwise_core::HidTransport`] with OS
//!   errors mapped to [`gearwise_core::Error`] (`PermissionDenied`,
//!   `Disconnected`, `Timeout`, `Transport`); see the `error_map` module docs
//!   for how.
//! * [`permission_hint`] gives a short, platform-specific fix to show when a
//!   device cannot be opened.
//!
//! The build needs no system C libraries: Linux uses hidapi's
//! `linux-native-basic-udev` backend (hidraw plus a pure-Rust sysfs/udev
//! look-alike) and Windows uses `windows-native` (`windows-sys` bindings).
//!
//! On Linux enumeration only reads sysfs (`/sys/class/hidraw`), so it needs no
//! privileges and no running udev daemon; opening a node is what needs the
//! udev rule from `packaging/linux`.

mod backend;
mod error_map;
mod transport;

pub use backend::{
    DEFAULT_VENDOR_IDS, HidapiBackend, VID_CORSAIR, VID_LOGITECH, VID_RAZER, VID_STEELSERIES,
    VendorFilter,
};
pub use transport::HidapiTransport;

/// Short user-facing advice for fixing [`gearwise_core::Error::PermissionDenied`]
/// on this platform, or `None` where there is no known remedy.
pub fn permission_hint() -> Option<&'static str> {
    if cfg!(target_os = "linux") {
        Some(
            "Your user is not allowed to access this device. Install the udev rules \
             with packaging/linux/install-udev-rules.sh, then unplug and replug the device.",
        )
    } else if cfg!(windows) {
        Some(
            "Windows denied access to this device. Close the vendor's own app \
             (Logitech G HUB, Razer Synapse, Corsair iCUE, SteelSeries GG), which may be \
             holding it exclusively, and try again.",
        )
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hint_exists_on_supported_platforms() {
        let hint = permission_hint();
        if cfg!(any(target_os = "linux", windows)) {
            let hint = hint.expect("hint on linux/windows");
            assert!(hint.len() < 220, "keep the hint short");
            if cfg!(target_os = "linux") {
                assert!(hint.contains("install-udev-rules.sh"));
            } else {
                assert!(hint.contains("Close"));
            }
        } else {
            assert!(hint.is_none());
        }
    }
}
