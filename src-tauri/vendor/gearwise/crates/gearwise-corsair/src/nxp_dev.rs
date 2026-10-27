//! A legacy NXP keyboard or mouse as a [`Device`].
//!
//! Mice keep their DPI stage table in the device. Stage `k` of the user's
//! table lives in hardware stage `k + 1`: hardware stage 0 belongs to the
//! sniper button and is left alone. Settings are written live (volatile) and
//! persisted by [`Device::save_onboard`]. Lighting is host-rendered in
//! software mode.

use std::thread::sleep;
use std::time::Duration;

use gearwise_core::device::{Capabilities, Device, DeviceDescriptor, DeviceState, Setting};
use gearwise_core::error::{Error, Result};
use gearwise_core::model::{Connection, DpiStage, DpiState, Rgb};
use gearwise_core::transport::{HidInterfaceInfo, HidTransport};

use crate::common::{self, RATES_HZ};
use crate::keymap::NXP_KEYS;
use crate::models::{DPI_STAGES, KEYS_ZONE, Model, NxpSpec};
use crate::nxp::{GET, Ident, Nxp, SET, field, mouse};

/// Keyboards address their LEDs in three planes (red, green, blue) streamed
/// as 60-byte chunks.
const CHUNK: usize = 60;
/// Keys addressed by the key-input configuration.
const KEY_INPUT_KEYS: usize = 144;
const SETTLE: Duration = Duration::from_millis(30);

/// Key input modes: report as HID, report as Corsair event, or both.
const INPUT_HID: u8 = 0x80;
const INPUT_BOTH: u8 = 0xc0;
/// Keys whose hardware function (M-keys, brightness, Win lock) is restored
/// on exit, with the action to hand back.
const HARDWARE_KEYS: [(u8, u8); 6] = [
    (0x0b, 9),
    (0x17, 10),
    (0x23, 11),
    (0x2f, 12),
    (0x89, 1),
    (0x08, 8),
];

const LIFT_LABELS: [&str; 5] = ["1 (lowest)", "2", "3", "4", "5 (highest)"];

/// Lighting zone numbers on mice and their names.
const MOUSE_ZONES: [(u8, &str); 6] = [
    (1, "Front"),
    (2, "Logo"),
    (3, "DPI indicator"),
    (4, "Scroll wheel"),
    (5, "Thumb"),
    (6, "Side"),
];

pub(crate) struct NxpDevice {
    nxp: Nxp,
    desc: DeviceDescriptor,
    caps: Capabilities,
    spec: &'static NxpSpec,
    state: DeviceState,
    /// Mouse lighting zone numbers present on this device.
    zones: Vec<u8>,
    /// Hardware stage behind each stage of `state.dpi`.
    hw_stages: Vec<u8>,
    /// Enabled-stage bitmask as last known (bit 0 is the sniper stage).
    mask: u8,
    software: bool,
    input_software: bool,
    input_buttons: std::collections::BTreeSet<u16>,
}

impl NxpDevice {
    pub(crate) fn open(
        transport: Box<dyn HidTransport>,
        model: &'static Model,
        spec: &'static NxpSpec,
        iface: &HidInterfaceInfo,
    ) -> Result<Self> {
        let mut nxp = Nxp::new(transport);
        let ident = nxp.ident()?;
        let zones: Vec<u8> = if spec.dpi_max.is_some() {
            MOUSE_ZONES
                .iter()
                .map(|(n, _)| *n)
                .filter(|n| ident.zones >> n & 1 == 1)
                .collect()
        } else {
            Vec::new()
        };
        let mut caps = capabilities(spec, &zones);
        if model.pid==0x1b2e {use gearwise_core::model::{ButtonCaps,ButtonAction,DpiFunction,MouseButton};caps.buttons=(1..=8).map(|id|ButtonCaps{id,name:["左键","右键","中键","后退","前进","DPI 上调","DPI 下调","狙击键"][id as usize-1].into(),remappable:id!=1,default_action:match id{1=>ButtonAction::Mouse(MouseButton::Left),2=>ButtonAction::Mouse(MouseButton::Right),3=>ButtonAction::Mouse(MouseButton::Middle),4=>ButtonAction::Mouse(MouseButton::Back),5=>ButtonAction::Mouse(MouseButton::Forward),6=>ButtonAction::Dpi(DpiFunction::Up),7=>ButtonAction::Dpi(DpiFunction::Down),_=>ButtonAction::Default},hotspot:None}).collect();}
        let mut dev = Self {
            nxp,
            desc: common::describe(
                model,
                iface,
                None,
                Connection::Wired,
                Some(ident.firmware.clone()),
            ),
            caps,
            spec,
            state: DeviceState::default(),
            zones,
            hw_stages: Vec::new(),
            mask: 0,
            software: false,
            input_software: false,
            input_buttons: std::collections::BTreeSet::new(),
        };
        dev.state.report_rate = rate_from_interval(&ident);
        if spec.dpi_max.is_some() {
            dev.read_mouse_profile()?;
        }
        Ok(dev)
    }

