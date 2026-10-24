//! Device discovery and lifetime: matches HID interfaces to drivers, keeps
//! opened devices, and drops them when their interface disappears.

use std::collections::HashSet;

use crate::device::{Device, Driver};
use crate::error::Error;
use crate::transport::{HidBackend, HidInterfaceInfo};

/// Why an interface a driver wanted could not be opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanIssue {
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub product: Option<String>,
    pub permission_denied: bool,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanReport {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub issues: Vec<ScanIssue>,
}

impl ScanReport {
    pub fn changed(&self) -> bool {
        !self.added.is_empty() || !self.removed.is_empty()
    }
}

struct Entry {
    /// Interface path the devices came from; `None` for simulated devices.
    path: Option<String>,
    devices: Vec<Box<dyn Device>>,
}

pub struct DeviceManager {
    backend: Box<dyn HidBackend>,
    drivers: Vec<Box<dyn Driver>>,
    entries: Vec<Entry>,
    /// Paths that failed to open; retried only when they disappear and come
    /// back, so a missing udev rule doesn't spam the log every scan.
    failed: HashSet<String>,
}

impl DeviceManager {
    pub fn new(backend: Box<dyn HidBackend>, drivers: Vec<Box<dyn Driver>>) -> Self {
        Self {
            backend,
            drivers,
            entries: Vec::new(),
            failed: HashSet::new(),
        }
    }

    /// Add devices that don't come from enumeration (demo devices).
    pub fn add_virtual(&mut self, devices: Vec<Box<dyn Device>>) -> Vec<String> {
        let ids = devices.iter().map(|d| d.descriptor().id.clone()).collect();
        self.entries.push(Entry {
            path: None,
            devices,
        });
        ids
    }

    /// Remove all simulated devices.
    pub fn remove_virtual(&mut self) -> Vec<String> {
        let mut removed = Vec::new();
        self.entries.retain(|e| {
            if e.path.is_none() {
                removed.extend(e.devices.iter().map(|d| d.descriptor().id.clone()));
                false
            } else {
                true
            }
        });
        removed
    }

    pub fn has_real_devices(&self) -> bool {
        self.entries
            .iter()
            .any(|e| e.path.is_some() && !e.devices.is_empty())
    }

    /// Explicit retry after a reconnect/open failure. Normal scans retain their
    /// backoff; a user-requested retry may attempt present failed paths again.
    pub fn retry_scan(&mut self)->ScanReport {self.failed.clear();self.scan()}

    /// Enumerate interfaces, open new ones, drop vanished ones.
    pub fn scan(&mut self) -> ScanReport {
        let mut report = ScanReport::default();
        let ifaces = match self.backend.enumerate() {
            Ok(i) => i,
            Err(e) => {
                log::warn!("HID enumeration failed: {e}");
                return report;
            }
        };
        let present: HashSet<&str> = ifaces.iter().map(|i| i.path.as_str()).collect();

        // Drop devices whose interface is gone.
        self.entries.retain(|e| match &e.path {
            Some(p) if !present.contains(p.as_str()) => {
                report
                    .removed
                    .extend(e.devices.iter().map(|d| d.descriptor().id.clone()));
                false
            }
            _ => true,
        });
        self.failed.retain(|p| present.contains(p.as_str()));

        let mut opened: HashSet<String> =
            self.entries.iter().filter_map(|e| e.path.clone()).collect();
        for iface in &ifaces {
            // Backends may list one path several times (one per top-level
            // collection); open each path once.
            if opened.contains(&iface.path) || self.failed.contains(&iface.path) {
                continue;
            }
            let Some(driver) = self.drivers.iter().find(|d| d.probe(iface)) else {
                continue;
            };
            match driver.open(iface, self.backend.as_ref()) {
                // Nothing behind it yet (a receiver whose devices are asleep):
                // leave it unclaimed so the next scan tries again.
                Ok(devices) if devices.is_empty() => {}
                Ok(devices) => {
                    for d in &devices {
                        log::info!(
                            "{}: opened {} ({})",
                            driver.name(),
                            d.descriptor().name,
                            iface.path
                        );
                    }
                    report
                        .added
                        .extend(devices.iter().map(|d| d.descriptor().id.clone()));
                    opened.insert(iface.path.clone());
                    self.entries.push(Entry {
                        path: Some(iface.path.clone()),
                        devices,
                    });
                }
                Err(e) => {
                    log::warn!("{}: cannot open {}: {e}", driver.name(), iface.path);
                    self.failed.insert(iface.path.clone());
                    report.issues.push(issue(iface, &e));
                }
            }
        }
        report
    }

