//! Corsair driver: DPI, polling rate, lift-off, angle snapping and static
//! colours for mice and keyboards.
//!
//! Two protocol families are supported, each in its own module pair:
//!
//! * **Bragi** (`bragi`, `bragi_dev`) — most models since ~2019, including
//!   mice behind Slipstream receivers. The control collection is usage page
//!   `0xFF42` usage 1 on USB interface 1.
//! * **NXP** (`nxp`, `nxp_dev`) — the older RGB K65/K70/K95/Strafe
//!   keyboards and M65/Scimitar/Harpoon-era mice. Usage page `0xFFC2`.
//!
//! Lighting is rendered by the host in software mode; only the Off and Static
//! effects are offered (no background animation thread). Brightness is applied
//! to the colours before they are sent.

mod art;
mod bragi;
mod bragi_dev;
mod common;
mod keymap;
mod models;
mod nxp;
mod nxp_dev;

#[cfg(any(test, feature = "simulation"))]
mod testing;

use gearwise_core::device::{Device, Driver};
use gearwise_core::error::{Error, Result};
use gearwise_core::model::{VID_CORSAIR, Vendor};
use gearwise_core::transport::{HidBackend, HidInterfaceInfo, share};

use crate::bragi::{Link, prop};
use crate::bragi_dev::BragiDevice;
use crate::models::{BragiSpec, CONTROL_INTERFACE, Model, Proto};
use crate::nxp_dev::NxpDevice;

/// Receivers use 64-byte packets.
const RECEIVER_PACKET: usize = 64;
/// Receiver sub-device slots.
const RECEIVER_SLOTS: u8 = 7;

/// Driver for Corsair mice and keyboards.
#[derive(Clone, Copy, Debug, Default)]
pub struct CorsairDriver;

impl Driver for CorsairDriver {
    fn name(&self) -> &'static str {
        "Corsair"
    }

    fn vendor(&self) -> Vendor {
        Vendor::Corsair
    }

    /// Claim exactly the control collection of a known model: every Corsair
    /// device also exposes keyboard/mouse HID interfaces and (Bragi) a second
    /// vendor collection for events, none of which speak the protocol.
    fn probe(&self, iface: &HidInterfaceInfo) -> bool {
        if iface.vendor_id != VID_CORSAIR {
            return false;
        }
        let Some(model) = models::find(iface.product_id) else {
            return false;
        };
        if !matches!(iface.interface_number, -1 | CONTROL_INTERFACE) {
            return false;
        }
        let (page, usage) = model.control_endpoint();
        if iface.usage_page == 0 {
            // The backend could not read the report descriptor; the interface number decides.
            return iface.interface_number == CONTROL_INTERFACE;
        }
        iface.usage_page == page && (usage == 0 || iface.usage == usage)
    }

    fn open(
        &self,
        iface: &HidInterfaceInfo,
        backend: &dyn HidBackend,
    ) -> Result<Vec<Box<dyn Device>>> {
        let model =
            models::find(iface.product_id).ok_or(Error::Unsupported("unknown Corsair product"))?;
        let transport = backend.open(&iface.path)?;
        match &model.proto {
            Proto::Nxp(spec) => Ok(vec![Box::new(NxpDevice::open(
                transport, model, spec, iface,
            )?)]),
            Proto::Bragi(spec) => {
                let link = Link::new(share(transport), 0, spec.packet);
                Ok(vec![Box::new(BragiDevice::open(
                    link, None, model, spec, iface,
                )?)])
            }
            Proto::Receiver => {
                let link = Link::new(share(transport), 0, RECEIVER_PACKET);
                Ok(open_receiver(&link, iface))
            }
        }
    }
}

/// Devices currently paired with and awake behind a receiver. Devices that
/// fail to open are skipped so one sleeping mouse does not hide the others.
fn open_receiver(receiver: &Link, iface: &HidInterfaceInfo) -> Vec<Box<dyn Device>> {
    let bits = match receiver.get(prop::SUBDEVICES) {
        Ok(bits) => bits,
        Err(e) => {
            log::warn!("receiver {}: cannot list devices: {e}", iface.path);
            return Vec::new();
        }
    };
    let mut devices: Vec<Box<dyn Device>> = Vec::new();
    for slot in 1..=RECEIVER_SLOTS {
        if bits >> slot & 1 == 0 {
            continue;
        }
        let link = receiver.sub_device(slot);
        let found = link
            .get(prop::PID)
            .ok()
            .and_then(|pid| u16::try_from(pid).ok())
            .and_then(models::find);
        let Some((model, spec)) = found.and_then(bragi_spec) else {
            log::debug!(
                "receiver {}: slot {slot} holds no supported device",
                iface.path
            );
            continue;
        };
        match BragiDevice::open(link, Some((receiver.clone(), slot)), model, spec, iface) {
            Ok(d) => devices.push(Box::new(d)),
            Err(e) => log::warn!("receiver {}: cannot open slot {slot}: {e}", iface.path),
        }
    }
    devices
}

