//! [`HidBackend`] over hidapi: enumeration with a vendor filter, and open.

use std::collections::HashMap;
use std::ffi::{CString, OsString};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use gearwise_core::transport::BusKind;
use gearwise_core::{Error, HidBackend, HidInterfaceInfo, HidTransport, Result};
use hidapi::{BusType, DeviceInfo, HidApi};

use crate::error_map::map_error;
use crate::transport::HidapiTransport;

pub const VID_LOGITECH: u16 = 0x046d;
pub const VID_RAZER: u16 = 0x1532;
pub const VID_CORSAIR: u16 = 0x1b1c;
pub const VID_STEELSERIES: u16 = 0x1038;

/// Vendors Gearwise has (or plans) drivers for.
pub const DEFAULT_VENDOR_IDS: [u16; 4] = [VID_LOGITECH, VID_RAZER, VID_CORSAIR, VID_STEELSERIES];

/// Which USB vendor IDs enumeration reports. Everything else is dropped before
/// it reaches the caller, so keyboards, hubs and unrelated HID gadgets are
/// never even looked at by the drivers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VendorFilter {
    /// `None` means every vendor.
    vendors: Option<Vec<u16>>,
}

impl VendorFilter {
    /// Report only these vendor IDs.
    pub fn only(vendors: impl IntoIterator<Item = u16>) -> Self {
        let mut v: Vec<u16> = vendors.into_iter().collect();
        v.sort_unstable();
        v.dedup();
        Self { vendors: Some(v) }
    }

    /// Report every HID interface (useful for diagnostics).
    pub fn any() -> Self {
        Self { vendors: None }
    }

    pub fn allows(&self, vendor_id: u16) -> bool {
        self.vendors
            .as_ref()
            .is_none_or(|v| v.binary_search(&vendor_id).is_ok())
    }

    /// The explicit vendor list, `None` when everything is allowed.
    pub fn vendors(&self) -> Option<&[u16]> {
        self.vendors.as_deref()
    }
}

impl Default for VendorFilter {
    /// Logitech, Razer, Corsair and SteelSeries.
    fn default() -> Self {
        Self::only(DEFAULT_VENDOR_IDS)
    }
}

/// hidapi-backed [`HidBackend`].
///
/// One `HidApi` context lives behind a mutex for the backend's lifetime.
/// [`enumerate`](HidBackend::enumerate) refreshes it and never opens a device
/// for I/O; only [`open`](HidBackend::open) does.
///
/// On Linux, hidapi's pure-Rust enumeration walks and reads every sysfs
/// attribute of every hidraw node and all of its ancestors (about 100 ms and
/// 17 000 syscalls for a handful of devices). The device manager polls every
/// couple of seconds, so `enumerate` first compares a cheap fingerprint of
/// `/sys/class/hidraw` (node names plus the sysfs paths they link to, which
/// embed a per-plug instance counter) and returns the previous result while
/// nothing was plugged or unplugged.
pub struct HidapiBackend {
    state: Mutex<State>,
    filter: VendorFilter,
}

struct State {
    api: HidApi,
    cache: Option<Cache>,
}

struct Cache {
    fingerprint: Fingerprint,
    interfaces: Vec<HidInterfaceInfo>,
}

impl HidapiBackend {
    /// Backend reporting the [default vendors](DEFAULT_VENDOR_IDS).
    pub fn new() -> Result<Self> {
        Self::with_filter(VendorFilter::default())
    }

    pub fn with_filter(filter: VendorFilter) -> Result<Self> {
        let api = HidApi::new()
            .map_err(|e| Error::Transport(format!("cannot initialise hidapi: {e}")))?;
        Ok(Self {
            state: Mutex::new(State { api, cache: None }),
            filter,
        })
    }