    /// Load the stored DPI stages, current stage, lift height and angle snapping.
    fn read_mouse_profile(&mut self) -> Result<()> {
        // Argument 1 reads the stored profile.
        let mask = self.nxp.query(&[GET, field::MOUSE, mouse::DPI_MASK, 1])?[4];
        let current = self.nxp.query(&[GET, field::MOUSE, mouse::DPI, 1])?[4];
        let lift = self.nxp.query(&[GET, field::MOUSE, mouse::LIFT, 1])?[4];
        let snap = self.nxp.query(&[GET, field::MOUSE, mouse::SNAP, 1])?[4];
        let mut stages = Vec::new();
        let mut hw = Vec::new();
        for h in 1..=DPI_STAGES {
            if mask >> h & 1 == 1 {
                let r = self
                    .nxp
                    .query(&[GET, field::MOUSE, mouse::DPI_PROFILE | h, 1])?;
                stages.push(DpiStage {
                    x: u16::from_le_bytes([r[5], r[6]]),
                    y: u16::from_le_bytes([r[7], r[8]]),
                });
                hw.push(h);
            }
        }
        self.mask = mask;
        self.state.dpi = if stages.is_empty() {
            None
        } else {
            let active = hw.iter().position(|h| *h == current).unwrap_or(0) as u8;
            Some(DpiState { stages, active })
        };
        self.hw_stages = hw;
        self.state.lift_off = (1..=5).contains(&lift).then(|| lift - 1);
        self.state.angle_snapping = Some(snap != 0);
        if self.desc.product_id==0x1b2e{if let Ok(r)=self.nxp.query(&[GET,field::MOUSE,mouse::DPI_PROFILE,1]){let sniper=u16::from_le_bytes([r[5],r[6]]);if (100..=12000).contains(&sniper){let action=gearwise_core::model::ButtonAction::Dpi(gearwise_core::model::DpiFunction::Shift(sniper));self.state.buttons.insert(8,action.clone());if let Some(b)=self.caps.buttons.iter_mut().find(|b|b.id==8){b.default_action=action;}}}}
        Ok(())
    }

