//! A Bragi mouse or keyboard as a [`Device`].
//!
//! Sensor settings are plain properties. Lighting is host-rendered: the first
//! lighting write switches the device to software mode, opens the lighting
//! resource and streams a colour buffer; a keep-alive from [`Device::poll_events`]
//! stops the firmware falling back to its own lighting.

use std::time::{Duration, Instant};

use gearwise_core::device::{
    Capabilities, Device, DeviceDescriptor, DeviceEvent, DeviceState, Setting,
};
use gearwise_core::error::{Error, Result};
use gearwise_core::model::{Battery, Connection, DpiStage, DpiState, Rgb};
use gearwise_core::transport::HidInterfaceInfo;

use crate::bragi::{self, Link, prop};
use crate::common::{self, RATES_HZ, optional};
use crate::keymap;
use crate::models::{BragiSpec, DPI_STEP, KEYS_ZONE, Model};

const PING_EVERY: Duration = Duration::from_secs(20);
const CHECK_EVERY: Duration = Duration::from_secs(5);
const BATTERY_EVERY: Duration = Duration::from_secs(30);

/// Key id that is never in the per-key map: asks for the base colour.
const NO_KEY: u16 = u16::MAX;

/// How the colour buffer is laid out on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LightPath {
    /// Resource 0x01: all reds, then all greens, then all blues.
    Planar,
    /// Resource 0x22: `12 00` then R, G, B per LED (K100-class keyboards).
    Triplets,
}

/// The receiver a wireless device is reached through.
struct Receiver {
    link: Link,
    /// Sub-device slot, 1..=7.
    index: u8,
}

pub(crate) struct BragiDevice {
    link: Link,
    receiver: Option<Receiver>,
    desc: DeviceDescriptor,
    caps: Capabilities,
    spec: &'static BragiSpec,
    state: DeviceState,
    light: Option<LightPath>,
    connected: bool,
    last_ping: Instant,
    last_check: Instant,
    last_battery: Instant,
}

impl BragiDevice {
    /// Open a device reachable through `link`. `receiver` is the dongle link
    /// and slot for wireless devices.
    pub(crate) fn open(
        link: Link,
        receiver: Option<(Link, u8)>,
        model: &'static Model,
        spec: &'static BragiSpec,
        iface: &HidInterfaceInfo,
    ) -> Result<Self> {
        let firmware = optional(firmware(&link))?;
        let (connection, index) = match &receiver {
            Some((_, i)) => (Connection::Wireless, Some(*i)),
            None => (Connection::Wired, None),
        };
        // Highest rate the firmware offers; 1 kHz when it does not say.
        let max = optional(link.get(prop::MAX_POLL_RATE))?
            .filter(|v| (1..=7).contains(v))
            .unwrap_or(4);
        let mut caps = Capabilities {
            dpi: spec.dpi.as_ref().map(common::dpi_caps),
            report_rates: RATES_HZ[..max as usize].to_vec(),
            lighting: if spec.keys.is_some() {
                vec![common::zone_caps(KEYS_ZONE, "Keys")]
            } else {
                common::zones_caps(spec.zones)
            },
            per_key: spec.keys.map(|l| l.build()),
            battery: spec.battery,
            lift_off: spec
                .lift
                .iter()
                .map(|(label, _)| label.to_string())
                .collect(),
            angle_snapping: spec.angle_snap,
            ..Default::default()
        };
        if let Some(art) = crate::art::physical(model.pid) {
            for (zone, keys) in art.zones {
                if let Some(z) = caps.lighting.iter_mut().find(|z| z.id == zone) {
                    z.keys = keys;
                }
            }
            caps.layout = Some(art.layout);
        }
        let now = Instant::now();
        let mut dev = Self {
            link,
            receiver: receiver.map(|(link, index)| Receiver { link, index }),
            desc: common::describe(model, iface, index, connection, firmware),
            caps,
            spec,
            state: DeviceState::default(),
            light: None,
            connected: true,
            last_ping: now,
            last_check: now,
            last_battery: now,
        };
        dev.refresh()?;
        Ok(dev)
    }

    /// Re-read everything the firmware exposes into the cached state.
    fn refresh(&mut self) -> Result<()> {
        let rate = optional(self.link.get(prop::POLL_RATE))?;
        self.state.report_rate = rate.and_then(rate_hz).or(self.state.report_rate);
        if let Some(live) = self.read_dpi()? {
            self.adopt_dpi(live);
        }
        if self.spec.battery {
            self.state.battery = self.read_battery()?.or(self.state.battery);
        }
        if self.spec.angle_snap {
            let snap = optional(self.link.get(prop::ANGLE_SNAP))?;
            self.state.angle_snapping = snap.map(|v| v != 0).or(self.state.angle_snapping);
        }
        if !self.spec.lift.is_empty() {
            let lift = optional(self.link.get(prop::LIFT_OFF))?;
            let index = lift.and_then(|v| {
                self.spec
                    .lift
                    .iter()
                    .position(|(_, value)| u32::from(*value) == v)
            });
            self.state.lift_off = index.map(|i| i as u8).or(self.state.lift_off);
        }
        Ok(())
    }

