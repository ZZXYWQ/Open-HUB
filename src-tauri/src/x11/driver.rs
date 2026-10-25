use super::*;
use gearwise_core::{
    model::*,
    transport::{BusKind, HidBackend, HidInterfaceInfo, MockRecord, MockReply, MockTransport},
    Capabilities, Device, DeviceDescriptor, DeviceState, Driver, Setting,
};
use std::sync::{Arc, Mutex};
struct X11Device {
    desc: DeviceDescriptor,
    caps: Capabilities,
    writer: Writer,
}
fn exact_collection(i: &HidInterfaceInfo) -> bool {
    // The pinned Windows HID implementation identifies the feature collection
    // as MI_02 / COL04. Do not copy its broad "first path" fallback, which can
    // claim the ordinary keyboard or mouse collection.
    supported(i.vendor_id, i.product_id)
        && i.bus == BusKind::Usb
        && i.interface_number == 2
        && i.path.to_ascii_lowercase().contains("col04")
}
impl X11Device {
    fn new(io: Box<dyn HidTransport>, i: &HidInterfaceInfo) -> Result<Self> {
        if !exact_collection(i) {
            return Err(invalid("不是已证实的 MI_02 / COL04 控制集合"));
        }
        Ok(Self {
            desc: DeviceDescriptor {
                id: format!("attackshark:{:04x}:{}", i.product_id, i.path),
                model_key: format!("attackshark:{:04x}", i.product_id),
                vendor: Vendor::Other,
                kind: DeviceKind::Mouse,
                name: format!(
                    "Attack Shark X11{}",
                    if i.product_id == WIRELESS {
                        " · 2.4G"
                    } else {
                        " · USB"
                    }
                ),
                vendor_id: VID,
                product_id: i.product_id,
                connection: if i.product_id == WIRELESS {
                    Connection::Wireless
                } else {
                    Connection::Wired
                },
                firmware: None,
                serial: i.serial.clone(),
                simulated: false,
            },
            caps: Capabilities {
                vendor_profile: Some(SCHEMA.into()),
                onboard_profiles: 1,
                ..Default::default()
            },
            writer: Writer::new(io, i.vendor_id, i.product_id)?,
        })
    }
}
impl Device for X11Device {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }
    fn capabilities(&self) -> &Capabilities {
        &self.caps
    }
    fn read_state(&mut self) -> Result<DeviceState> {
        // Absence means UNKNOWN, never a factory read. DPI/rate remain inside this
        // atomic profile so session refresh cannot append conflicting partial edits.
        Ok(DeviceState {
            vendor_profile: self
                .writer
                .cached()
                .map(serde_json::to_value)
                .transpose()
                .map_err(|_| invalid("缓存序列化失败"))?,
            ..Default::default()
        })
    }
    fn apply(&mut self, s: &Setting) -> Result<()> {
        if let Setting::VendorProfile { schema, data } = s {
            let p = decode(schema, data)?;
            self.writer.apply(&p)?;
            Ok(())
        } else {
            Err(invalid(
                "X11 必须明确应用完整方案；不使用示例补齐未知板载设置",
            ))
        }
    }
}
pub struct X11Driver;
impl Driver for X11Driver {
    fn name(&self) -> &'static str {
        "Attack Shark X11 native HID (experimental)"
    }
    fn vendor(&self) -> Vendor {
        Vendor::Other
    }
    fn probe(&self, i: &HidInterfaceInfo) -> bool {
        exact_collection(i)
    }
    fn open(&self, i: &HidInterfaceInfo, b: &dyn HidBackend) -> Result<Vec<Box<dyn Device>>> {
        if !self.probe(i) {
            return Err(invalid("错误的接口"));
        }
        Ok(vec![Box::new(X11Device::new(b.open(&i.path)?, i)?)])
    }
}
pub fn simulated(product: u16) -> Result<(Box<dyn Device>, Arc<Mutex<Vec<MockRecord>>>)> {
    let mock = MockTransport::new(|_| MockReply::none());
    let log = mock.log();
    let i = HidInterfaceInfo {
        vendor_id: VID,
        product_id: product,
        interface_number: 2,
        path: format!("simulation:mi_02:col04:{product:04x}"),
        bus: BusKind::Usb,
        ..Default::default()
    };
    let mut d = X11Device::new(Box::new(mock), &i)?;
    d.writer.pause = false;
    d.writer.cached = Some(Profile::example());
    d.desc.simulated = true;
    Ok((Box::new(d), log))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scan_only_claims_exact_vendor_control_collection_and_writes_nothing() {
        let valid = HidInterfaceInfo {
            vendor_id: VID,
            product_id: WIRED,
            interface_number: 2,
            path: "hid:mi_02:col04".into(),
            bus: BusKind::Usb,
            ..Default::default()
        };
        let driver = X11Driver;
        assert!(driver.probe(&valid));
        for i in [
            HidInterfaceInfo {
                product_id: 0xfa61,
                ..valid.clone()
            },
            HidInterfaceInfo {
                interface_number: 1,
                ..valid.clone()
            },
            HidInterfaceInfo {
                path: "hid:mi_02:col01".into(),
                ..valid.clone()
            },
            HidInterfaceInfo {
                bus: BusKind::Bluetooth,
                ..valid.clone()
            },
        ] {
            assert!(!driver.probe(&i));
        }
        let mock = MockTransport::new(|_| panic!("scan must never write or query"));
        let log = mock.log();
        let mut d = X11Device::new(Box::new(mock), &valid).unwrap();
        assert!(d.read_state().unwrap().vendor_profile.is_none());
        assert!(log.lock().unwrap().is_empty());
        assert!(d.apply(&Setting::ReportRate(500)).is_err());
        assert!(log.lock().unwrap().is_empty());
    }
}