    fn validate(&self, setting: &Setting) -> Result<()> {
        let mouse = self.spec.dpi_max.is_some();
        match setting {
            Setting::Dpi(d) => {
                common::check_dpi(&self.spec.dpi().ok_or(Error::Unsupported("dpi"))?, d)
            }
            Setting::DpiActiveStage(i) => {
                let n = self.state.dpi.as_ref().map_or(0, |d| d.stages.len());
                if !mouse {
                    Err(Error::Unsupported("dpi"))
                } else if (*i as usize) < n {
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
            Setting::LiftOff(i) if mouse && (*i as usize) < LIFT_LABELS.len() => Ok(()),
            Setting::LiftOff(_) if mouse => Err(Error::invalid("lift-off choice")),
            Setting::LiftOff(_) => Err(Error::Unsupported("lift-off distance")),
            Setting::AngleSnapping(_) if mouse => Ok(()),
            Setting::AngleSnapping(_) => Err(Error::Unsupported("angle snapping")),
            Setting::Button{id,action} if self.desc.product_id==0x1b2e=>{use gearwise_core::model::{ButtonAction,DpiFunction};if !(2..=8).contains(id){return Err(Error::invalid("unknown M65 button or protected left click"));}match action{ButtonAction::Default|ButtonAction::Disabled|ButtonAction::Mouse(_)|ButtonAction::Dpi(DpiFunction::Cycle|DpiFunction::Up|DpiFunction::Down)=>Ok(()),ButtonAction::Dpi(DpiFunction::Shift(v)) if (100..=12000).contains(v)=>Ok(()),ButtonAction::Key{usage,..} if (4..=0xa4).contains(usage)=>Ok(()),ButtonAction::Macro(id) if !id.is_empty()&&id.len()<=128=>Ok(()),_=>Err(Error::Unsupported("M65 software action"))}},
            Setting::Button { .. } => Err(Error::Unsupported("button remapping")),
            Setting::OnboardProfile(_) => Err(Error::Unsupported("onboard profiles")),
            Setting::GameMode(_) => Err(Error::Unsupported("game mode")),
            Setting::DpiColor{..} | Setting::DpiIndicator(_) | Setting::SensorCalibration(_) | Setting::VendorProfile{..} | Setting::Tuning(_) => Err(Error::Unsupported("mouse performance")),
        }
    }

    /// Write the stage table, enable mask and active stage.
    fn write_dpi(&mut self, dpi: &DpiState, persist: bool) -> Result<()> {
        let flag = u8::from(persist);
        for (k, st) in dpi.stages.iter().enumerate() {
            let h = k as u8 + 1;
            let (x, y) = (st.x.to_le_bytes(), st.y.to_le_bytes());
            // Byte 4 marks independent X and Y values; it is zero when saving.
            let independent = u8::from(!persist && st.x != st.y);
            self.nxp.send(&[
                SET,
                field::MOUSE,
                mouse::DPI_PROFILE | h,
                flag,
                independent,
                x[0],
                x[1],
                y[0],
                y[1],
            ])?;
        }
        // Keep the sniper stage's bit, enable stages 1..=n.
        let mask = (self.mask & 1) | ((1u8 << (dpi.stages.len() + 1)) - 2);
        self.nxp
            .send(&[SET, field::MOUSE, mouse::DPI_MASK, flag, mask])?;
        self.nxp
            .send(&[SET, field::MOUSE, mouse::DPI, flag, dpi.active + 1])?;
        self.mask = mask;
        self.hw_stages = (1..=dpi.stages.len() as u8).collect();
        Ok(())
    }

    // --- lighting --------------------------------------------------------

    /// Switch to software lighting (idempotent).
    fn ensure_software(&mut self) -> Result<()> {
        if self.software {
            return Ok(());
        }
        if self.spec.keys.is_none() {
            self.nxp.send(&[SET, field::LIGHTING, 2, 0, 1])?;
        } else if self.spec.extended {
            self.nxp.send(&[SET, field::SPECIAL, 2])?;
            sleep(SETTLE);
            self.key_input(|_| INPUT_BOTH)?;
            sleep(SETTLE);
        } else {
            self.nxp.send(&[SET, field::LIGHTING, 2, 0, 3])?;
        }
        self.software = true;
        Ok(())
    }

    fn write_mouse_input(&mut self,next:&DeviceState)->Result<()>{
        let _=next;
        let mut packet=vec![SET,field::KEY_INPUT,20,0];
        for id in 1..=20u8{let overridden=self.input_buttons.contains(&u16::from(id));packet.extend_from_slice(&[id,if id<=5&&!overridden{0x80}else{0x40}]);}
        self.input_software=true; // every subsequent failure must attempt hardware restoration
        self.nxp.send(&[SET,field::SPECIAL,2])?;
        self.nxp.send(&packet)
    }
    /// Configure how each key reports (30 keys per packet), then commit.
    fn key_input(&mut self, action: impl Fn(u8) -> u8) -> Result<()> {
        let keys: Vec<u8> = (0..KEY_INPUT_KEYS as u8).collect();
        for group in keys.chunks(30) {
            let mut p = vec![SET, field::KEY_INPUT, group.len() as u8, 0];
            for k in group {
                p.extend_from_slice(&[*k, action(*k)]);
            }
            self.nxp.send(&p)?;
        }
        self.nxp.send(&[SET, field::LIGHTING, 2, 0, 3, 0])
    }

    fn write_lighting(&mut self, state: &DeviceState) -> Result<()> {
        self.ensure_software()?;
        match self.spec.keys {
            Some((_, slots)) => self.write_keyboard(state, slots),
            None => self.write_mouse(state),
        }
    }

    fn write_mouse(&mut self, state: &DeviceState) -> Result<()> {
        let mut p = vec![SET, field::MOUSE_COLOR, self.zones.len() as u8, 1];
        for zone in &self.zones {
            let c = common::zone_color(&state.lighting, *zone, true);
            p.extend_from_slice(&[*zone, c.r, c.g, c.b]);
        }
        self.nxp.send(&p)
    }

    fn write_keyboard(&mut self, state: &DeviceState, slots: usize) -> Result<()> {
        let mut planes = [vec![0u8; slots], vec![0u8; slots], vec![0u8; slots]];
        for (usage, slot) in NXP_KEYS {
            let Rgb { r, g, b } = common::key_color(state, KEYS_ZONE, u16::from(usage));
            let slot = slot as usize;
            planes[0][slot] = r;
            planes[1][slot] = g;
            planes[2][slot] = b;
        }
        for (i, plane) in planes.iter().enumerate() {
            for (n, chunk) in plane.chunks(CHUNK).enumerate() {
                self.nxp.stream(n as u8 + 1, chunk)?;
            }
            // Colour 1..=3; the last plane carries the "finished" marker.
            self.nxp.send(&[
                SET,
                field::KEYBOARD_COLOR,
                i as u8 + 1,
                3,
                if i == 2 { 2 } else { 1 },
            ])?;
        }
        Ok(())
    }
}

fn capabilities(spec: &NxpSpec, zones: &[u8]) -> Capabilities {
    let mut caps = Capabilities::default();
    if spec.adj_rate {
        caps.report_rates = RATES_HZ[..4].to_vec();
    }
    if let Some((layout, _)) = spec.keys {
        caps.lighting = vec![common::zone_caps(KEYS_ZONE, "Keys")];
        caps.per_key = Some(layout.build());
    }
    if let Some(dpi) = spec.dpi() {
        caps.dpi = Some(common::dpi_caps(&dpi));
        caps.lighting = zones
            .iter()
            .filter_map(|z| MOUSE_ZONES.iter().find(|(n, _)| n == z))
            .map(|(n, name)| common::zone_caps(*n, name))
            .collect();
        caps.lift_off = LIFT_LABELS.iter().map(|s| s.to_string()).collect();
        caps.angle_snapping = true;
    }
    caps
}

fn rate_from_interval(ident: &Ident) -> Option<u16> {
    match ident.poll_ms {
        ms @ (1 | 2 | 4 | 8) => Some(1000 / u16::from(ms)),
        _ => None,
    }
}

impl Device for NxpDevice {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }

    fn capabilities(&self) -> &Capabilities {
        &self.caps
    }

    fn read_state(&mut self) -> Result<DeviceState> {
        Ok(self.state.clone())
    }

    fn apply(&mut self, setting: &Setting) -> Result<()> {
        self.validate(setting)?;
        match setting {
            Setting::Dpi(d) => self.write_dpi(d, false)?,
            Setting::DpiActiveStage(i) => {
                let hw = self
                    .hw_stages
                    .get(*i as usize)
                    .copied()
                    .ok_or_else(|| Error::invalid("active stage out of range"))?;
                self.nxp.send(&[SET, field::MOUSE, mouse::DPI, 0, hw])?;
            }
            Setting::ReportRate(hz) => {
                // The device re-enumerates after this; the next I/O fails and the device is reopened.
                let result=self.nxp.send(&[SET, field::POLL_RATE, 0, 0, (1000 / *hz) as u8]);
                // M65 continuation owns the next connection. Drop must not send
                // cached input/lighting resets to the potentially stale handle.
                if self.desc.product_id==0x1b2e{self.software=false;self.input_software=false;self.input_buttons.clear();}
                result?;
            }
            Setting::Lighting { .. } | Setting::PerKey(_) => {
                let mut next = self.state.clone();
                setting.apply_to(&mut next);
                self.write_lighting(&next)?;
                self.state = next;
                return Ok(());
            }
            Setting::LiftOff(i) => self
                .nxp
                .send(&[SET, field::MOUSE, mouse::LIFT, 0, *i + 1])?,
            Setting::AngleSnapping(on) => {
                self.nxp
                    .send(&[SET, field::MOUSE, mouse::SNAP, 0, u8::from(*on), 0x05])?
            }
            Setting::Button { id,action } if self.desc.product_id==0x1b2e=>{let mut next=self.state.clone();setting.apply_to(&mut next);if matches!(action,gearwise_core::model::ButtonAction::Default){self.input_buttons.remove(id);}else{self.input_buttons.insert(*id);}if self.input_buttons.is_empty(){self.stop_software_actions()?;}else{self.write_mouse_input(&next)?;}self.state=next;return Ok(());}
            // Rejected by validate().
            Setting::Button { .. } | Setting::OnboardProfile(_) | Setting::GameMode(_) | Setting::DpiColor{..} | Setting::DpiIndicator(_) | Setting::SensorCalibration(_) | Setting::VendorProfile{..} | Setting::Tuning(_) => {
                return Err(Error::Unsupported("setting"));
            }
        }
        setting.apply_to(&mut self.state);
        Ok(())
    }

    fn stop_software_actions(&mut self)->Result<()>{
        if self.desc.product_id!=0x1b2e||!self.input_software{return Ok(());}
        self.restore_hardware_input()
    }
    fn restore_hardware_input(&mut self)->Result<()>{
        if self.desc.product_id!=0x1b2e{return Err(Error::Unsupported("manual hardware input recovery"));}
        let mut p=vec![SET,field::KEY_INPUT,20,0];for id in 1..=20u8{p.extend_from_slice(&[id,if id<=5{0x80}else{0x40}]);}
        self.nxp.send(&p)?;self.nxp.send(&[SET,field::SPECIAL,1])?;self.input_software=false;self.input_buttons.clear();
        self.state.buttons.clear();if let Some(b)=self.caps.buttons.iter().find(|b|b.id==8){self.state.buttons.insert(8,b.default_action.clone());}Ok(())
    }
    /// Mice: write the stage table, active stage, lift height and angle
    /// snapping to the stored profile.
    fn save_onboard(&mut self) -> Result<()> {
        let Some(dpi) = self.state.dpi.clone() else {
            return Ok(());
        };
        self.write_dpi(&dpi, true)?;
        if let Some(l) = self.state.lift_off {
            self.nxp.send(&[SET, field::MOUSE, mouse::LIFT, 1, l + 1])?;
        }
        if let Some(on) = self.state.angle_snapping {
            self.nxp
                .send(&[SET, field::MOUSE, mouse::SNAP, 1, u8::from(on), 0x05])?;
        }
        Ok(())
    }
}

impl Drop for NxpDevice {
    /// Hand control back to the firmware.
    fn drop(&mut self) {
        if !self.software && !self.input_software {
            return;
        }
        let restore = |dev: &mut Self| -> Result<()> {
            dev.stop_software_actions()?;
            dev.nxp.send(&[SET, field::SPECIAL, 1])?;
            if dev.spec.extended {
                // The command is only honoured when repeated.
                sleep(SETTLE);
                dev.nxp.send(&[SET, field::SPECIAL, 1])?;
                sleep(SETTLE);
                dev.key_input(|k| {
                    HARDWARE_KEYS
                        .iter()
                        .find(|(key, _)| *key == k)
                        .map_or(INPUT_HID, |(_, a)| *a)
                })?;
            }
            Ok(())
        };
        if let Err(e) = restore(self) {
            log::debug!("{}: could not restore hardware mode: {e}", self.desc.name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::NXP_KEYS;
    use crate::models::{self, Proto};
    use crate::testing::{FakeNxp, assert_packet, iface};
    use gearwise_core::model::{LightingEffect, ZoneState};
    use std::collections::BTreeMap;

    fn open(fake: &FakeNxp, pid: u16) -> NxpDevice {
        let model = models::find(pid).unwrap();
        let Proto::Nxp(spec) = &model.proto else {
            panic!("not an NXP device")
        };
        NxpDevice::open(
            fake.transport(),
            model,
            spec,
            &iface(pid, "/dev/hidraw-test"),
        )
        .unwrap()
    }

    /// An M65 PRO RGB: zones 1, 2, 4, 6; stages 1..=3 enabled plus the sniper bit.
    fn mouse(fake: &FakeNxp) -> NxpDevice {
        fake.ident_for_mouse(0x0206, 1, 0x56);
        fake.mouse_profile(
            0b1111,
            2,
            3,
            1,
            &[(0, 400, 400), (1, 800, 800), (2, 1600, 1600), (3, 3200, 3200)],
        );
        open(fake, 0x1b2e)
    }

    fn keyboard(fake: &FakeNxp, pid: u16) -> NxpDevice {
        fake.ident_for_keyboard(0x0205, 1);
        open(fake, pid)
    }

    fn static_zone(r: u8, g: u8, b: u8) -> ZoneState {
        ZoneState {
            effect: LightingEffect::Static {
                color: Rgb::new(r, g, b),
            },
            brightness: 100,
        }
    }

    /// Writes after the first `skip`, without the report id.
    fn tail(fake: &FakeNxp, skip: usize) -> Vec<Vec<u8>> {
        fake.writes()[skip..].to_vec()
    }

    #[test]
    fn mouse_state_is_read_from_the_stored_profile() {
        let fake = FakeNxp::new();
        let mut dev = mouse(&fake);
        assert_eq!(dev.descriptor().firmware.as_deref(), Some("2.06"));
        let caps = dev.capabilities().clone();
        let dpi = caps.dpi.unwrap();
        assert_eq!((dpi.min, dpi.max, dpi.max_stages), (100, 12_000, 5));
        assert_eq!(caps.report_rates, [125, 250, 500, 1000]);
        let zones: Vec<(u8, &str)> = caps
            .lighting
            .iter()
            .map(|z| (z.id, z.name.as_str()))
            .collect();
        assert_eq!(
            zones,
            [(1, "Front"), (2, "Logo"), (4, "Scroll wheel"), (6, "Side")]
        );
        assert_eq!(caps.lift_off.len(), 5);
        assert!(caps.angle_snapping && caps.per_key.is_none() && !caps.battery);
        let s = dev.read_state().unwrap();
        assert_eq!(
            s.dpi,
            Some(DpiState {
                stages: vec![
                    DpiStage::uniform(800),
                    DpiStage::uniform(1600),
                    DpiStage::uniform(3200)
                ],
                active: 1
            })
        );
        assert_eq!(
            (s.report_rate, s.lift_off, s.angle_snapping),
            (Some(1000), Some(2), Some(true))
        );
    }

    #[test]
    fn dpi_table_maps_to_hardware_stages_one_to_n() {
        let fake = FakeNxp::new();
        let mut dev = mouse(&fake);
        let before = fake.writes().len();
        let table = DpiState {
            stages: vec![DpiStage::uniform(800), DpiStage { x: 1500, y: 1200 }],
            active: 1,
        };
        dev.apply(&Setting::Dpi(table.clone())).unwrap();
        let w = tail(&fake, before);
        assert_eq!(w.len(), 4);
        assert_packet(
            &w[0],
            &[0x07, 0x13, 0xd1, 0x00, 0x00, 0x20, 0x03, 0x20, 0x03],
        );
        assert_packet(
            &w[1],
            &[0x07, 0x13, 0xd2, 0x00, 0x01, 0xdc, 0x05, 0xb0, 0x04],
        );
        assert_packet(&w[2], &[0x07, 0x13, 0x05, 0x00, 0x07]); // sniper bit kept, stages 1 and 2
        assert_packet(&w[3], &[0x07, 0x13, 0x02, 0x00, 0x02]); // hardware stage 2
        assert_eq!(dev.read_state().unwrap().dpi, Some(table));

        let before = fake.writes().len();
        dev.apply(&Setting::DpiActiveStage(0)).unwrap();
        assert_packet(&tail(&fake, before)[0], &[0x07, 0x13, 0x02, 0x00, 0x01]);
        assert_eq!(dev.read_state().unwrap().dpi.unwrap().active, 0);
    }

    #[test]
    fn save_onboard_repeats_the_profile_as_stored_values() {
        let fake = FakeNxp::new();
        let mut dev = mouse(&fake);
        dev.apply(&Setting::Dpi(DpiState {
            stages: vec![DpiStage::uniform(800), DpiStage { x: 1500, y: 1200 }],
            active: 1,
        }))
        .unwrap();
        let before = fake.writes().len();
        dev.save_onboard().unwrap();
        let w = tail(&fake, before);
        assert_eq!(w.len(), 6);
        assert_packet(
            &w[0],
            &[0x07, 0x13, 0xd1, 0x01, 0x00, 0x20, 0x03, 0x20, 0x03],
        );
        assert_packet(
            &w[1],
            &[0x07, 0x13, 0xd2, 0x01, 0x00, 0xdc, 0x05, 0xb0, 0x04],
        );
        assert_packet(&w[2], &[0x07, 0x13, 0x05, 0x01, 0x07]);
        assert_packet(&w[3], &[0x07, 0x13, 0x02, 0x01, 0x02]);
        assert_packet(&w[4], &[0x07, 0x13, 0x03, 0x01, 0x03]); // lift read from the profile: stored 3 -> choice 2 -> 3
        assert_packet(&w[5], &[0x07, 0x13, 0x04, 0x01, 0x01, 0x05]);
    }

    #[test]
    fn lift_snap_and_rate_packets() {
        let fake = FakeNxp::new();
        let mut dev = mouse(&fake);
        let before = fake.writes().len();
        dev.apply(&Setting::LiftOff(4)).unwrap();
        dev.apply(&Setting::AngleSnapping(false)).unwrap();
        dev.apply(&Setting::ReportRate(500)).unwrap();
        let w = tail(&fake, before);
        assert_packet(&w[0], &[0x07, 0x13, 0x03, 0x00, 0x05]);
        assert_packet(&w[1], &[0x07, 0x13, 0x04, 0x00, 0x00, 0x05]);
        assert_packet(&w[2], &[0x07, 0x0a, 0x00, 0x00, 0x02]);
        let s = dev.read_state().unwrap();
        assert_eq!(
            (s.lift_off, s.angle_snapping, s.report_rate),
            (Some(4), Some(false), Some(500))
        );
    }

    #[test]
    fn invalid_settings_are_rejected_before_any_packet() {
        let fake = FakeNxp::new();
        let mut dev = mouse(&fake);
        let before = fake.writes().len();
        let big = DpiState {
            stages: vec![DpiStage::uniform(13_000)],
            active: 0,
        };
        assert!(matches!(
            dev.apply(&Setting::Dpi(big)),
            Err(Error::InvalidValue(_))
        ));
        assert!(matches!(
            dev.apply(&Setting::LiftOff(5)),
            Err(Error::InvalidValue(_))
        ));
        assert!(matches!(
            dev.apply(&Setting::ReportRate(2000)),
            Err(Error::InvalidValue(_))
        ));
        assert!(matches!(
            dev.apply(&Setting::DpiActiveStage(3)),
            Err(Error::InvalidValue(_))
        ));
        assert!(matches!(
            dev.apply(&Setting::GameMode(true)),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            dev.apply(&Setting::PerKey(BTreeMap::new())),
            Err(Error::Unsupported(_))
        ));
        let zone = Setting::Lighting {
            zone: 3,
            state: static_zone(1, 2, 3),
        };
        assert!(
            matches!(dev.apply(&zone), Err(Error::InvalidValue(_))),
            "zone 3 does not exist on this mouse"
        );
        assert_eq!(fake.writes().len(), before);

        let fake = FakeNxp::new();
        let mut kb = keyboard(&fake, 0x1b33);
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
        let dpi = DpiState {
            stages: vec![DpiStage::uniform(800)],
            active: 0,
        };
        assert!(matches!(
            kb.apply(&Setting::Dpi(dpi)),
            Err(Error::Unsupported(_))
        ));
        assert!(
            matches!(
                kb.apply(&Setting::ReportRate(1000)),
                Err(Error::InvalidValue(_))
            ),
            "K70 LUX cannot change its rate"
        );
    }

    #[test]
    fn mouse_lighting_sends_one_packet_with_every_zone() {
        let fake = FakeNxp::new();
        let mut dev = mouse(&fake);
        let before = fake.writes().len();
        dev.apply(&Setting::Lighting {
            zone: 2,
            state: static_zone(255, 0, 0),
        })
        .unwrap();
        let w = tail(&fake, before);
        assert_eq!(w.len(), 2);
        assert_packet(&w[0], &[0x07, 0x05, 0x02, 0x00, 0x01]); // software lighting
        assert_packet(
            &w[1],
            &[
                0x07, 0x22, 0x04, 0x01, 1, 255, 255, 255, 2, 255, 0, 0, 4, 255, 255, 255, 6, 255,
                255, 255,
            ],
        );
        dev.apply(&Setting::Lighting {
            zone: 6,
            state: ZoneState {
                effect: LightingEffect::Off,
                brightness: 100,
            },
        })
        .unwrap();
        let w = tail(&fake, before);
        assert_eq!(w.len(), 3, "software mode is entered once");
        assert_packet(
            &w[2],
            &[
                0x07, 0x22, 0x04, 0x01, 1, 255, 255, 255, 2, 255, 0, 0, 4, 255, 255, 255, 6, 0, 0,
                0,
            ],
        );
    }

    #[test]
    fn mouse_without_a_zone_mask_has_no_lighting() {
        let fake = FakeNxp::new();
        fake.ident_for_mouse(0x0206, 1, 0);
        fake.mouse_profile(0b11, 1, 1, 0, &[(1, 800, 800)]);
        let dev = open(&fake, 0x1b2e);
        assert!(dev.capabilities().lighting.is_empty());
    }

    /// Collect the colour planes sent after `skip` writes: (sequence, bytes) per stream packet.
    fn planes(w: &[Vec<u8>], slots: usize) -> [Vec<u8>; 3] {
        let mut out: [Vec<u8>; 3] = Default::default();
        let mut plane = 0;
        for p in w {
            match p[1] {
                0x7f => {
                    let len = p[3] as usize;
                    out[plane].extend_from_slice(&p[5..5 + len]);
                }
                0x07 if p[2] == 0x28 => {
                    assert_eq!(p[3] as usize, plane + 1, "colour id");
                    assert_eq!(p[4], 3, "three packets per colour");
                    assert_eq!(p[5], if plane == 2 { 2 } else { 1 }, "finish marker");
                    assert_eq!(out[plane].len(), slots);
                    plane += 1;
                }
                _ => {}
            }
        }
        assert_eq!(plane, 3);
        out
    }

    /// Planes the test expects: every mapped key white except key A (slot 0x0f).
    fn expected_planes(slots: usize) -> [Vec<u8>; 3] {
        let mut p = [vec![0u8; slots], vec![0u8; slots], vec![0u8; slots]];
        for (_, slot) in NXP_KEYS {
            for plane in &mut p {
                plane[slot as usize] = 255;
            }
        }
        p[1][0x0f] = 0;
        p[2][0x0f] = 0;
        p
    }

    #[test]
    fn keyboard_per_key_colors_stream_three_planes() {
        let fake = FakeNxp::new();
        let mut dev = keyboard(&fake, 0x1b33); // K70 LUX RGB: plain lighting control
        let before = fake.writes().len();
        dev.apply(&Setting::PerKey(BTreeMap::from([(
            0x04u16,
            Rgb::new(255, 0, 0),
        )])))
        .unwrap();
        let w = tail(&fake, before);
        assert_packet(&w[0], &[0x07, 0x05, 0x02, 0x00, 0x03]);
        assert_eq!(w.len(), 1 + 3 * 4);
        // Red plane: 60 + 60 + 24 bytes, then the colour submit.
        assert_eq!(w[1][1..5], [0x7f, 0x01, 0x3c, 0x00]);
        assert_eq!(w[2][1..5], [0x7f, 0x02, 0x3c, 0x00]);
        assert_eq!(w[3][1..5], [0x7f, 0x03, 0x18, 0x00]);
        assert_packet(&w[4], &[0x07, 0x28, 0x01, 0x03, 0x01]);
        assert_packet(&w[8], &[0x07, 0x28, 0x02, 0x03, 0x01]);
        assert_packet(&w[12], &[0x07, 0x28, 0x03, 0x03, 0x02]);
        assert_eq!(planes(&w[1..], 144), expected_planes(144));
        // Setting the keys zone later re-renders the whole board.
        let before = fake.writes().len();
        dev.apply(&Setting::Lighting {
            zone: 0,
            state: ZoneState {
                effect: LightingEffect::Off,
                brightness: 100,
            },
        })
        .unwrap();
        let w = tail(&fake, before);
        assert_eq!(w.len(), 12);
        assert!(planes(&w, 144).iter().all(|p| p.iter().all(|b| *b == 0)));
    }

    #[test]
    fn k95_platinum_has_168_key_slots() {
        let fake = FakeNxp::new();
        let mut dev = keyboard(&fake, 0x1b2d);
        let before = fake.writes().len();
        dev.apply(&Setting::PerKey(BTreeMap::from([(
            0x04u16,
            Rgb::new(255, 0, 0),
        )])))
        .unwrap();
        let w = tail(&fake, before);
        let stream: Vec<&Vec<u8>> = w.iter().filter(|p| p[1] == 0x7f).collect();
        assert_eq!(stream.len(), 9);
        assert_eq!(stream[2][1..5], [0x7f, 0x03, 0x30, 0x00]);
        let start = w.iter().position(|p| p[1] == 0x7f).unwrap();
        assert_eq!(planes(&w[start..], 168), expected_planes(168));
    }

    #[test]
    fn mk2_keyboards_switch_special_function_and_key_input_first() {
        let fake = FakeNxp::new();
        let mut dev = keyboard(&fake, 0x1b49);
        let before = fake.writes().len();
        dev.apply(&Setting::Lighting {
            zone: 0,
            state: static_zone(1, 2, 3),
        })
        .unwrap();
        let w = tail(&fake, before);
        assert_packet(&w[0], &[0x07, 0x04, 0x02]);
        // 144 keys, 30 per packet: 30, 30, 30, 30, 24, each key reporting as HID and Corsair.
        assert_eq!(w[1][1..5], [0x07, 0x40, 30, 0x00]);
        assert_eq!(w[1][5..9], [0, 0xc0, 1, 0xc0]);
        assert_eq!(w[5][1..5], [0x07, 0x40, 24, 0x00]);
        assert_eq!(w[5][5 + 2 * 23..5 + 2 * 24], [143, 0xc0]);
        assert_packet(&w[6], &[0x07, 0x05, 0x02, 0x00, 0x03, 0x00]);
        assert_eq!(w.len(), 1 + 5 + 1 + 12);

        // Dropping restores hardware control: special function twice, then default key input.
        let before = fake.writes().len();
        drop(dev);
        let w = tail(&fake, before);
        assert_packet(&w[0], &[0x07, 0x04, 0x01]);
        assert_packet(&w[1], &[0x07, 0x04, 0x01]);
        assert_eq!(w[2][1..5], [0x07, 0x40, 30, 0x00]);
        let pair = |k: usize| w[2 + k / 30][5 + 2 * (k % 30)..7 + 2 * (k % 30)].to_vec();
        assert_eq!(pair(0), [0, 0x80]);
        assert_eq!(pair(0x0b), [0x0b, 9]); // MR
        assert_eq!(pair(0x17), [0x17, 10]); // M1
        assert_eq!(pair(0x89), [0x89, 1]); // brightness
        assert_packet(w.last().unwrap(), &[0x07, 0x05, 0x02, 0x00, 0x03, 0x00]);
    }

    #[test]
    fn plain_keyboards_and_mice_only_get_the_special_function_reset_on_drop() {
        let fake = FakeNxp::new();
        let dev = keyboard(&fake, 0x1b33);
        let before = fake.writes().len();
        drop(dev);
        assert_eq!(fake.writes().len(), before, "lighting was never taken over");

        let mut dev = keyboard(&fake, 0x1b33);
        dev.apply(&Setting::Lighting {
            zone: 0,
            state: static_zone(1, 2, 3),
        })
        .unwrap();
        let before = fake.writes().len();
        drop(dev);
        let w = tail(&fake, before);
        assert_eq!(w.len(), 1);
        assert_packet(&w[0], &[0x07, 0x04, 0x01]);
    }

    #[test]
    fn reads_that_never_complete_fail_the_open() {
        let fake = FakeNxp::new();
        let model = models::find(0x1b49).unwrap();
        let Proto::Nxp(spec) = &model.proto else {
            panic!()
        };
        let r = NxpDevice::open(fake.transport(), model, spec, &iface(0x1b49, "x"));
        assert!(matches!(r, Err(Error::Timeout)));
    }
}