    /// The sensor's current resolution, if the model has DPI control.
    fn read_dpi(&self) -> Result<Option<DpiStage>> {
        let Some(spec) = &self.spec.dpi else {
            return Ok(None);
        };
        let value = |v: u32| u16::try_from(v).unwrap_or(u16::MAX);
        if spec.separate_xy {
            let x = optional(self.link.get(prop::DPI_X))?;
            let y = optional(self.link.get(prop::DPI_Y))?;
            Ok(x.zip(y).map(|(x, y)| DpiStage {
                x: value(x),
                y: value(y),
            }))
        } else {
            Ok(optional(self.link.get(prop::DPI))?.map(|v| DpiStage::uniform(value(v))))
        }
    }

    /// Make the cached stage table agree with what the sensor reports (the
    /// user may have pressed the mouse's DPI button). Returns true on change.
    fn adopt_dpi(&mut self, live: DpiStage) -> bool {
        match self.state.dpi.as_mut() {
            Some(d) if d.current() == Some(live) => false,
            Some(d) => {
                match d.stages.iter().position(|s| *s == live) {
                    Some(i) => d.active = i as u8,
                    None => {
                        *d = DpiState {
                            stages: vec![live],
                            active: 0,
                        }
                    }
                }
                true
            }
            None => {
                self.state.dpi = Some(DpiState {
                    stages: vec![live],
                    active: 0,
                });
                true
            }
        }
    }

    fn read_battery(&self) -> Result<Option<Battery>> {
        let Some(level) = optional(self.link.get(prop::BATTERY_LEVEL))? else {
            return Ok(None);
        };
        // 1 charging, 2 discharging, 3 fully charged.
        let status = optional(self.link.get(prop::BATTERY_STATUS))?.unwrap_or(2);
        Ok(Some(Battery {
            percent: Some((level / 10).min(100) as u8),
            charging: status == 1,
        }))
    }

    /// Push a stage's resolution to the sensor.
    fn write_dpi(&mut self, stage: DpiStage) -> Result<()> {
        let spec = self.spec.dpi.as_ref().ok_or(Error::Unsupported("dpi"))?;
        let mut status = if spec.separate_xy {
            self.link.set_status(prop::DPI_X, stage.x.into())?
        } else {
            self.link.set_status(prop::DPI, stage.x.into())?
        };
        if spec.separate_xy {
            status |= self.link.set_status(prop::DPI_Y, stage.y.into())?;
        }
        if status != 0 {
            // Some firmware answers with an error although the value took
            // effect, so judge by what the sensor reports.
            let applied = self.read_dpi()?.is_some_and(|l| {
                l.x.abs_diff(stage.x) <= DPI_STEP && l.y.abs_diff(stage.y) <= DPI_STEP
            });
            if !applied {
                return bragi::check(status, "set DPI");
            }
        }
        Ok(())
    }

    // --- lighting --------------------------------------------------------

    /// Take over lighting from the firmware (idempotent).
    fn ensure_software(&mut self) -> Result<()> {
        if self.light.is_some() {
            return Ok(());
        }
        self.link.set(prop::MODE, bragi::MODE_SOFTWARE)?;
        let path = self.open_lighting()?;
        self.restore_brightness()?;
        self.light = Some(path);
        self.last_ping = Instant::now();
        Ok(())
    }

    fn open_lighting(&self) -> Result<LightPath> {
        match self.open_resource(bragi::RES_LIGHTING)? {
            0 => Ok(LightPath::Planar),
            // Newer keyboards reject the plain resource and want the alternative.
            bragi::STATUS_INVALID | bragi::STATUS_NO_RESOURCE => {
                match self.open_resource(bragi::RES_LIGHTING_ALT)? {
                    0 => Ok(LightPath::Triplets),
                    s => Err(bragi::status_error(s, "open lighting")),
                }
            }
            s => Err(bragi::status_error(s, "open lighting")),
        }
    }

    fn open_resource(&self, resource: u16) -> Result<u8> {
        let status = self.link.open(bragi::LIGHTING_HANDLE, resource)?;
        if status == bragi::STATUS_BUSY {
            // Left open by an earlier session.
            self.link.close(bragi::LIGHTING_HANDLE)?;
            return self.link.open(bragi::LIGHTING_HANDLE, resource);
        }
        Ok(status)
    }

    /// Zone brightness is applied in software, so make sure the hardware
    /// brightness (kept across power cycles) is not dimming on top of it.
    fn restore_brightness(&self) -> Result<()> {
        if let Some(level) = optional(self.link.get(prop::BRIGHTNESS))? {
            if level < 1000 {
                optional(self.link.set(prop::BRIGHTNESS, 1000))?;
            }
        } else if let Some(level) = optional(self.link.get(prop::BRIGHTNESS_COARSE))? {
            if level < 3 {
                optional(self.link.set(prop::BRIGHTNESS_COARSE, 3))?;
            }
        }
        Ok(())
    }

    fn write_lighting(&self, state: &DeviceState) -> Result<()> {
        let path = self
            .light
            .ok_or_else(|| Error::protocol("lighting not open"))?;
        self.link
            .write(bragi::LIGHTING_HANDLE, &render(self.spec, state, path))
    }

    /// Render `state` to the device, re-taking the handle once if the
    /// firmware dropped it (it falls back to hardware mode when idle).
    fn push_lighting(&mut self, state: &DeviceState) -> Result<()> {
        self.ensure_software()?;
        match self.write_lighting(state) {
            Err(Error::Protocol(why)) => {
                log::debug!(
                    "{}: lighting write failed ({why}), reopening",
                    self.desc.name
                );
                self.light = None;
                self.ensure_software()?;
                self.write_lighting(state)
            }
            r => r,
        }
    }

    // --- events ----------------------------------------------------------

