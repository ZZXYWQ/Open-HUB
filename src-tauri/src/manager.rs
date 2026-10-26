//! Device enumeration: finds Logitech HID++ collections, probes receiver
//! slots, and keeps transports open across rescans.

use std::collections::HashMap;
use std::ffi::CString;
use std::time::Duration;

use hidapi::HidApi;

use crate::device::Device;
use crate::hidpp::{
    Error, EventSink, Result, Transport, DIRECT, HIDPP_LONG_USAGE, HIDPP_SHORT_USAGE,
    HIDPP_USAGE_PAGE, LOGITECH_VID,
};

/// Unifying, Nano, Lightspeed and Bolt receivers.
const RECEIVER_PIDS: &[u16] = &[
    0xC517, 0xC52B, 0xC52F, 0xC531, 0xC532, 0xC534, 0xC539, 0xC53A, 0xC53D, 0xC53F, 0xC541, 0xC545,
    0xC547, 0xC548, 0xC54D,
];
/// Paired-device slots on a receiver.
const RECEIVER_SLOTS: std::ops::RangeInclusive<u8> = 1..=6;
/// HID++ 1.0 receiver register 0x00 bits: wireless (device connection)
/// notifications and "software present", which G HUB's agent also sets.
const NOTIFY_WIRELESS: u8 = 0x01;
const NOTIFY_SOFTWARE_PRESENT: u8 = 0x08;
const REGISTER_TIMEOUT: Duration = Duration::from_millis(500);

fn is_receiver(product_id: u16, product: &str) -> bool {
    RECEIVER_PIDS.contains(&product_id) || product.to_ascii_lowercase().contains("receiver")
}

/// Key shared by the short and long collections of one HID++ interface:
/// `...&mi_02&col01#9&26fbe7a5&0&0000#{guid}` and `...&col02#...&0001#{guid}`
/// both become `...&mi_02#9&26fbe7a5&0`.
fn interface_key(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    let mut key = match lower.find("&col") {
        Some(i) => format!("{}{}", &lower[..i], &lower[(i + 6).min(lower.len())..]),
        None => lower,
    };
    if let Some(i) = key.find("#{") {
        key.truncate(i);
    }
    if let Some(i) = key.rfind('&') {
        key.truncate(i);
    }
    key
}

struct Collection {
    mchose: bool,
    long: CString,
    short: Option<CString>,
    product_id: u16,
    product: String,
}

pub struct Manager {
    api: HidApi,
    events: EventSink,
    transports: HashMap<CString, Transport>,
    devices: Vec<Device>,
}

impl Manager {
    pub fn new(events: EventSink) -> Result<Self> {
        Ok(Self {
            api: HidApi::new()?,
            events,
            transports: HashMap::new(),
            devices: Vec::new(),
        })
    }

    pub fn devices(&self) -> &[Device] {
        &self.devices
    }

    /// Device addressed by a notification: its transport path and device index.
    pub fn device_at(&self, path: &CString, index: u8) -> Option<&Device> {
        self.devices
            .iter()
            .find(|d| &d.path == path && d.index == index)
    }

    fn collections(&self) -> Vec<Collection> {
        let hidpp = |usage: u16| {
            self.api.device_list().filter(move |d| {
                d.vendor_id() == LOGITECH_VID
                    && d.usage_page() == HIDPP_USAGE_PAGE
                    && d.usage() == usage
            })
        };
        let shorts: HashMap<String, CString> = hidpp(HIDPP_SHORT_USAGE)
            .map(|d| {
                (
                    interface_key(&d.path().to_string_lossy()),
                    d.path().to_owned(),
                )
            })
            .collect();
        let mut out: Vec<Collection> = hidpp(HIDPP_LONG_USAGE)
            .map(|d| Collection {
                mchose: false,
                long: d.path().to_owned(),
                short: shorts
                    .get(&interface_key(&d.path().to_string_lossy()))
                    .cloned(),
                product_id: d.product_id(),
                product: d.product_string().unwrap_or("Logitech device").to_string(),
            })
            .collect();
        out.extend(
            self.api
                .device_list()
                .filter(|d| {
                    crate::mchose::known_interface(d.vendor_id(),d.product_id(),d.usage_page(),d.usage(),d.product_string())
                })
                .map(|d| Collection {
                    mchose: true,
                    long: d.path().to_owned(),
                    short: None,
                    product_id: d.product_id(),
                    product: d.product_string().unwrap_or_default().to_string(),
                }),
        );
        out
    }

    /// Asks a receiver to report device connections, keeping its other flags.
    fn enable_receiver_notifications(transport: &Transport) -> Result<()> {
        let flags = transport.register(0x81, 0x00, [0; 3], REGISTER_TIMEOUT)?;
        let wanted = NOTIFY_WIRELESS | NOTIFY_SOFTWARE_PRESENT;
        if flags[1] & wanted != wanted {
            transport.register(
                0x80,
                0x00,
                [flags[0], flags[1] | wanted, flags[2]],
                REGISTER_TIMEOUT,
            )?;
        }
        Ok(())
    }