fn bragi_spec(model: &'static Model) -> Option<(&'static Model, &'static BragiSpec)> {
    match &model.proto {
        Proto::Bragi(spec) => Some((model, spec)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeBragi, FakeNxp, iface};
    use gearwise_core::model::{Connection, DeviceKind};
    use gearwise_core::transport::MockBackend;

    fn node(pid: u16, interface: i32, page: u16, usage: u16) -> HidInterfaceInfo {
        HidInterfaceInfo {
            path: format!("/dev/hidraw-{interface}-{usage}"),
            vendor_id: 0x1b1c,
            product_id: pid,
            interface_number: interface,
            usage_page: page,
            usage,
            ..Default::default()
        }
    }

    fn backend_for(
        info: &HidInterfaceInfo,
        factory: impl Fn() -> Box<dyn gearwise_core::HidTransport> + Send + Sync + 'static,
    ) -> MockBackend {
        MockBackend::new(vec![info.clone()], move |_| Ok(factory()))
    }

    #[test]
    fn probe_claims_only_the_k55_pro_control_node() {
        // The three nodes of a K55 RGB PRO as the Linux backend lists them.
        let nodes = [
            node(0x1ba4, 0, 0x0001, 0x0006),
            node(0x1ba4, 1, 0xff42, 0x0001),
            node(0x1ba4, 2, 0xff42, 0x0002),
        ];
        let claimed: Vec<bool> = nodes.iter().map(|n| CorsairDriver.probe(n)).collect();
        assert_eq!(claimed, [false, true, false]);
    }

    #[test]
    fn probe_handles_windows_style_collections_and_foreign_devices() {
        let d = CorsairDriver;
        // Windows lists every collection separately; only the control one matches.
        assert!(d.probe(&node(0x1b9e, 1, 0xff42, 0x0001)));
        assert!(!d.probe(&node(0x1b9e, 1, 0xff42, 0x0002)));
        assert!(!d.probe(&node(0x1b9e, 0, 0x0001, 0x0002)));
        // Unknown product, other vendor, and a known model on the wrong interface.
        assert!(!d.probe(&node(0x0001, 1, 0xff42, 0x0001)));
        assert!(!d.probe(&HidInterfaceInfo {
            vendor_id: 0x046d,
            ..node(0x1b9e, 1, 0xff42, 0x0001)
        }));
        assert!(!d.probe(&node(0x1b9e, 3, 0xff42, 0x0001)));
        // Backend without usage info: interface 1 only.
        assert!(d.probe(&node(0x1b9e, 1, 0, 0)));
        assert!(!d.probe(&node(0x1b9e, 0, 0, 0)));
    }

    #[test]
    fn probe_uses_the_legacy_usage_page_for_nxp_models() {
        let d = CorsairDriver;
        assert!(d.probe(&node(0x1b49, 1, 0xffc2, 0x0001)));
        assert!(d.probe(&node(0x1b49, 1, 0xffc2, 0x0000)));
        assert!(!d.probe(&node(0x1b49, 1, 0xff42, 0x0001)));
        assert!(!d.probe(&node(0x1b49, 0, 0x0001, 0x0006)));
        assert!(d.probe(&node(0x1b5a, 1, 0xffc2, 0x0001)));
    }

    #[test]
    fn open_builds_a_bragi_mouse_with_the_documented_id() {
        let fake = FakeBragi::new(64);
        fake.set_raw(prop::FIRMWARE, &[1, 2, 3, 0]);
        fake.set_prop(prop::POLL_RATE, 4);
        let info = iface(0x1b9e, "/dev/hidraw4");
        let devices = CorsairDriver
            .open(&info, &backend_for(&info, fake.transport_factory()))
            .unwrap();
        assert_eq!(devices.len(), 1);
        let d = devices[0].descriptor();
        assert_eq!(d.id, "corsair:1b9e:SN1B9E");
        assert_eq!(d.model_key, "corsair:1b9e");
        assert_eq!(d.kind, DeviceKind::Mouse);
        assert_eq!(d.connection, Connection::Wired);
        assert_eq!(d.firmware.as_deref(), Some("1.2.3"));
        assert_eq!(d.name, "Corsair M65 RGB Ultra");
        assert!(!d.simulated);
    }

    #[test]
    fn id_falls_back_to_the_path_without_a_serial() {
        let fake = FakeBragi::new(64);
        fake.set_raw(prop::FIRMWARE, &[1, 0, 0, 0]);
        let info = HidInterfaceInfo {
            serial: None,
            ..iface(0x1ba4, "/dev/hidraw4")
        };
        let devices = CorsairDriver
            .open(&info, &backend_for(&info, fake.transport_factory()))
            .unwrap();
        assert_eq!(devices[0].descriptor().id, "corsair:1ba4:/dev/hidraw4");
    }

    fn receiver_fake(slots: u32, pid: u32) -> FakeBragi {
        let fake = FakeBragi::new(64);
        fake.set_prop(prop::SUBDEVICES, slots);
        fake.set_raw(prop::FIRMWARE, &[1, 2, 3, 0]);
        fake.set_prop(prop::POLL_RATE, 4);
        fake.set_prop(prop::DPI_X, 1000);
        fake.set_prop(prop::DPI_Y, 1000);
        fake.set_prop(prop::BATTERY_LEVEL, 500);
        fake.set_prop(prop::BATTERY_STATUS, 1);
        fake.set_prop(prop::ANGLE_SNAP, 1);
        // The fake answers PID alike for every slot.
        fake.set_prop(prop::PID, pid);
        fake
    }

    #[test]
    fn receiver_exposes_each_awake_device_with_its_slot() {
        // Slots 1 and 3 awake, both Ironclaw Wireless.
        let fake = receiver_fake(0b0000_1010, 0x1b4c);
        let info = iface(0x1ba6, "/dev/hidraw7");
        let devices = CorsairDriver
            .open(&info, &backend_for(&info, fake.transport_factory()))
            .unwrap();
        let ids: Vec<&str> = devices.iter().map(|d| d.descriptor().id.as_str()).collect();
        assert_eq!(ids, ["corsair:1b4c:SN1BA6:1", "corsair:1b4c:SN1BA6:3"]);
        assert!(
            devices
                .iter()
                .all(|d| d.descriptor().connection == Connection::Wireless)
        );
        // Packets to slot 3 carry 0x08 | 3 in the magic byte.
        assert!(
            fake.writes()
                .iter()
                .any(|w| w[1] == 0x0b && w[2] == 0x02 && w[3] == prop::PID)
        );
    }

    #[test]
    fn receiver_skips_devices_it_does_not_know() {
        let fake = receiver_fake(0b0000_0010, 0x0001);
        let info = iface(0x1ba6, "/dev/hidraw7");
        assert!(
            CorsairDriver
                .open(&info, &backend_for(&info, fake.transport_factory()))
                .unwrap()
                .is_empty()
        );
        // A receiver is not a device behind a receiver.
        let fake = receiver_fake(0b0000_0010, 0x1ba6);
        assert!(
            CorsairDriver
                .open(&info, &backend_for(&info, fake.transport_factory()))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn receiver_with_nothing_awake_yields_no_devices() {
        let fake = receiver_fake(0, 0);
        let info = iface(0x1ba6, "/dev/hidraw7");
        assert!(
            CorsairDriver
                .open(&info, &backend_for(&info, fake.transport_factory()))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn open_reports_unresponsive_devices_as_errors() {
        let fake = FakeBragi::new(64);
        fake.mute();
        let info = iface(0x1b9e, "/dev/hidraw4");
        let result = CorsairDriver.open(&info, &backend_for(&info, fake.transport_factory()));
        assert!(matches!(result, Err(Error::Timeout)));
    }

    #[test]
    fn open_builds_an_nxp_keyboard() {
        let fake = FakeNxp::new();
        fake.ident_for_keyboard(0x0205, 1);
        let info = iface(0x1b49, "/dev/hidraw2");
        let devices = CorsairDriver
            .open(&info, &backend_for(&info, fake.transport_factory()))
            .unwrap();
        let d = devices[0].descriptor();
        assert_eq!(d.id, "corsair:1b49:SN1B49");
        assert_eq!(d.firmware.as_deref(), Some("2.05"));
        assert_eq!(d.kind, DeviceKind::Keyboard);
    }
}

#[cfg(feature="simulation")]
pub fn simulated_m65_pro()->(Box<dyn Device>,std::sync::Arc<std::sync::Mutex<Vec<gearwise_core::transport::MockRecord>>>){
 simulated_m65_pro_at_rate(1000).expect("valid simulated rate")
}
#[cfg(feature="simulation")]
pub fn simulated_m65_pro_at_rate(rate:u16)->gearwise_core::Result<(Box<dyn Device>,std::sync::Arc<std::sync::Mutex<Vec<gearwise_core::transport::MockRecord>>>)>{
 if ![125,250,500,1000].contains(&rate){return Err(gearwise_core::Error::invalid("unsupported simulated rate"));}
 let fake=testing::FakeNxp::new();fake.ident_for_mouse(0x0206,(1000/rate) as u8,0x56);fake.mouse_profile(0b1111,2,3,1,&[(0,400,400),(1,800,800),(2,1600,1600),(3,3200,3200)]);
 let model=models::find(0x1b2e).unwrap();let models::Proto::Nxp(spec)=&model.proto else{unreachable!()};
 let log=fake.operation_log();Ok((Box::new(nxp_dev::NxpDevice::open(fake.transport(),model,spec,&testing::iface(0x1b2e,"simulation:corsair"))?),log))
}
