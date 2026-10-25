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
    battery: battery::BatteryInput,
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
                battery: i.product_id == WIRELESS,
                onboard_profiles: 1,
                ..Default::default()
            },
            writer: Writer::new(io, i.vendor_id, i.product_id)?,
            battery: Default::default(),
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
            battery: self.battery.poll(Instant::now()).map(|percent| Battery {
                percent: Some(percent),
                charging: false,
            }),
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
        open_with_container(i, b, crate::hid_container::id)
    }
}
fn open_with_container(
    i: &HidInterfaceInfo,
    b: &dyn HidBackend,
    container: impl FnMut(&str) -> Option<[u8; 16]>,
) -> Result<Vec<Box<dyn Device>>> {
    if !exact_collection(i) {
        return Err(invalid("错误的接口"));
    }
    let mut device = X11Device::new(b.open(&i.path)?, i)?;
    if i.product_id == WIRELESS {
        if let Ok(all) = b.enumerate() {
            if let Some(input) = battery::choose_input(i, &all, container) {
                // Missing/inaccessible input leaves battery UNKNOWN, without
                // disabling the independent feature-output collection.
                device.battery.io = b.open(&input.path).ok();
            }
        }
    }
    Ok(vec![Box::new(device)])
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
    #[test]
    fn opened_driver_pairs_only_its_battery_collection_and_never_sends_a_battery_query() {
        use gearwise_core::transport::MockBackend;
        let control = |unit: &str| HidInterfaceInfo {
            vendor_id: VID,
            product_id: WIRELESS,
            interface_number: 2,
            path: format!("{unit}:mi_02:col04"),
            bus: BusKind::Usb,
            ..Default::default()
        };
        let input = |unit: &str| HidInterfaceInfo {
            path: format!("{unit}:input"),
            usage_page: 0x0a,
            ..control(unit)
        };
        let a = control("A");
        let b = control("B");
        let opened = Arc::new(Mutex::new(vec![]));
        let seen = opened.clone();
        let backend = MockBackend::new(
            vec![input("B"), input("A"), a.clone(), b.clone()],
            move |path| {
                let mut transport =
                    MockTransport::new(|_| panic!("read_state must never send control reports"));
                if path.ends_with(":input") {
                    transport.push_input(vec![
                        3,
                        0x55,
                        0x40,
                        1,
                        if path.starts_with('A') { 80 } else { 20 },
                    ]);
                }
                seen.lock().unwrap().push(path.to_string());
                Ok(Box::new(transport))
            },
        );
        let group = |path: &str| {
            Some(if path.starts_with('A') {
                [1; 16]
            } else {
                [2; 16]
            })
        };
        let mut da = open_with_container(&a, &backend, group).unwrap().remove(0);
        let mut db = open_with_container(&b, &backend, group).unwrap().remove(0);
        assert_eq!(da.read_state().unwrap().battery.unwrap().percent, Some(80));
        assert_eq!(db.read_state().unwrap().battery.unwrap().percent, Some(20));
        assert_eq!(
            *opened.lock().unwrap(),
            vec![
                a.path.clone(),
                "A:input".into(),
                b.path.clone(),
                "B:input".into()
            ]
        );
        assert!(da.read_state().unwrap().vendor_profile.is_none());
        opened.lock().unwrap().clear();
        let mut unknown = open_with_container(&a, &backend, |_| None)
            .unwrap()
            .remove(0);
        assert!(unknown.read_state().unwrap().battery.is_none());
        assert_eq!(*opened.lock().unwrap(), vec![a.path]);
    }
}