    /// Re-apply what the host had set after a wireless device woke up.
    fn restore_after_reconnect(&mut self) {
        let state = self.state.clone();
        if !state.lighting.is_empty() || !state.per_key.is_empty() {
            if let Err(e) = self.push_lighting(&state) {
                log::debug!("{}: restoring lighting failed: {e}", self.desc.name);
            }
        }
        if let Some(stage) = state.dpi.as_ref().and_then(DpiState::current) {
            if let Err(e) = self.write_dpi(stage) {
                log::debug!("{}: restoring DPI failed: {e}", self.desc.name);
            }
        }
    }

    fn check(&mut self, events: &mut Vec<DeviceEvent>) {
        if let Some(rx) = &self.receiver {
            if let Ok(bits) = rx.link.get(prop::SUBDEVICES) {
                let present = bits >> rx.index & 1 == 1;
                if present != self.connected {
                    self.connected = present;
                    if present {
                        self.light = None;
                        self.restore_after_reconnect();
                        events.push(DeviceEvent::Reconnected);
                        events.push(DeviceEvent::StateChanged);
                    } else {
                        events.push(DeviceEvent::Disconnected);
                    }
                }
            }
        }
        if !self.connected {
            return;
        }
        if let Ok(Some(live)) = self.read_dpi() {
            if self.adopt_dpi(live) {
                events.push(DeviceEvent::StateChanged);
            }
        }
    }

    fn validate(&self, setting: &Setting) -> Result<()> {
        match setting {
            Setting::Dpi(d) => {
                common::check_dpi(self.spec.dpi.as_ref().ok_or(Error::Unsupported("dpi"))?, d)
            }
            Setting::DpiActiveStage(i) => {
                let stages = self
                    .state
                    .dpi
                    .as_ref()
                    .ok_or(Error::Unsupported("dpi"))?
                    .stages
                    .len();
                if (*i as usize) < stages {
                    Ok(())
                } else {
                    Err(Error::invalid("active stage out of range"))
                }
            }
            Setting::ReportRate(hz) => {
                if self.caps.report_rates.contains(hz) {
                    Ok(())
                } else {
                    Err(Error::invalid(format!("{hz} Hz")))
                }
            }
            Setting::Lighting { zone, state } => {
                if !self.caps.lighting.iter().any(|z| z.id == *zone) {
                    return Err(Error::invalid(format!("lighting zone {zone}")));
                }
                common::check_zone_state(state)
            }
            Setting::PerKey(keys) => {
                let layout = self
                    .caps
                    .per_key
                    .as_ref()
                    .ok_or(Error::Unsupported("per-key lighting"))?;
                match keys.keys().find(|k| !layout.contains(**k)) {
                    Some(k) => Err(Error::invalid(format!(
                        "key id {k:#x} is not on this keyboard"
                    ))),
                    None => Ok(()),
                }
            }
            Setting::LiftOff(i) => {
                if self.spec.lift.is_empty() {
                    Err(Error::Unsupported("lift-off distance"))
                } else if (*i as usize) < self.spec.lift.len() {
                    Ok(())
                } else {
                    Err(Error::invalid("lift-off choice"))
                }
            }
            Setting::AngleSnapping(_) if self.spec.angle_snap => Ok(()),
            Setting::AngleSnapping(_) => Err(Error::Unsupported("angle snapping")),
            Setting::Button { .. } => Err(Error::Unsupported("button remapping")),
            Setting::OnboardProfile(_) => Err(Error::Unsupported("onboard profiles")),
            Setting::GameMode(_) => Err(Error::Unsupported("game mode")),
        }
    }
}

fn rate_hz(property_value: u32) -> Option<u16> {
    let index = usize::try_from(property_value).ok()?.checked_sub(1)?;
    RATES_HZ.get(index).copied()
}

fn firmware(link: &Link) -> Result<String> {
    let d = link.get_raw(prop::FIRMWARE)?;
    if d.len() < 4 {
        return Err(Error::protocol("short firmware reply"));
    }
    Ok(format!(
        "{}.{}.{}",
        d[0],
        d[1],
        u16::from_le_bytes([d[2], d[3]])
    ))
}

/// Build the colour buffer for the whole device.
fn render(spec: &BragiSpec, state: &DeviceState, path: LightPath) -> Vec<u8> {
    let mut rgb = vec![Rgb::BLACK; spec.leds as usize];
    if spec.keys.is_some() {
        // LEDs that are not letter keys (logo, volume wheel, ...) follow the base colour.
        rgb.fill(common::key_color(state, KEYS_ZONE, NO_KEY));
        for id in state.per_key.keys() {
            if let Some(slot) = keymap::bragi_led(*id).and_then(|led| rgb.get_mut(led)) {
                *slot = common::key_color(state, KEYS_ZONE, *id);
            }
        }
    } else {
        for zone in spec.zones {
            let color = common::zone_color(&state.lighting, zone.id, zone.default_on);
            for led in zone.leds {
                rgb[*led as usize] = color;
            }
        }
    }
    match path {
        LightPath::Planar => {
            let mut out = Vec::with_capacity(rgb.len() * 3);
            out.extend(rgb.iter().map(|c| c.r));
            out.extend(rgb.iter().map(|c| c.g));
            out.extend(rgb.iter().map(|c| c.b));
            out
        }
        LightPath::Triplets => {
            let mut out = vec![0x12, 0x00];
            out.extend(rgb.iter().flat_map(|c| [c.r, c.g, c.b]));
            out
        }
    }
}

