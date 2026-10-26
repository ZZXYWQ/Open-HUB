//! Razer driver for Gearwise.
//!
//! Razer mice and keyboards take 90-byte vendor commands as HID feature
//! reports on one of their interfaces. This crate speaks
//! that protocol for DPI and DPI stages, polling rate, battery, Chroma
//! lighting (zone effects and per-key frames) and keyboard game mode. Button
//! remapping covers captured Standard-layer mouse actions on the exact Viper
//! V3 Pro ids and independently documented Basilisk V3 Pro AA/AB bindings.
//! Other model layouts and native macro flash protocols are not inferred.
//!
//! * [`models`] lists the supported products and how each one differs.
//! * [`RazerDriver`] plugs into the device manager.
//!
//! Protocol facts (byte layouts, command ids, product ids) were taken from
//! the openrazer and OpenRGB projects as references; the code is original.

pub mod basilisk;
mod commands;
mod device;
mod keymap;
mod link;
pub mod models;
mod report;
mod power;
mod dav3pro_dpi;
pub mod dav3pro_buttons;
#[cfg(any(test, feature = "simulation"))]
mod simulation;
#[cfg(any(test, feature = "simulation"))]
pub use simulation::mouse as simulated_viper;
#[cfg(any(test,feature="simulation"))]
mod basilisk_sim;
#[cfg(any(test,feature="simulation"))]
pub use basilisk_sim::mouse as simulated_basilisk;
#[cfg(any(test, feature = "simulation"))]
mod testutil;

pub use commands::{PollProto, Store, WaveCodes};
pub use device::RazerDevice;
pub use keymap::KeyMatrix;
pub use link::{Settle, Timing};
pub use models::{MODELS, Model};

use gearwise_core::model::Vendor;
use gearwise_core::transport::{HidBackend, HidInterfaceInfo};
use gearwise_core::{Device, Driver, Error, Result};

use link::Link;

/// Driver for Razer mice and keyboards.
#[derive(Default)]
pub struct RazerDriver {
    timing: Option<Timing>,
}

impl RazerDriver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Use `timing` for every device instead of the per-model settle times
    /// (tests, or buses that need more patience).
    pub fn with_timing(timing: Timing) -> Self {
        Self {
            timing: Some(timing),
        }
    }
}