    pub fn devices(&self) -> impl Iterator<Item = &dyn Device> {
        self.entries
            .iter()
            .flat_map(|e| e.devices.iter().map(|d| d.as_ref()))
    }

    pub fn devices_mut(&mut self) -> impl Iterator<Item = &mut Box<dyn Device>> {
        self.entries.iter_mut().flat_map(|e| e.devices.iter_mut())
    }

    pub fn device_mut(&mut self, id: &str) -> Option<&mut Box<dyn Device>> {
        self.devices_mut().find(|d| d.descriptor().id == id)
    }

    /// Forget a device whose I/O failed with [`Error::Disconnected`]; the next
    /// scan reopens it if it's still there.
    pub fn drop_device(&mut self, id: &str) {
        for e in &mut self.entries {
            e.devices.retain(|d| d.descriptor().id != id);
        }
        self.entries
            .retain(|e| e.path.is_none() || !e.devices.is_empty());
    }
}

fn issue(iface: &HidInterfaceInfo, e: &Error) -> ScanIssue {
    ScanIssue {
        path: iface.path.clone(),
        vendor_id: iface.vendor_id,
        product_id: iface.product_id,
        product: iface.product.clone(),
        permission_denied: matches!(e, Error::PermissionDenied(_)),
        message: e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;
    use crate::error::Result;
    use crate::model::Vendor;
    use crate::transport::*;
    use std::sync::{Arc, Mutex};

    struct OneShot;
    impl Driver for OneShot {
        fn name(&self) -> &'static str {
            "test"
        }
        fn vendor(&self) -> Vendor {
            Vendor::Other
        }
        fn probe(&self, iface: &HidInterfaceInfo) -> bool {
            iface.vendor_id == 0x1234
        }
        fn open(
            &self,
            iface: &HidInterfaceInfo,
            backend: &dyn HidBackend,
        ) -> Result<Vec<Box<dyn Device>>> {
            backend.open(&iface.path)?;
            Ok(vec![demo::devices().remove(0)])
        }
    }

    fn iface(path: &str, vid: u16) -> HidInterfaceInfo {
        HidInterfaceInfo {
            path: path.into(),
            vendor_id: vid,
            ..Default::default()
        }
    }

    #[test]
    fn scan_opens_matching_and_reports_permission_issues() {
        let backend = MockBackend::new(
            vec![iface("a", 0x1234), iface("b", 0x1234), iface("c", 0x9999)],
            |p| {
                if p == "b" {
                    Err(Error::PermissionDenied(p.into()))
                } else {
                    Ok(Box::new(MockTransport::new(|_| MockReply::none())))
                }
            },
        );
        let mut m = DeviceManager::new(Box::new(backend), vec![Box::new(OneShot)]);
        let r = m.scan();
        assert_eq!(r.added.len(), 1);
        assert_eq!(r.issues.len(), 1);
        assert!(r.issues[0].permission_denied);
        // Second scan: nothing new, failed path not retried.
        let r2 = m.scan();
        assert!(!r2.changed());
        assert!(r2.issues.is_empty());
        assert!(m.has_real_devices());
    }

    #[test]
    fn vanished_interface_removes_device() {
        let present = Arc::new(Mutex::new(true));
        struct Toggle(Arc<Mutex<bool>>);
        impl HidBackend for Toggle {
            fn enumerate(&self) -> Result<Vec<HidInterfaceInfo>> {
                Ok(if *self.0.lock().unwrap() {
                    vec![iface("a", 0x1234)]
                } else {
                    vec![]
                })
            }
            fn open(&self, _: &str) -> Result<Box<dyn HidTransport>> {
                Ok(Box::new(MockTransport::new(|_| MockReply::none())))
            }
        }
        let mut m = DeviceManager::new(Box::new(Toggle(present.clone())), vec![Box::new(OneShot)]);
        assert_eq!(m.scan().added.len(), 1);
        *present.lock().unwrap() = false;
        assert_eq!(m.scan().removed.len(), 1);
        assert_eq!(m.devices().count(), 0);
    }

    #[test]
    fn virtual_devices_survive_scans() {
        let backend = MockBackend::new(vec![], |_| Err(Error::Disconnected));
        let mut m = DeviceManager::new(Box::new(backend), vec![]);
        let ids = m.add_virtual(demo::devices());
        assert!(!ids.is_empty());
        m.scan();
        assert_eq!(m.devices().count(), ids.len());
        assert!(!m.has_real_devices());
        assert_eq!(m.remove_virtual().len(), ids.len());
    }
}