impl Device for BragiDevice {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }

    fn capabilities(&self) -> &Capabilities {
        &self.caps
    }

    fn read_state(&mut self) -> Result<DeviceState> {
        self.refresh()?;
        Ok(self.state.clone())
    }

    fn apply(&mut self, setting: &Setting) -> Result<()> {
        self.validate(setting)?;
        match setting {
            Setting::Dpi(d) => match d.current() {
                Some(stage) => self.write_dpi(stage)?,
                None => return Err(Error::invalid("empty DPI table")),
            },
            Setting::DpiActiveStage(i) => {
                let stage = self
                    .state
                    .dpi
                    .as_ref()
                    .and_then(|d| d.stages.get(*i as usize))
                    .copied();
                self.write_dpi(stage.ok_or_else(|| Error::invalid("active stage out of range"))?)?;
            }
            Setting::ReportRate(hz) => {
                let index = RATES_HZ.iter().position(|r| r == hz).map_or(0, |i| i + 1);
                self.link.set(prop::POLL_RATE, index as u32)?;
            }
            Setting::Lighting { .. } | Setting::PerKey(_) => {
                let mut next = self.state.clone();
                setting.apply_to(&mut next);
                self.push_lighting(&next)?;
                self.state = next;
                return Ok(());
            }
            Setting::LiftOff(i) => self
                .link
                .set(prop::LIFT_OFF, u32::from(self.spec.lift[*i as usize].1))?,
            Setting::AngleSnapping(on) => self.link.set(prop::ANGLE_SNAP, u32::from(*on))?,
            // Rejected by validate().
            Setting::Button { .. } | Setting::OnboardProfile(_) | Setting::GameMode(_) => {
                return Err(Error::Unsupported("setting"));
            }
        }
        setting.apply_to(&mut self.state);
        Ok(())
    }

    fn poll_events(&mut self) -> Vec<DeviceEvent> {
        let mut events = Vec::new();
        let now = Instant::now();
        if now.duration_since(self.last_check) >= CHECK_EVERY {
            self.last_check = now;
            self.check(&mut events);
        }
        if self.connected
            && self.light.is_some()
            && now.duration_since(self.last_ping) >= PING_EVERY
        {
            self.last_ping = now;
            if let Err(e) = self.link.ping() {
                log::debug!("{}: keep-alive failed: {e}", self.desc.name);
            }
        }
        if self.connected
            && self.spec.battery
            && now.duration_since(self.last_battery) >= BATTERY_EVERY
        {
            self.last_battery = now;
            if let Ok(Some(battery)) = self.read_battery() {
                if self.state.battery != Some(battery) {
                    self.state.battery = Some(battery);
                    events.push(DeviceEvent::BatteryChanged(battery));
                }
            }
        }
        events
    }
}