    pub fn filter(&self) -> &VendorFilter {
        &self.filter
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // The state holds no invariants a panicking thread could break
        // halfway: the device list is rebuilt on every refresh and the cache
        // is only replaced after a refresh succeeded.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl HidBackend for HidapiBackend {
    fn enumerate(&self) -> Result<Vec<HidInterfaceInfo>> {
        let mut state = self.lock();

        // Taken before the refresh: a device that appears in between only
        // causes one extra refresh on the next call.
        let fingerprint = device_fingerprint();
        if let (Some(fp), Some(cache)) = (&fingerprint, &state.cache) {
            if *fp == cache.fingerprint {
                return Ok(cache.interfaces.clone());
            }
        }

        refresh(&mut state.api, &self.filter)
            .map_err(|e| Error::Transport(format!("HID enumeration failed: {e}")))?;
        let interfaces = dedupe_by_path(
            state
                .api
                .device_list()
                .filter(|d| self.filter.allows(d.vendor_id()))
                .filter_map(interface_info),
        );
        state.cache = fingerprint.map(|fingerprint| Cache {
            fingerprint,
            interfaces: interfaces.clone(),
        });
        Ok(interfaces)
    }

    fn open(&self, path: &str) -> Result<Box<dyn HidTransport>> {
        let c_path = CString::new(path)
            .map_err(|_| Error::Transport(format!("invalid HID path {path:?} (embedded NUL)")))?;
        let dev = self.lock().api.open_path(&c_path).map_err(|e| {
            let err = map_error(&e, path);
            log::debug!("opening {path} failed: {e} -> {err}");
            err
        })?;
        Ok(Box::new(HidapiTransport::new(dev, path.to_owned())))
    }
}

/// Sorted `(node name, sysfs link target)` pairs of the hidraw class.
type Fingerprint = Vec<(OsString, PathBuf)>;

/// Cheap description of "which HID nodes exist right now", or `None` where
/// there is no such shortcut (the caller then always refreshes).
fn device_fingerprint() -> Option<Fingerprint> {
    if cfg!(target_os = "linux") {
        dir_fingerprint(Path::new("/sys/class/hidraw"))
    } else {
        None
    }
}

fn dir_fingerprint(dir: &Path) -> Option<Fingerprint> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        // No hidraw class at all: a stable "no devices".
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(Vec::new()),
        Err(_) => return None,
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.ok()?;
        // Anything that is not a symlink is not the layout we know; refuse
        // to cache rather than risk serving stale data.
        out.push((entry.file_name(), std::fs::read_link(entry.path()).ok()?));
    }
    out.sort();
    Some(out)
}

/// Rebuild hidapi's device list.
fn refresh(api: &mut HidApi, filter: &VendorFilter) -> hidapi::HidResult<()> {
    match filter.vendors() {
        // The Windows backend builds the full descriptor (string queries,
        // preparsed data, devnode properties) for every device it is asked
        // to index, so asking per vendor avoids that work for the dozens of
        // unrelated HID interfaces a PC has. On Linux a single udev scan
        // filtered afterwards is cheaper than one scan per vendor.
        Some(vendors) if cfg!(windows) => {
            api.reset_devices()?;
            vendors.iter().try_for_each(|&vid| api.add_devices(vid, 0))
        }
        _ => api.refresh_devices(),
    }
}

/// Convert hidapi's record. `None` when the path cannot be represented
/// losslessly as a `String`: such an interface could not be passed back to
/// [`HidBackend::open`] anyway.
fn interface_info(d: &DeviceInfo) -> Option<HidInterfaceInfo> {
    let Ok(path) = d.path().to_str() else {
        log::debug!("skipping HID interface with non-UTF-8 path {:?}", d.path());
        return None;
    };
    Some(HidInterfaceInfo {
        path: path.to_owned(),
        vendor_id: d.vendor_id(),
        product_id: d.product_id(),
        interface_number: d.interface_number(),
        usage_page: d.usage_page(),
        usage: d.usage(),
        manufacturer: non_empty(d.manufacturer_string()),
        product: non_empty(d.product_string()),
        serial: non_empty(d.serial_number()),
        bus: bus_kind(d.bus_type()),
    })
}