    /// Re-enumerates HID devices. Known devices that still answer a ping are
    /// kept as-is; new ones are probed. Returns non-fatal warnings.
    pub fn scan(&mut self) -> Result<Vec<String>> {
        self.api.refresh_devices()?;
        let collections = self.collections();

        let mut warnings = Vec::new();
        self.transports
            .retain(|path, t| t.is_alive() && collections.iter().any(|c| &c.long == path));

        let retained_paths = self.transports.keys().cloned().collect::<std::collections::HashSet<_>>();
        for c in &collections {
            if self.transports.contains_key(&c.long) {
                continue;
            }
            let receiver = !c.mchose && is_receiver(c.product_id, &c.product);
            let short = receiver.then_some(c.short.as_deref()).flatten();
            let opened = if c.mchose {
                Transport::open_mchose(&self.api, &c.long, self.events.clone())
            } else {
                Transport::open(&self.api, &c.long, short, self.events.clone())
            };
            match opened {
                Ok(t) => {
                    if receiver {
                        if let Err(e) = Self::enable_receiver_notifications(&t) {
                            warnings.push(format!(
                                "{}: could not enable notifications ({e})",
                                c.product
                            ));
                        }
                    }
                    self.transports.insert(c.long.clone(), t);
                }
                Err(e) => warnings.push(format!("{}: could not open ({e})", c.product)),
            }
        }

        let mut found = Vec::new();
        for c in &collections {
            let Some(transport) = self.transports.get(&c.long) else {
                continue;
            };
            if c.mchose {
                match transport.mchose().expect("MCHOSE transport").identity() {
                    Ok(_) => found.push(Device {
                        vendor_id: crate::mchose::VID,
                        connection_key: self.devices.iter().find(|d| d.path == c.long && retained_paths.contains(&c.long)).map(|d| d.connection_key.clone()).unwrap_or_else(|| format!("connection:{}",crate::profiles::new_id())),
                        profile_key: self.devices.iter().find(|d| d.path == c.long && retained_paths.contains(&c.long)).map(|d| d.profile_key.clone()).unwrap_or_else(|| format!("session:{}", crate::profiles::new_id())),
                        id: format!("{}#{DIRECT}", c.long.to_string_lossy()),
                        path: c.long.clone(),
                        index: DIRECT,
                        product_id: c.product_id,
                        product: c.product.clone(),
                        name: "MCHOSE A7 V3 Ultra+".into(),
                        firmware_name: c.product.clone(),
                        model_ids: vec![crate::mchose::MODEL_PID],
                        kind: "Mouse",
                        protocol: "MCHOSE V3 (0x4D)".into(),
                        features: Default::default(),
                    }),
                    Err(e) => warnings.push(format!("{}: {e}", c.product)),
                }
                continue;
            }
            let slots: Vec<u8> = if is_receiver(c.product_id, &c.product) {
                RECEIVER_SLOTS.collect()
            } else {
                vec![DIRECT]
            };
            for index in slots {
                let id = format!("{}#{index}", c.long.to_string_lossy());
                if let Some(known) = self.devices.iter().find(|d| d.id == id && retained_paths.contains(&c.long)) {
                    if !known.ping(transport) { continue; }
                    let same_unit = if known.profile_key.starts_with("unit:") {
                        known.call_unpadded(transport, crate::device::feature::DEVICE_INFO, 0, &[]).ok()
                            .and_then(|r| crate::profile_routing::logitech_unit(&r)).as_ref() == Some(&known.profile_key)
                    } else { true };
                    if same_unit { found.push(known.clone()); continue; }
                    // A receiver slot can be re-paired without its USB path changing.
                    // Discover the new mouse instead of copying the previous unit's key.
                }
                match Device::probe(
                    transport,
                    id,
                    c.long.clone(),
                    index,
                    c.product_id,
                    c.product.clone(),
                ) {
                    Ok(device) => found.push(device),
                    // Empty receiver slots and sleeping devices simply don't answer.
                    Err(e @ (Error::Timeout | Error::Hidpp10(_))) => {
                        if crate::events::logging() {
                            eprintln!("[scan] {} slot {index:#04x}: {e}", c.product);
                        }
                    }
                    Err(e) => warnings.push(format!("{} (slot {index:#04x}): {e}", c.product)),
                }
            }
        }
        self.devices = found;
        Ok(warnings)
    }

    pub fn with_device<R>(
        &self,
        id: &str,
        f: impl FnOnce(&Device, &Transport) -> Result<R>,
    ) -> Result<R> {
        let device = self
            .devices
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| Error::Other("device is no longer connected; rescan".into()))?;
        let transport = self
            .transports
            .get(&device.path)
            .ok_or_else(|| Error::Other("device handle was closed; rescan".into()))?;
        f(device, transport)
    }
}

#[cfg(test)]
mod tests {
    use super::interface_key;

    #[test]
    fn pairs_short_and_long_collections() {
        let short = r"\\?\hid#vid_046d&pid_c547&mi_02&col01#9&26fbe7a5&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}";
        let long = r"\\?\HID#VID_046D&PID_C547&MI_02&COL02#9&26FBE7A5&0&0001#{4d1e55b2-f16f-11cf-88cb-001111000030}";
        assert_eq!(interface_key(short), interface_key(long));
        let other = r"\\?\hid#vid_046d&pid_c547&mi_02&col02#9&99999999&0&0001#{4d1e55b2-f16f-11cf-88cb-001111000030}";
        assert_ne!(interface_key(short), interface_key(other));
    }
}
