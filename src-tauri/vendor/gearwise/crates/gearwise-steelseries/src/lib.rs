//! SteelSeries driver for Gearwise.
//!
//! Covers the Rival 3, 100/105/106/110, 300/300S, 310, 600 and the original
//! Rival, Sensei 310 and Ten, Aerox 3/5/9 (wired and Wireless), Prime, Prime+,
//! Prime Mini and Prime Wireless, Kana v2 and Rival 3 Wireless: DPI stages,
//! polling rate, lighting, button remapping, saving to the mouse and, on the
//! wireless models, the battery. Apex keyboards are not covered: their
//! lighting protocol is not documented well enough to implement from here.
//!
//! These mice take short vendor commands as output (sometimes feature)
//! reports on one HID interface. Complete configuration reads are not yet
//! implemented here; the device keeps the values it last applied, starting from the
//! factory defaults rivalcfg documents, and serves those from `read_state`.
//! Settings changed with other software or the mouse's own buttons are not
//! noticed.
//!
//! * [`models`] lists the supported products and how each one differs.
//! * [`SteelSeriesDriver`] plugs into the device manager.
//!
//! Protocol facts (command bytes, value encodings, product ids) were taken
//! from the rivalcfg project and checked against libratbag and OpenRGB where
//! they cover the same mice; the code is original.

mod battery;
mod buttons;
pub mod cpi;
mod device;
mod dpi;
mod light;
pub mod rival3;
pub mod prime;
pub mod prime_gradient;
pub mod rival3_sequence;
mod link;
mod proximity;
pub mod models;
#[cfg(any(test, feature = "simulation"))]
mod testutil;

pub use models::{MODELS, Model};

use std::time::Duration;

use gearwise_core::model::Vendor;
use gearwise_core::transport::{HidBackend, HidInterfaceInfo};
use gearwise_core::{Device, Driver, Error, Result};

use device::SteelSeriesDevice;
use link::Link;

/// Pause after every command; the firmware drops commands that come faster.
const DEFAULT_GAP: Duration = Duration::from_millis(20);

/// Driver for SteelSeries mice.
#[derive(Clone, Copy, Debug)]
pub struct SteelSeriesDriver {
    gap: Duration,
}

impl SteelSeriesDriver {
    pub fn new() -> Self {
        Self { gap: DEFAULT_GAP }
    }

    /// Wait `gap` after every command instead of the default.
    pub fn with_gap(gap: Duration) -> Self {
        Self { gap }
    }
}

impl Default for SteelSeriesDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl Driver for SteelSeriesDriver {
    fn name(&self) -> &'static str {
        "SteelSeries"
    }

    fn vendor(&self) -> Vendor {
        Vendor::SteelSeries
    }

    /// Claims exactly the interface that takes commands, so each physical
    /// mouse is opened once (see [`models::Control::matches`]).
    fn probe(&self, iface: &HidInterfaceInfo) -> bool {
        models::for_interface(iface, cfg!(windows)).is_some()
    }

    fn open(
        &self,
        iface: &HidInterfaceInfo,
        backend: &dyn HidBackend,
    ) -> Result<Vec<Box<dyn Device>>> {
        let model = models::for_interface(iface, cfg!(windows))
            .ok_or(Error::Unsupported("not a SteelSeries control interface"))?;
        let io = backend.open(&iface.path)?;
        let gap=if model.pid==prime::PID&&!self.gap.is_zero(){self.gap.max(Duration::from_millis(30))}else{self.gap};
        let link = Link::new(io, model.spec.wire, gap);
        Ok(vec![Box::new(SteelSeriesDevice::new(model, link, iface))])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{iface, open_with};
    use gearwise_core::model::{Connection, DeviceKind};
    use gearwise_core::transport::{MockBackend, MockReply, MockTransport};
    use std::sync::Mutex;

    fn node(pid: u16, interface: i32, page: u16, usage: u16) -> HidInterfaceInfo {
        HidInterfaceInfo {
            interface_number: interface,
            usage_page: page,
            usage,
            ..iface(pid)
        }
    }

    #[test]
    fn probe_claims_one_node_of_an_aerox() {
        let d = SteelSeriesDriver::new();
        // An Aerox 3 as the Linux backend lists it: a node per interface.
        let nodes = [
            node(0x1836, 0, 0x0001, 0x0002),
            node(0x1836, 1, 0x0001, 0x0006),
            node(0x1836, 2, 0x000c, 0x0001),
            node(0x1836, 3, 0xffc0, 0x0001),
        ];
        assert_eq!(nodes.iter().filter(|n| d.probe(n)).count(), 1);
        assert!(d.probe(&nodes[3]));
        assert_eq!(d.name(), "SteelSeries");
        assert_eq!(d.vendor(), Vendor::SteelSeries);
    }

    #[test]
    fn probe_ignores_other_vendors_and_unknown_products() {
        let d = SteelSeriesDriver::new();
        assert!(!d.probe(&node(0x0001, 3, 0xffc0, 1)));
        let logitech = HidInterfaceInfo {
            vendor_id: 0x046d,
            ..node(0x1836, 3, 0xffc0, 1)
        };
        assert!(!d.probe(&logitech));
    }

    #[test]
    fn open_names_the_device_from_the_model_table() {
        let (dev, _) = open_with(0x1838, |_| MockReply::none());
        let desc = dev.descriptor();
        assert_eq!(desc.id, "steelseries:1838:SN1838");
        assert_eq!(desc.model_key, "steelseries:1838");
        assert_eq!(desc.name, "SteelSeries Aerox 3 Wireless");
        assert_eq!(desc.kind, DeviceKind::Mouse);
        assert_eq!(desc.connection, Connection::Wireless);
        assert_eq!(desc.serial.as_deref(), Some("SN1838"));
        assert_eq!((desc.vendor_id, desc.product_id), (0x1038, 0x1838));
        assert!(!desc.simulated);
    }

    #[test]
    fn open_falls_back_to_the_path_without_a_serial() {
        let info = HidInterfaceInfo {
            serial: None,
            ..iface(0x1824)
        };
        let slot = Mutex::new(Some(MockTransport::new(|_| MockReply::none())));
        let backend = MockBackend::new(vec![info.clone()], move |_| {
            Ok(Box::new(slot.lock().unwrap().take().expect("opened once")))
        });
        let devs = SteelSeriesDriver::new().open(&info, &backend).unwrap();
        assert_eq!(devs.len(), 1);
        assert_eq!(devs[0].descriptor().id, "steelseries:1824:hid-1824");
        assert_eq!(devs[0].descriptor().serial, None);
    }

    #[test]
    fn open_does_not_touch_the_mouse() {
        let (_, log) = open_with(0x1850, |_| MockReply::none());
        assert!(log.lock().unwrap().is_empty());
    }

    #[test]
    fn open_refuses_interfaces_it_would_not_claim() {
        let backend = MockBackend::new(vec![], |_| Err(Error::Disconnected));
        let wrong = node(0x1836, 1, 0x0001, 0x0006);
        assert!(matches!(
            SteelSeriesDriver::new().open(&wrong, &backend),
            Err(Error::Unsupported(_))
        ));
    }
}