fn non_empty(s: Option<&str>) -> Option<String> {
    s.filter(|s| !s.is_empty()).map(str::to_owned)
}

fn bus_kind(bus: BusType) -> BusKind {
    match bus {
        BusType::Usb => BusKind::Usb,
        BusType::Bluetooth => BusKind::Bluetooth,
        BusType::Unknown | BusType::I2c | BusType::Spi => BusKind::Unknown,
    }
}

/// One entry per path, in first-seen order.
///
/// The Linux backend emits one record per top-level collection of a hidraw
/// node's report descriptor, all with the same path, while `open` works on the
/// whole node. When several exist the vendor-defined collection (usage page
/// 0xFF00 and up, where configuration protocols live) wins, otherwise the
/// first. On Windows every collection has its own path, so nothing is merged.
fn dedupe_by_path(infos: impl IntoIterator<Item = HidInterfaceInfo>) -> Vec<HidInterfaceInfo> {
    let mut out: Vec<HidInterfaceInfo> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for info in infos {
        match index.get(&info.path) {
            None => {
                index.insert(info.path.clone(), out.len());
                out.push(info);
            }
            Some(&i) => {
                if !is_vendor_page(out[i].usage_page) && is_vendor_page(info.usage_page) {
                    out[i] = info;
                }
            }
        }
    }
    out
}