impl Driver for RazerDriver {
    fn name(&self) -> &'static str {
        "Razer"
    }

    fn vendor(&self) -> Vendor {
        Vendor::Razer
    }

    /// Claims exactly the interface that carries feature reports, so each
    /// physical device is opened once.
    fn probe(&self, iface: &HidInterfaceInfo) -> bool {
        models::for_interface(iface).is_some()
    }

    fn open(
        &self,
        iface: &HidInterfaceInfo,
        backend: &dyn HidBackend,
    ) -> Result<Vec<Box<dyn Device>>> {
        let model = models::for_interface(iface)
            .ok_or(Error::Unsupported("not a Razer control interface"))?;
        let io = backend.open(&iface.path)?;
        let timing = self.timing.unwrap_or(Timing::for_settle(model.settle));
        let link = Link::new(io, model.transaction_id, timing);
        Ok(vec![Box::new(RazerDevice::open(model, link, iface)?)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{Fake, INSTANT};
    use gearwise_core::transport::{MockBackend, MockRecord};
    use std::sync::{Arc, Mutex};

    fn iface(pid: u16, interface: i32, usage_page: u16, usage: u16) -> HidInterfaceInfo {
        HidInterfaceInfo {
            path: format!("hid-{pid:04x}-{interface}"),
            vendor_id: 0x1532,
            product_id: pid,
            interface_number: interface,
            usage_page,
            usage,
            ..Default::default()
        }
    }

    #[test]
    fn probe_picks_one_interface_per_device() {
        let d = RazerDriver::new();
        // Two DeathAdder V2 Pro dongle nodes plus a keyboard-class node.
        let nodes = [
            iface(0x007d, 0, 1, 2),
            iface(0x007d, 1, 1, 6),
            iface(0x007d, 2, 12, 1),
        ];
        assert_eq!(nodes.iter().filter(|n| d.probe(n)).count(), 1);
        assert_eq!(d.name(), "Razer");
        assert_eq!(d.vendor(), Vendor::Razer);
    }

    #[test]
    fn open_reads_identity_and_names_the_device() {
        let fake = Fake::default()
            .on(0x00, 0x82, 2, b"PM2145H12345AB\0\0\0\0\0\0\0\0")
            .on(0x00, 0x81, 2, &[1, 7]);
        let (io, log) = fake.transport();
        let slot = Arc::new(Mutex::new(Some(io)));
        let node = iface(0x0084, 0, 1, 2);
        let backend = MockBackend::new(vec![node.clone()], move |_| {
            Ok(Box::new(slot.lock().unwrap().take().expect("opened twice")))
        });

        let devs = RazerDriver::with_timing(INSTANT)
            .open(&node, &backend)
            .unwrap();
        assert_eq!(devs.len(), 1);
        let desc = devs[0].descriptor();
        assert_eq!(desc.id, "razer:0084:PM2145H12345AB");
        assert_eq!(desc.model_key, "razer:0084");
        assert_eq!(desc.name, "Razer DeathAdder V2");
        assert_eq!(desc.firmware.as_deref(), Some("v1.7"));
        assert_eq!(desc.serial.as_deref(), Some("PM2145H12345AB"));
        assert_eq!(
            (desc.vendor_id, desc.product_id, desc.simulated),
            (0x1532, 0x0084, false)
        );

        // Identity went out on the model's transaction id (0x3f).
        let log = log.lock().unwrap();
        assert!(
            matches!(&log[0], MockRecord::SetFeature(d) if d[2] == 0x3f && d[7] == 0x00 && d[8] == 0x82)
        );
    }

    #[test]
    fn open_falls_back_to_the_path_when_the_device_has_no_serial() {
        let fake = Fake::default()
            .on(0x00, 0x82, 5, &[])
            .on(0x00, 0x81, 5, &[]);
        let (io, _) = fake.transport();
        let slot = Arc::new(Mutex::new(Some(io)));
        let node = iface(0x0084, 0, 1, 2);
        let backend = MockBackend::new(vec![node.clone()], move |_| {
            Ok(Box::new(slot.lock().unwrap().take().unwrap()))
        });
        let devs = RazerDriver::with_timing(INSTANT)
            .open(&node, &backend)
            .unwrap();
        assert_eq!(devs[0].descriptor().id, "razer:0084:hid-0084-0");
        assert_eq!(devs[0].descriptor().firmware, None);
    }

    #[test]
    fn open_refuses_foreign_interfaces() {
        let backend = MockBackend::new(vec![], |_| Err(Error::Disconnected));
        let node = iface(0x0084, 1, 1, 6);
        assert!(matches!(
            RazerDriver::new().open(&node, &backend),
            Err(Error::Unsupported(_))
        ));
    }
}

#[cfg(feature="simulation")]
pub fn simulated_deathadder_v2()->(Box<dyn Device>,std::sync::Arc<std::sync::Mutex<Vec<gearwise_core::transport::MockRecord>>>){
 let fake=testutil::Fake::default().on(4,0x85,2,&[1,3,0x20,3,0x20]).on(0,0x85,2,&[1]);
 let(io,log)=fake.transport();let model=models::MODELS.iter().find(|m|m.pid==0x0084).unwrap();
 let link=link::Link::new(Box::new(io),model.transaction_id,Timing::new(std::time::Duration::ZERO,std::time::Duration::ZERO,1));
 let iface=HidInterfaceInfo{path:"simulation:razer".into(),vendor_id:0x1532,product_id:0x0084,..Default::default()};
 (Box::new(RazerDevice::open(model,link,&iface).unwrap()),log)
}

#[cfg(test)]
mod captured_dav3pro;

/// Pure exact-model power preflight, before any HID access.
pub fn validate_power_patch(pid:u16,patch:&gearwise_core::tuning::MouseTuning)->Result<()>{power::validate(pid,patch)}

#[cfg(any(test,feature="simulation"))]
mod dav3pro_sim;
#[cfg(any(test,feature="simulation"))]
pub use dav3pro_sim::mouse as simulated_dav3pro;
#[cfg(any(test,feature="simulation"))]
pub use dav3pro_sim::current_mouse as simulated_dav3pro_current;