#[cfg(feature="simulation")]
pub fn simulated_rival310()->(Box<dyn Device>,std::sync::Arc<std::sync::Mutex<Vec<gearwise_core::transport::MockRecord>>>){testutil::silent(0x1720)}

/// Pure preflight using the same encoders as the device. No transport is opened.
/// The bridge can reject an unsupported tail before earlier settings are sent.
pub fn validate_setting(pid:u16,setting:&gearwise_core::Setting)->Result<()>{
 use gearwise_core::Setting;
 let spec=models::find(pid).ok_or(Error::Unsupported("unknown SteelSeries model"))?.spec;
 match setting{
  Setting::VendorProfile{schema,data}=>{if pid==prime::PID{prime::plan(pid,schema,data)?;}else{rival3::plan(pid,schema,data)?;}},
  Setting::Dpi(want)=>{let(applied,_)=dpi::plan(&spec.dpi,want)?;if applied!=*want{return Err(Error::invalid("DPI must exactly match a native sensor step"));}},
  Setting::LiftOff(index) if pid==0x1724=>{proximity::packet(*index)?;},
  Setting::ReportRate(hz)=>{if !spec.rate.codes.iter().any(|(r,_)|r==hz){return Err(Error::invalid("native SteelSeries report rate"));}},
  Setting::Lighting{zone,state} if pid==prime::PID=>{prime::static_zone(&serde_json::to_value(prime::Profile::default()).unwrap(),*zone,state)?;},
  Setting::Button{id,action} if pid==prime::PID=>{prime::button_plan(&Default::default(),*id,action)?;},
  Setting::Lighting{zone,state}=>{let(applied,_)=light::plan(spec.light.as_ref().ok_or(Error::Unsupported("lighting"))?,*zone,state)?;if applied!=*state{return Err(Error::invalid("lighting period must match a native firmware rhythm"));}},
  Setting::Button{id,action}=>{buttons::plan(spec.buttons.as_ref().ok_or(Error::Unsupported("button table"))?,&std::collections::BTreeMap::new(),*id,action)?;},
  _=>return Err(Error::Unsupported("SteelSeries setting not enabled in bridge")),
 }Ok(())
}
#[cfg(feature="simulation")]
pub fn simulated_model(pid:u16)->Result<(Box<dyn Device>,std::sync::Arc<std::sync::Mutex<Vec<gearwise_core::transport::MockRecord>>>)>{
 if !matches!(pid,0x1710|0x1720|0x1722|0x1832|0x1724|0x1824|0x184c|0x182e){return Err(Error::Unsupported("unreviewed bridge SteelSeries simulation"));}Ok(testutil::silent(pid))
}