impl Drop for BragiDevice {
    /// Hand lighting back to the firmware.
    fn drop(&mut self) {
        if self.light.is_some() && self.connected {
            let _ = self.link.close(bragi::LIGHTING_HANDLE);
            let _ = self.link.set(prop::MODE, bragi::MODE_HARDWARE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeBragi, assert_packet, iface};
    use gearwise_core::model::{DpiStage, LightingEffect, ZoneState};
    use std::collections::BTreeMap;

    fn zone(r: u8, g: u8, b: u8, brightness: u8) -> ZoneState {
        ZoneState {
            effect: LightingEffect::Static {
                color: Rgb::new(r, g, b),
            },
            brightness,
        }
    }

    fn lighting(zone_id: u8, state: ZoneState) -> Setting {
        Setting::Lighting {
            zone: zone_id,
            state,
        }
    }

    fn past(d: Duration) -> Instant {
        Instant::now() - d
    }

    #[test]
    fn open_reads_identity_capabilities_and_state() {
        let fake = FakeBragi::new(64);
        let mut dev = fake.device(0x1b9e);
        assert_eq!(dev.descriptor().firmware.as_deref(), Some("1.2.3"));
        let caps = dev.capabilities().clone();
        assert_eq!(caps.report_rates, [125, 250, 500, 1000]);
        let dpi = caps.dpi.unwrap();
        assert_eq!(
            (dpi.min, dpi.max, dpi.step, dpi.max_stages, dpi.separate_xy),
            (100, 26_000, 10, 5, true)
        );
        assert_eq!(
            caps.lighting
                .iter()
                .map(|z| z.name.as_str())
                .collect::<Vec<_>>(),
            ["Logo", "Scroll wheel", "DPI indicator"]
        );
        assert_eq!(caps.lift_off, ["Ultra low", "Low", "Medium", "High"]);
        assert!(
            caps.angle_snapping
                && !caps.battery
                && caps.per_key.is_none()
                && caps.buttons.is_empty()
        );
        let s = dev.read_state().unwrap();
        assert_eq!(s.report_rate, Some(1000));
        assert_eq!(
            s.dpi,
            Some(DpiState {
                stages: vec![DpiStage::uniform(800)],
                active: 0
            })
        );
        assert_eq!(s.angle_snapping, Some(false));
        assert_eq!(s.lift_off, Some(0));
        assert!(
            s.lighting.is_empty(),
            "lighting is write-only and unknown until set"
        );
    }

    #[test]
    fn firmware_reported_maximum_rate_extends_the_choices() {
        let fake = FakeBragi::new(64);
        fake.set_prop(prop::MAX_POLL_RATE, 7);
        let mut dev = fake.device(0x1b9e);
        assert_eq!(
            dev.capabilities().report_rates,
            [125, 250, 500, 1000, 2000, 4000, 8000]
        );
        dev.apply(&Setting::ReportRate(8000)).unwrap();
        assert_eq!(fake.prop(prop::POLL_RATE), Some(7));
    }

    #[test]
    fn dpi_is_written_to_x_and_y() {
        let fake = FakeBragi::new(64);
        let mut dev = fake.device(0x1b9e);
        let before = fake.writes().len();
        let table = DpiState {
            stages: vec![DpiStage::uniform(800), DpiStage { x: 1600, y: 1200 }],
            active: 1,
        };
        dev.apply(&Setting::Dpi(table.clone())).unwrap();
        let w = fake.writes();
        assert_eq!(w.len(), before + 2);
        assert_packet(&w[before], &[0x08, 0x01, 0x21, 0x00, 0x40, 0x06, 0x00]); // X = 1600
        assert_packet(&w[before + 1], &[0x08, 0x01, 0x22, 0x00, 0xb0, 0x04, 0x00]); // Y = 1200
        let s = dev.read_state().unwrap();
        assert_eq!(s.dpi, Some(table));
    }

    #[test]
    fn linked_axes_use_the_single_dpi_property() {
        let fake = FakeBragi::new(64);
        let mut dev = fake.device(0x1b93); // Katar Pro
        let before = fake.writes().len();
        dev.apply(&Setting::Dpi(DpiState {
            stages: vec![DpiStage::uniform(1200)],
            active: 0,
        }))
        .unwrap();
        let w = fake.writes();
        assert_eq!(w.len(), before + 1);
        assert_packet(&w[before], &[0x08, 0x01, 0x20, 0x00, 0xb0, 0x04, 0x00]);
        let split = DpiState {
            stages: vec![DpiStage { x: 800, y: 1200 }],
            active: 0,
        };
        assert!(matches!(
            dev.apply(&Setting::Dpi(split)),
            Err(Error::InvalidValue(_))
        ));
    }

    #[test]
    fn switching_the_active_stage_writes_that_stage() {
        let fake = FakeBragi::new(64);
        let mut dev = fake.device(0x1b9e);
        let table = DpiState {
            stages: vec![DpiStage::uniform(800), DpiStage::uniform(2400)],
            active: 0,
        };
        dev.apply(&Setting::Dpi(table)).unwrap();
        let before = fake.writes().len();
        dev.apply(&Setting::DpiActiveStage(1)).unwrap();
        let w = fake.writes();
        assert_packet(&w[before], &[0x08, 0x01, 0x21, 0x00, 0x60, 0x09, 0x00]); // 2400
        assert_eq!(dev.read_state().unwrap().dpi.unwrap().active, 1);
        assert!(matches!(
            dev.apply(&Setting::DpiActiveStage(2)),
            Err(Error::InvalidValue(_))
        ));
    }

    #[test]
    fn firmware_errors_are_forgiven_only_when_the_sensor_took_the_value() {
        let fake = FakeBragi::new(64);
        let mut dev = fake.device(0x1b9e);
        let table = DpiState {
            stages: vec![DpiStage::uniform(1000)],
            active: 0,
        };
        // Value stored although the reply carries an error status.
        fake.quirk_prop(prop::DPI_X, 3);
        dev.apply(&Setting::Dpi(table.clone())).unwrap();
        // Value rejected.
        fake.fail_prop(prop::DPI_X, 1);
        let r = dev.apply(&Setting::Dpi(DpiState {
            stages: vec![DpiStage::uniform(2000)],
            active: 0,
        }));
        assert!(matches!(r, Err(Error::Protocol(_))));
        assert_eq!(
            dev.state.dpi,
            Some(table),
            "failed writes leave the cache alone"
        );
    }

    #[test]
    fn rate_lift_and_snap_packets() {
        let fake = FakeBragi::new(64);
        let mut dev = fake.device(0x1b9e);
        let before = fake.writes().len();
        dev.apply(&Setting::ReportRate(500)).unwrap();
        dev.apply(&Setting::LiftOff(2)).unwrap();
        dev.apply(&Setting::AngleSnapping(true)).unwrap();
        let w = fake.writes();
        assert_packet(&w[before], &[0x08, 0x01, 0x01, 0x00, 0x03, 0x00, 0x00]);
        assert_packet(&w[before + 1], &[0x08, 0x01, 0x06, 0x00, 0x03, 0x00, 0x00]); // Medium = 3
        assert_packet(&w[before + 2], &[0x08, 0x01, 0x07, 0x00, 0x01, 0x00, 0x00]);
        let s = dev.read_state().unwrap();
        assert_eq!(
            (s.report_rate, s.lift_off, s.angle_snapping),
            (Some(500), Some(2), Some(true))
        );
    }

    #[test]
    fn invalid_and_unsupported_settings_send_nothing() {
        let fake = FakeBragi::new(64);
        let mut dev = fake.device(0x1b9e);
        let before = fake.writes().len();
        let too_fast = Setting::ReportRate(8000);
        let too_sharp = Setting::Dpi(DpiState {
            stages: vec![DpiStage::uniform(30_000)],
            active: 0,
        });
        let no_zone = lighting(9, zone(1, 2, 3, 100));
        let breathing = lighting(
            0,
            ZoneState {
                effect: LightingEffect::Breathing {
                    color: Rgb::WHITE,
                    period_ms: 1000,
                },
                brightness: 100,
            },
        );
        let loud = lighting(0, zone(1, 2, 3, 101));
        for bad in [too_fast, too_sharp, no_zone, loud, Setting::LiftOff(9)] {
            assert!(
                matches!(dev.apply(&bad), Err(Error::InvalidValue(_))),
                "{bad:?}"
            );
        }
        for unsupported in [
            breathing,
            Setting::PerKey(BTreeMap::new()),
            Setting::GameMode(true),
            Setting::OnboardProfile(0),
            Setting::Button {
                id: 1,
                action: gearwise_core::model::ButtonAction::Disabled,
            },
        ] {
            assert!(
                matches!(dev.apply(&unsupported), Err(Error::Unsupported(_))),
                "{unsupported:?}"
            );
        }
        assert_eq!(fake.writes().len(), before);
        // Lift-off and DPI do not exist on a keyboard.
        let fake = FakeBragi::new(64);
        let mut kb = fake.device(0x1ba4);
        assert!(matches!(
            kb.apply(&Setting::LiftOff(0)),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            kb.apply(&Setting::AngleSnapping(true)),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            kb.apply(&Setting::DpiActiveStage(0)),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn first_lighting_write_takes_over_and_streams_the_planes() {
        let fake = FakeBragi::new(64);
        let mut dev = fake.device(0x1ba4); // K55 RGB PRO: lock LED + 5 zones
        let before = fake.writes().len();
        dev.apply(&lighting(1, zone(255, 0, 0, 100))).unwrap();
        let w = &fake.writes()[before..];
        assert_eq!(w.len(), 5);
        assert_packet(&w[0], &[0x08, 0x01, 0x03, 0x00, 0x02, 0x00, 0x00]); // software mode
        assert_packet(&w[1], &[0x08, 0x0d, 0x00, 0x01, 0x00]); // open lighting on handle 0
        assert_packet(&w[2], &[0x08, 0x02, 0x02, 0x00]); // brightness probe
        assert_packet(&w[3], &[0x08, 0x02, 0x44, 0x00]); // coarse brightness probe
        let mut expected = vec![0x08, 0x06, 0x00, 18, 0, 0, 0];
        expected.extend([0, 255, 255, 255, 255, 255]); // red plane, lock LED off
        expected.extend([0, 0, 255, 255, 255, 255]); // green plane
        expected.extend([0, 0, 255, 255, 255, 255]); // blue plane
        assert_packet(&w[4], &expected);
        assert_eq!(fake.handle_open(0), Some(0x01));

        // Already in software mode: just the colours.
        let before = fake.writes().len();
        dev.apply(&lighting(2, zone(0, 255, 0, 50))).unwrap();
        let w = &fake.writes()[before..];
        assert_eq!(w.len(), 1);
        let mut expected = vec![0x08, 0x06, 0x00, 18, 0, 0, 0];
        expected.extend([0, 255, 0, 255, 255, 255]);
        expected.extend([0, 0, 127, 255, 255, 255]);
        expected.extend([0, 0, 0, 255, 255, 255]);
        assert_packet(&w[0], &expected);

        let s = dev.read_state().unwrap();
        assert_eq!(s.lighting.len(), 2);
        assert_eq!(s.lighting[&2], zone(0, 255, 0, 50));
    }

    #[test]
    fn off_and_indicator_defaults() {
        let fake = FakeBragi::new(64);
        let mut dev = fake.device(0x1ba4);
        dev.apply(&lighting(
            1,
            ZoneState {
                effect: LightingEffect::Off,
                brightness: 100,
            },
        ))
        .unwrap();
        let data = fake.written(0);
        assert_eq!(
            data[..6],
            [0, 0, 255, 255, 255, 255],
            "zone 1 off, lock indicator off by default"
        );
    }

    #[test]
    fn dimmed_hardware_brightness_is_restored_once() {
        let fake = FakeBragi::new(64);
        fake.set_prop(prop::BRIGHTNESS, 500);
        let mut dev = fake.device(0x1b9e);
        let before = fake.writes().len();
        dev.apply(&lighting(0, zone(255, 255, 255, 100))).unwrap();
        let w = &fake.writes()[before..];
        assert_packet(&w[2], &[0x08, 0x02, 0x02, 0x00]); // read the dimmed level
        assert_packet(&w[3], &[0x08, 0x01, 0x02, 0x00, 0xe8, 0x03, 0x00]); // write 1000
        assert_eq!(fake.prop(prop::BRIGHTNESS), Some(1000));
        // Coarse-only firmware (0..=3).
        let fake = FakeBragi::new(64);
        fake.set_prop(prop::BRIGHTNESS_COARSE, 1);
        let mut dev = fake.device(0x1ba4);
        dev.apply(&lighting(1, zone(255, 255, 255, 100))).unwrap();
        assert_eq!(fake.prop(prop::BRIGHTNESS_COARSE), Some(3));
    }

    #[test]
    fn keyboards_that_reject_the_plain_resource_get_rgb_triplets() {
        let fake = FakeBragi::new(1024);
        fake.refuse_resource(0x01, 6);
        let mut dev = fake.device(0x1b73); // K70 RGB TKL
        let keys = BTreeMap::from([(0x04u16, Rgb::new(255, 0, 0))]);
        let before = fake.writes().len();
        dev.apply(&Setting::PerKey(keys)).unwrap();
        let w = &fake.writes()[before..];
        assert_packet(&w[1], &[0x08, 0x0d, 0x00, 0x01, 0x00]);
        assert_packet(&w[2], &[0x08, 0x0d, 0x00, 0x22, 0x00]);
        assert_eq!(w[0].len(), 1025);
        assert_eq!(fake.handle_open(0), Some(0x22));
        let data = fake.written(0);
        assert_eq!(data.len(), 2 + 193 * 3);
        assert_eq!(data[..2], [0x12, 0x00]);
        assert_eq!(
            data[2..5],
            [255, 255, 255],
            "LEDs without a key follow the base colour"
        );
        assert_eq!(data[2 + 4 * 3..2 + 4 * 3 + 3], [255, 0, 0], "A is LED 4");
    }

    #[test]
    fn per_key_buffer_is_split_over_continuation_packets() {
        let fake = FakeBragi::new(64);
        let mut dev = fake.device(0x1b89); // K95 RGB Platinum XT: 156 LEDs
        let keys = BTreeMap::from([(0x04u16, Rgb::new(255, 0, 0)), (0xe1, Rgb::new(0, 0, 255))]);
        let before = fake.writes().len();
        dev.apply(&Setting::PerKey(keys)).unwrap();
        let w: Vec<Vec<u8>> = fake.writes()[before..]
            .iter()
            .filter(|p| p[2] == 0x06 || p[2] == 0x07)
            .cloned()
            .collect();
        // 468 bytes: 57 in the first packet, then 61 per continuation.
        assert_eq!(w.len(), 1 + 7);
        assert_eq!(w[0][1..8], [0x08, 0x06, 0x00, 0xd4, 0x01, 0x00, 0x00]);
        for p in &w[1..] {
            assert_eq!(p[1..4], [0x08, 0x07, 0x00]);
            assert_eq!(p.len(), 65);
        }
        let mut expected = vec![255u8; 156 * 3];
        expected[4] = 255; // A: red 255, green 0, blue 0
        expected[156 + 4] = 0;
        expected[312 + 4] = 0;
        expected[106] = 0; // Left Shift (LED 106): blue only
        expected[156 + 106] = 0;
        expected[312 + 106] = 255;
        // Reassemble from the raw packets rather than trusting the fake.
        let mut got = w[0][8..65].to_vec();
        for p in &w[1..] {
            got.extend_from_slice(&p[4..65]);
        }
        got.truncate(468);
        assert_eq!(got, expected);
        assert_eq!(fake.written(0), expected);
    }

    #[test]
    fn unknown_keys_are_rejected_without_io() {
        let fake = FakeBragi::new(64);
        let mut dev = fake.device(0x1b73);
        let before = fake.writes().len();
        let r = dev.apply(&Setting::PerKey(BTreeMap::from([(0x0100u16, Rgb::WHITE)])));
        assert!(matches!(r, Err(Error::InvalidValue(_))));
        assert_eq!(fake.writes().len(), before);
    }

    #[test]
    fn keys_zone_brightness_dims_per_key_colors() {
        let fake = FakeBragi::new(64);
        let mut dev = fake.device(0x1b89);
        dev.apply(&lighting(0, zone(0, 0, 200, 50))).unwrap();
        dev.apply(&Setting::PerKey(BTreeMap::from([(
            0x04u16,
            Rgb::new(200, 100, 0),
        )])))
        .unwrap();
        let data = fake.written(0);
        assert_eq!((data[4], data[156 + 4], data[312 + 4]), (100, 50, 0));
        assert_eq!((data[5], data[156 + 5], data[312 + 5]), (0, 0, 100));
    }

    #[test]
    fn wireless_battery_is_parsed_and_changes_are_reported() {
        let fake = FakeBragi::new(64);
        let mut dev = fake.device(0x1bb5);
        assert!(dev.capabilities().battery);
        let s = dev.read_state().unwrap();
        assert_eq!(
            s.battery,
            Some(Battery {
                percent: Some(75),
                charging: false
            })
        );
        assert!(
            dev.poll_events().is_empty(),
            "nothing is due right after opening"
        );

        fake.set_prop(prop::BATTERY_LEVEL, 500);
        fake.set_prop(prop::BATTERY_STATUS, 1);
        dev.last_battery = past(BATTERY_EVERY);
        let ev = dev.poll_events();
        assert_eq!(
            ev,
            [DeviceEvent::BatteryChanged(Battery {
                percent: Some(50),
                charging: true
            })]
        );
        dev.last_battery = past(BATTERY_EVERY);
        assert!(
            dev.poll_events().is_empty(),
            "unchanged battery is not reported again"
        );
    }

    #[test]
    fn full_battery_is_not_charging() {
        let fake = FakeBragi::new(64);
        fake.set_prop(prop::BATTERY_LEVEL, 1000);
        fake.set_prop(prop::BATTERY_STATUS, 3);
        let dev = fake.device(0x1bb5);
        assert_eq!(
            dev.state.battery,
            Some(Battery {
                percent: Some(100),
                charging: false
            })
        );
    }

    #[test]
    fn keep_alive_is_sent_only_while_lighting_is_ours() {
        let fake = FakeBragi::new(64);
        let mut dev = fake.device(0x1ba4);
        dev.last_ping = past(PING_EVERY);
        let before = fake.writes().len();
        dev.poll_events();
        assert_eq!(fake.writes().len(), before, "no ping in hardware mode");

        dev.apply(&lighting(1, zone(1, 2, 3, 100))).unwrap();
        let before = fake.writes().len();
        dev.poll_events();
        assert_eq!(fake.writes().len(), before, "not due yet");
        dev.last_ping = past(PING_EVERY);
        dev.poll_events();
        let w = fake.writes();
        assert_eq!(w.len(), before + 1);
        assert_packet(&w[before], &[0x08, 0x12]);
    }

    #[test]
    fn dpi_changes_made_on_the_mouse_are_noticed() {
        let fake = FakeBragi::new(64);
        let mut dev = fake.device(0x1b9e);
        dev.apply(&Setting::Dpi(DpiState {
            stages: vec![DpiStage::uniform(800), DpiStage::uniform(1600)],
            active: 0,
        }))
        .unwrap();
        // The user presses the DPI button: the sensor moves to stage 2.
        fake.set_prop(prop::DPI_X, 1600);
        fake.set_prop(prop::DPI_Y, 1600);
        dev.last_check = past(CHECK_EVERY);
        assert_eq!(dev.poll_events(), [DeviceEvent::StateChanged]);
        assert_eq!(dev.state.dpi.as_ref().unwrap().active, 1);
        // A value outside the table replaces it.
        fake.set_prop(prop::DPI_X, 3200);
        fake.set_prop(prop::DPI_Y, 3200);
        dev.last_check = past(CHECK_EVERY);
        assert_eq!(dev.poll_events(), [DeviceEvent::StateChanged]);
        assert_eq!(
            dev.state.dpi,
            Some(DpiState {
                stages: vec![DpiStage::uniform(3200)],
                active: 0
            })
        );
        dev.last_check = past(CHECK_EVERY);
        assert!(dev.poll_events().is_empty());
    }

    #[test]
    fn wireless_device_going_to_sleep_and_waking() {
        let fake = FakeBragi::new(64);
        let model = crate::models::find(0x1b4c).unwrap();
        let crate::models::Proto::Bragi(spec) = &model.proto else {
            panic!()
        };
        fake.set_prop(prop::SUBDEVICES, 0b0000_0100);
        fake.set_prop(prop::MODE, 1);
        fake.set_raw(prop::FIRMWARE, &[1, 0, 0, 0]);
        fake.set_prop(prop::POLL_RATE, 4);
        fake.set_prop(prop::DPI_X, 800);
        fake.set_prop(prop::DPI_Y, 800);
        fake.set_prop(prop::BATTERY_LEVEL, 900);
        fake.set_prop(prop::BATTERY_STATUS, 2);
        fake.set_prop(prop::ANGLE_SNAP, 0);
        let mut dev = BragiDevice::open(
            fake.link(0).sub_device(2),
            Some((fake.link(0), 2)),
            model,
            spec,
            &iface(0x1ba6, "rx"),
        )
        .unwrap();
        dev.apply(&lighting(0, zone(10, 20, 30, 100))).unwrap();

        fake.set_prop(prop::SUBDEVICES, 0);
        dev.last_check = past(CHECK_EVERY);
        assert_eq!(dev.poll_events(), [DeviceEvent::Disconnected]);
        dev.last_ping = past(PING_EVERY);
        let before = fake.writes().len();
        assert!(dev.poll_events().is_empty());
        assert_eq!(fake.writes().len(), before, "asleep: no I/O to the device");

        fake.set_prop(prop::SUBDEVICES, 0b0000_0100);
        dev.last_check = past(CHECK_EVERY);
        assert_eq!(
            dev.poll_events(),
            [DeviceEvent::Reconnected, DeviceEvent::StateChanged]
        );
        // The colours were pushed again to the sub-device (magic 0x0a).
        let w = fake.writes();
        assert!(w[before..].iter().any(|p| p[1] == 0x0a && p[2] == 0x06));
    }

    #[test]
    fn dropping_hands_lighting_back_to_the_firmware() {
        let fake = FakeBragi::new(64);
        let mut dev = fake.device(0x1ba4);
        dev.apply(&lighting(1, zone(1, 2, 3, 100))).unwrap();
        drop(dev);
        let w = fake.writes();
        let n = w.len();
        assert_packet(&w[n - 2], &[0x08, 0x05, 0x01, 0x00]);
        assert_packet(&w[n - 1], &[0x08, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00]);
        assert_eq!(fake.handle_open(0), None);

        // A device whose lighting was never touched is left alone.
        let fake = FakeBragi::new(64);
        let dev = fake.device(0x1ba4);
        let before = fake.writes().len();
        drop(dev);
        assert_eq!(fake.writes().len(), before);
    }

    #[test]
    fn lighting_handle_lost_mid_session_is_reopened() {
        let fake = FakeBragi::new(64);
        let mut dev = fake.device(0x1ba4);
        dev.apply(&lighting(1, zone(1, 2, 3, 100))).unwrap();
        // The firmware dropped to hardware mode and closed the handle behind our back.
        fake.link(0).close(0).unwrap();
        dev.apply(&lighting(2, zone(4, 5, 6, 100))).unwrap();
        assert_eq!(fake.handle_open(0), Some(0x01));
        // Red plane: lock LED off, zone 1 keeps its colour, zone 2 is the new one.
        assert_eq!(fake.written(0)[..3], [0, 1, 4]);
    }

    #[test]
    fn stale_open_handle_from_an_earlier_session_is_closed_first() {
        let fake = FakeBragi::new(64);
        let mut dev = fake.device(0x1ba4);
        assert_eq!(fake.link(0).open(0, 0x01).unwrap(), 0);
        dev.apply(&lighting(1, zone(1, 2, 3, 100))).unwrap();
        assert_eq!(fake.written(0)[1], 1);
    }
}