fn is_vendor_page(usage_page: u16) -> bool {
    usage_page >= 0xff00
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iface(path: &str, vid: u16, usage_page: u16, usage: u16) -> HidInterfaceInfo {
        HidInterfaceInfo {
            path: path.to_owned(),
            vendor_id: vid,
            product_id: 0xc547,
            usage_page,
            usage,
            ..Default::default()
        }
    }

    #[test]
    fn default_filter_covers_the_four_vendors() {
        let f = VendorFilter::default();
        for vid in [0x046d, 0x1532, 0x1b1c, 0x1038] {
            assert!(f.allows(vid), "{vid:04x} should be allowed");
        }
        for vid in [
            0x0000, 0x8086, 0x045e, 0x046e, 0x1531, 0x1b1d, 0x1039, 0xffff,
        ] {
            assert!(!f.allows(vid), "{vid:04x} should be filtered out");
        }
        assert_eq!(f.vendors(), Some(&[0x046d, 0x1038, 0x1532, 0x1b1c][..]));
    }

    #[test]
    fn any_and_custom_filters() {
        assert!(VendorFilter::any().allows(0x8086));
        assert_eq!(VendorFilter::any().vendors(), None);

        let f = VendorFilter::only([0x1532, 0x1532, 0x0001]);
        assert!(f.allows(0x0001) && f.allows(0x1532));
        assert!(!f.allows(0x046d));
        assert_eq!(f.vendors(), Some(&[0x0001, 0x1532][..]));

        assert!(!VendorFilter::only([]).allows(0x046d));
    }

    #[test]
    fn dedupe_keeps_distinct_paths_in_order() {
        let out = dedupe_by_path([
            iface("/dev/hidraw2", VID_RAZER, 1, 2),
            iface("/dev/hidraw0", VID_LOGITECH, 0xff00, 1),
            iface("/dev/hidraw1", VID_LOGITECH, 0xff00, 2),
        ]);
        let paths: Vec<_> = out.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(paths, ["/dev/hidraw2", "/dev/hidraw0", "/dev/hidraw1"]);
    }

    #[test]
    fn dedupe_prefers_vendor_collection_then_first() {
        let out = dedupe_by_path([
            iface("/dev/hidraw0", VID_LOGITECH, 1, 6),
            iface("/dev/hidraw0", VID_LOGITECH, 0xff43, 0x0202),
            iface("/dev/hidraw0", VID_LOGITECH, 0x0c, 1),
            iface("/dev/hidraw1", VID_LOGITECH, 1, 2),
            iface("/dev/hidraw1", VID_LOGITECH, 0x0c, 1),
        ]);
        assert_eq!(out.len(), 2);
        assert_eq!((out[0].usage_page, out[0].usage), (0xff43, 0x0202));
        assert_eq!((out[1].usage_page, out[1].usage), (1, 2));
    }

    #[test]
    fn dedupe_keeps_first_vendor_collection() {
        let out = dedupe_by_path([
            iface("/dev/hidraw0", VID_LOGITECH, 0xff00, 1),
            iface("/dev/hidraw0", VID_LOGITECH, 0xff43, 2),
        ]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].usage_page, 0xff00);
    }

    #[test]
    fn bus_mapping() {
        assert_eq!(bus_kind(BusType::Usb), BusKind::Usb);
        assert_eq!(bus_kind(BusType::Bluetooth), BusKind::Bluetooth);
        assert_eq!(bus_kind(BusType::Unknown), BusKind::Unknown);
        assert_eq!(bus_kind(BusType::I2c), BusKind::Unknown);
        assert_eq!(bus_kind(BusType::Spi), BusKind::Unknown);
    }

    #[test]
    fn empty_strings_become_none() {
        assert_eq!(non_empty(None), None);
        assert_eq!(non_empty(Some("")), None);
        assert_eq!(non_empty(Some("G502")), Some("G502".to_owned()));
    }

    #[cfg(unix)]
    mod fingerprint {
        use super::super::*;
        use std::os::unix::fs::symlink;

        struct TempDir(PathBuf);

        impl TempDir {
            fn new(tag: &str) -> Self {
                let dir =
                    std::env::temp_dir().join(format!("gearwise-hid-{tag}-{}", std::process::id()));
                let _ = std::fs::remove_dir_all(&dir);
                std::fs::create_dir_all(&dir).unwrap();
                Self(dir)
            }
        }

        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        #[test]
        fn missing_directory_is_an_empty_fingerprint() {
            let dir = TempDir::new("missing");
            assert_eq!(dir_fingerprint(&dir.0.join("nope")), Some(Vec::new()));
        }

        #[test]
        fn fingerprint_tracks_plug_and_unplug_even_when_the_node_name_is_reused() {
            let dir = TempDir::new("track");
            let node = |name: &str| dir.0.join(name);
            symlink(
                "../../devices/usb1/0003:046D:C08B.0001/hidraw/hidraw0",
                node("hidraw0"),
            )
            .unwrap();
            symlink(
                "../../devices/usb1/0003:1B1C:1BA4.0002/hidraw/hidraw1",
                node("hidraw1"),
            )
            .unwrap();

            let before = dir_fingerprint(&dir.0).unwrap();
            assert_eq!(before.len(), 2);
            assert_eq!(
                dir_fingerprint(&dir.0).unwrap(),
                before,
                "stable while nothing changes"
            );

            // Replug: same node name, new HID instance counter in the target.
            std::fs::remove_file(node("hidraw0")).unwrap();
            symlink(
                "../../devices/usb1/0003:046D:C08B.0003/hidraw/hidraw0",
                node("hidraw0"),
            )
            .unwrap();
            let replugged = dir_fingerprint(&dir.0).unwrap();
            assert_ne!(replugged, before);

            // Unplug.
            std::fs::remove_file(node("hidraw1")).unwrap();
            assert_eq!(dir_fingerprint(&dir.0).unwrap().len(), 1);
        }

        #[test]
        fn non_symlink_entries_disable_caching() {
            let dir = TempDir::new("plain");
            std::fs::write(dir.0.join("hidraw0"), b"x").unwrap();
            assert_eq!(dir_fingerprint(&dir.0), None);
        }
    }
}
