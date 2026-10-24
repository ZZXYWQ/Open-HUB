//! A configurable SteelSeries mouse.
//!
//! These mice cannot report their settings, so the device keeps the last
//! values it applied (starting from the factory defaults) and serves them
//! back from `read_state`.

use std::time::{Duration, Instant};

use gearwise_core::device::{
    Capabilities, Device, DeviceDescriptor, DeviceEvent, DeviceState, Setting,
};
use gearwise_core::model::{
    Battery, ButtonCaps, DeviceKind, DpiCaps, DpiStage, DpiState, LightingZoneCaps,
    VID_STEELSERIES, Vendor, ZoneState,
};
use gearwise_core::transport::HidInterfaceInfo;
use gearwise_core::{Error, Result};

use crate::link::{Link, Packet};
use crate::models::{Field, Model, Spec};
use crate::{battery, buttons, dpi, light};

/// How often `poll_events` asks a wireless mouse for its charge.
const BATTERY_INTERVAL: Duration = Duration::from_secs(60);

pub struct SteelSeriesDevice {
    desc: DeviceDescriptor,
    caps: Capabilities,
    state: DeviceState,
    spec: &'static Spec,
    link: Link,
    /// When the battery was last asked for; `None` before the first time.
    battery_asked: Option<Instant>,
}

impl SteelSeriesDevice {
    /// Wrap an open control interface. Nothing is sent to the mouse.
    pub(crate) fn new(model: &'static Model, link: Link, iface: &HidInterfaceInfo) -> Self {
        let spec = model.spec;
        let id = format!(
            "steelseries:{:04x}:{}",
            model.pid,
            iface.serial.as_deref().unwrap_or(&iface.path)
        );
        Self {
            desc: DeviceDescriptor {
                id,
                model_key: DeviceDescriptor::model_key_for(Vendor::SteelSeries, model.pid),
                vendor: Vendor::SteelSeries,
                kind: DeviceKind::Mouse,
                name: model.name.to_owned(),
                vendor_id: VID_STEELSERIES,
                product_id: model.pid,
                connection: model.connection,
                // The firmware answers on another HID node than commands go
                // to, so the version cannot be read reliably.
                firmware: None,
                serial: iface.serial.clone(),
                simulated: false,
            },
            caps: capabilities(spec),
            state: factory_state(spec),
            spec,
            link,
            battery_asked: None,
        }
    }

    /// Validate `setting` and encode it. Returns the setting as the mouse
    /// will hold it (values snapped to what the hardware supports).
    fn plan(&self, setting: &Setting) -> Result<(Setting, Vec<Packet>)> {
        let spec = self.spec;
        match setting {
            Setting::Dpi(want) => {
                let (applied, packets) = dpi::plan(&spec.dpi, want)?;
                Ok((Setting::Dpi(applied), packets))
            }
            Setting::DpiActiveStage(index) => {
                let current = self.state.dpi.as_ref().ok_or(Error::Unsupported("DPI"))?;
                let (applied, packets) = dpi::select(&spec.dpi, current, *index)?;
                Ok((Setting::Dpi(applied), packets))
            }
            Setting::ReportRate(hz) => {
                let code = spec
                    .rate
                    .codes
                    .iter()
                    .find(|(rate, _)| rate == hz)
                    .map(|(_, code)| *code)
                    .ok_or_else(|| Error::invalid(format!("{hz} Hz")))?;
                let bytes = [spec.rate.cmd, &[code]].concat();
                Ok((setting.clone(), vec![Packet::output(bytes)]))
            }
            Setting::Lighting { zone, state } => {
                let lighting = spec.light.as_ref().ok_or(Error::Unsupported("lighting"))?;
                let (applied, packets) = light::plan(lighting, *zone, state)?;
                Ok((
                    Setting::Lighting {
                        zone: *zone,
                        state: applied,
                    },
                    packets,
                ))
            }
            Setting::Button { id, action } => {
                let table = spec
                    .buttons
                    .as_ref()
                    .ok_or(Error::Unsupported("button remapping"))?;
                let packet = buttons::plan(table, &self.state.buttons, *id, action)?;
                Ok((setting.clone(), vec![packet]))
            }
            Setting::PerKey(_) => Err(Error::Unsupported("per-key lighting")),
            Setting::OnboardProfile(_) => Err(Error::Unsupported("onboard profiles")),
            Setting::LiftOff(_) => Err(Error::Unsupported("lift-off distance")),
            Setting::AngleSnapping(_) => Err(Error::Unsupported("angle snapping")),
            Setting::GameMode(_) => Err(Error::Unsupported("game mode")),
        }
    }

    /// Ask a wireless mouse for its charge. `None` when it does not answer
    /// (asleep, out of range) or the model has no battery.
    fn read_battery(&mut self) -> Result<Option<Battery>> {
        let Some(proto) = self.spec.battery else {
            return Ok(None);
        };
        self.battery_asked = Some(Instant::now());
        let reply = self.link.query(&battery::request(proto), |r| {
            battery::parse(proto, r).is_some()
        });
        match reply {
            Ok(reply) => Ok(reply.and_then(|r| battery::parse(proto, &r))),
            Err(Error::Timeout) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// What the UI may offer for this model.
fn capabilities(spec: &Spec) -> Capabilities {
    let cpi = &spec.dpi.cpi;
    Capabilities {
        dpi: Some(DpiCaps {
            min: cpi.min,
            max: cpi.max,
            step: cpi.step,
            max_stages: spec.dpi.stages,
            separate_xy: false,
        }),
        report_rates: spec.rate.codes.iter().map(|(hz, _)| *hz).collect(),
        lighting: spec
            .light
            .iter()
            .flat_map(|light| {
                light
                    .zones
                    .iter()
                    .zip(0u8..)
                    .map(|(zone, id)| LightingZoneCaps {
                        keys: Vec::new(),
                        id,
                        name: zone.name.to_owned(),
                        effects: light.effects().to_vec(),
                        brightness: true,
                    })
            })
            .collect(),
        buttons: spec
            .buttons
            .iter()
            .flat_map(|table| table.fields)
            .filter_map(|field| match field {
                Field::Button(b) => Some(ButtonCaps {
                    id: b.id,
                    name: b.name.to_owned(),
                    remappable: true,
                    default_action: b.default.action(),
                    hotspot: Some(b.hotspot),
                }),
                Field::Fixed(_) => None,
            })
            .collect(),
        battery: spec.battery.is_some(),
        ..Default::default()
    }
}

/// The settings rivalcfg documents as the mice's out-of-the-box values.
fn factory_state(spec: &Spec) -> DeviceState {
    DeviceState {
        dpi: Some(DpiState {
            stages: spec
                .dpi
                .defaults
                .iter()
                .map(|d| DpiStage::uniform(*d))
                .collect(),
            active: 0,
        }),
        report_rate: Some(1000),
        lighting: spec
            .light
            .iter()
            .flat_map(|light| light.zones.iter().zip(0u8..))
            .map(|(zone, id)| {
                (
                    id,
                    ZoneState {
                        effect: zone.default,
                        brightness: 100,
                    },
                )
            })
            .collect(),
        ..Default::default()
    }
}

impl Device for SteelSeriesDevice {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }

    fn capabilities(&self) -> &Capabilities {
        &self.caps
    }

    /// The last applied settings; the battery is read live.
    fn read_state(&mut self) -> Result<DeviceState> {
        if let Some(battery) = self.read_battery()? {
            self.state.battery = Some(battery);
        }
        Ok(self.state.clone())
    }

    fn apply(&mut self, setting: &Setting) -> Result<()> {
        let (applied, packets) = self.plan(setting)?;
        for packet in &packets {
            self.link.send(packet)?;
        }
        applied.apply_to(&mut self.state);
        Ok(())
    }

    fn poll_events(&mut self) -> Vec<DeviceEvent> {
        if self.spec.battery.is_none()
            || self
                .battery_asked
                .is_some_and(|t| t.elapsed() < BATTERY_INTERVAL)
        {
            return Vec::new();
        }
        match self.read_battery() {
            Ok(Some(battery)) if self.state.battery != Some(battery) => {
                self.state.battery = Some(battery);
                vec![DeviceEvent::BatteryChanged(battery)]
            }
            Ok(_) => Vec::new(),
            Err(e) => {
                log::debug!("{}: battery query failed: {e}", self.desc.id);
                Vec::new()
            }
        }
    }

    fn save_onboard(&mut self) -> Result<()> {
        self.link.send(&Packet::output(self.spec.save.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use gearwise_core::model::{
        ButtonAction, DpiFunction, EffectKind, LightingEffect, Modifiers, MouseButton, Rgb, media,
    };
    use gearwise_core::transport::{MockOp, MockRecord, MockReply};

    use super::*;
    use crate::models::{self, MODELS};
    use crate::testutil::{Log, drain, feature, hex, open_with, output, silent};

    fn dpi_setting(stages: &[u16], active: u8) -> Setting {
        Setting::Dpi(DpiState {
            stages: stages.iter().map(|d| DpiStage::uniform(*d)).collect(),
            active,
        })
    }

    fn color(zone: u8, r: u8, g: u8, b: u8) -> Setting {
        Setting::Lighting {
            zone,
            state: ZoneState {
                effect: gearwise_core::model::LightingEffect::Static {
                    color: Rgb::new(r, g, b),
                },
                brightness: 100,
            },
        }
    }

    fn key(usage: u8) -> ButtonAction {
        ButtonAction::Key {
            usage,
            mods: Modifiers(0),
        }
    }

    fn button(id: u16, action: ButtonAction) -> Setting {
        Setting::Button { id, action }
    }

    /// Apply `settings` in order and return everything written for them.
    fn run(pid: u16, settings: &[Setting]) -> Vec<MockRecord> {
        let (mut dev, log) = silent(pid);
        for s in settings {
            dev.apply(s).unwrap();
        }
        drain(&log)
    }

    // -- Rival 3 (golden bytes from rivalcfg) -------------------------------

    #[test]
    fn rival_3_commands() {
        let written = run(
            0x1824,
            &[
                dpi_setting(&[800, 1600], 0),
                dpi_setting(&[1200, 3200], 1),
                Setting::ReportRate(500),
                color(0, 255, 0, 0),
                color(3, 16, 32, 48),
                Setting::Button {
                    id: 4,
                    action: key(0x04),
                },
                button(6, ButtonAction::Media(media::VOLUME_UP)),
                button(2, ButtonAction::Mouse(MouseButton::Middle)),
                button(5, ButtonAction::Disabled),
            ],
        );
        assert_eq!(
            written[..7],
            [
                output(&[0x0b, 0x00, 2, 1, 0x12, 0x24]),
                output(&[0x0b, 0x00, 2, 2, 0x1b, 0x49]),
                output(&[0x04, 0x00, 0x02]),
                output(&[0x05, 0x00, 0x01, 0xff, 0x00, 0x00, 0x64]),
                output(&[0x05, 0x00, 0x04, 0x10, 0x20, 0x30, 0x64]),
                // First button change: only button 4 differs from the factory table.
                output(&hex(
                    "07 00 01 00 02 00 03 00 33 04 05 00 30 00 31 00 32 00"
                )),
                // Button 6 now sends VolumeUp as a two-byte media slot.
                output(&hex(
                    "07 00 01 00 02 00 03 00 33 04 05 00 34 e9 31 00 32 00"
                )),
            ]
        );
        assert_eq!(
            written[8],
            output(&hex(
                "07 00 01 00 03 00 03 00 33 04 00 00 34 e9 31 00 32 00"
            ))
        );
    }

    #[test]
    fn rival_3_factory_button_table_and_save() {
        let (mut dev, log) = silent(0x1824);
        dev.apply(&button(1, ButtonAction::Default)).unwrap();
        dev.save_onboard().unwrap();
        assert_eq!(
            drain(&log),
            [
                output(&hex(
                    "07 00 01 00 02 00 03 00 04 00 05 00 30 00 31 00 32 00"
                )),
                output(&[0x09, 0x00]),
            ]
        );
    }

    // -- Aerox 5 ------------------------------------------------------------

    #[test]
    fn aerox_5_commands() {
        let written = run(
            0x1850,
            &[
                dpi_setting(&[400, 800, 1200, 2400, 3200], 2),
                dpi_setting(&[1700], 0),
                Setting::DpiActiveStage(0),
                Setting::ReportRate(250),
                color(0, 255, 0, 0),
                color(1, 10, 11, 12),
                color(2, 1, 2, 3),
            ],
        );
        assert_eq!(
            written,
            [
                output(&[0x2d, 5, 2, 0x04, 0x09, 0x0d, 0x1b, 0x26]),
                output(&[0x2d, 1, 0, 0x13]),
                output(&[0x2d, 1, 0, 0x13]),
                output(&[0x2b, 0x03]),
                output(&[0x21, 0x01, 0xff, 0x00, 0x00]),
                output(&[0x21, 0x02, 0, 0, 0, 0x0a, 0x0b, 0x0c]),
                output(&[0x21, 0x04, 0, 0, 0, 0, 0, 0, 0x01, 0x02, 0x03]),
            ]
        );
    }

    #[test]
    fn aerox_5_buttons_and_save() {
        let (mut dev, log) = silent(0x1850);
        for s in [
            button(4, key(0x04)),
            button(5, ButtonAction::Media(media::PLAY_PAUSE)),
            button(6, ButtonAction::Disabled),
            button(2, ButtonAction::Mouse(MouseButton::Middle)),
        ] {
            dev.apply(&s).unwrap();
        }
        dev.save_onboard().unwrap();
        let written = drain(&log);
        // Left, right (as middle), middle, back (A), forward (Play), the disabled
        // DPI button, three unused slots, then the wheel.
        let mut table =
            hex("2a 01 00 00 00 00 03 00 00 00 00 03 00 00 00 00 51 04 00 00 00 61 cd 00 00 00");
        table.extend([0; 20]);
        table.extend(hex("31 00 00 00 00 32 00 00 00 00"));
        assert_eq!(written[3], output(&table));
        assert_eq!(written[4], output(&[0x11, 0x00]));
    }

    #[test]
    fn active_stage_resends_the_stage_list() {
        let (mut dev, log) = silent(0x1850);
        dev.apply(&dpi_setting(&[400, 800, 1200], 0)).unwrap();
        dev.apply(&Setting::DpiActiveStage(2)).unwrap();
        let written = drain(&log);
        assert_eq!(written[1], output(&[0x2d, 3, 2, 0x04, 0x09, 0x0d]));
        assert_eq!(dev.read_state().unwrap().dpi.unwrap().active, 2);
    }

    // -- Aerox 3 Wireless: dongle and cable ---------------------------------

    #[test]
    fn dongle_commands_carry_the_flag() {
        let written = run(
            0x1838,
            &[
                dpi_setting(&[400, 800], 1),
                Setting::ReportRate(1000),
                color(2, 9, 8, 7),
            ],
        );
        assert_eq!(
            written,
            [
                output(&[0x6d, 2, 1, 0x04, 0x09]),
                output(&[0x6b, 0x00]),
                output(&[0x61, 0x01, 0x02, 0x09, 0x08, 0x07]),
            ]
        );
        let (mut dev, log) = silent(0x1838);
        dev.save_onboard().unwrap();
        assert_eq!(drain(&log), [output(&[0x51, 0x00])]);
    }

    #[test]
    fn cable_mode_commands_have_no_flag() {
        let written = run(
            0x183a,
            &[
                dpi_setting(&[400, 800], 1),
                Setting::ReportRate(125),
                color(0, 9, 8, 7),
            ],
        );
        assert_eq!(
            written,
            [
                output(&[0x2d, 2, 1, 0x04, 0x09]),
                output(&[0x2b, 0x03]),
                output(&[0x21, 0x01, 0x00, 0x09, 0x08, 0x07]),
            ]
        );
    }

    #[test]
    fn dongle_replies_are_read_after_every_command() {
        // The firmware acknowledges each command; a battery query must not
        // mistake a leftover acknowledgement for its answer.
        let (mut dev, _) = open_with(0x1838, |op| match op {
            MockOp::Write(d) if d[1] == 0xd2 => MockReply::input(vec![0xd2, 0x0b]),
            _ => MockReply::input(vec![0x00; 64]),
        });
        dev.apply(&Setting::ReportRate(500)).unwrap();
        dev.apply(&color(0, 1, 2, 3)).unwrap();
        let state = dev.read_state().unwrap();
        assert_eq!(
            state.battery,
            Some(Battery {
                percent: Some(50),
                charging: false
            })
        );
    }

    // -- Rival 310 / Sensei / Rival 600 -------------------------------------

    #[test]
    fn rival_310_commands() {
        let red = Rgb::new(255, 0, 0);
        let written = run(
            0x1720,
            &[
                dpi_setting(&[800, 12000], 0),
                Setting::ReportRate(500),
                color(0, 255, 0, 0),
                color(1, 1, 2, 3),
                Setting::Lighting {
                    zone: 0,
                    state: ZoneState {
                        effect: LightingEffect::Breathing {
                            color: red,
                            period_ms: 4000,
                        },
                        brightness: 100,
                    },
                },
                Setting::Lighting {
                    zone: 0,
                    state: ZoneState {
                        effect: LightingEffect::Off,
                        brightness: 100,
                    },
                },
            ],
        );
        let stops = |dump: &str| hex(dump);
        assert_eq!(
            written,
            [
                output(&[0x53, 0x00, 1, 7, 0x00, 0x42]),
                output(&[0x53, 0x00, 2, 119, 0x00, 0x42]),
                output(&[0x54, 0x00, 0x02]),
                feature(&stops(
                    "5b 00 00 e8 03 00 00 00 00 00 00 00 00 00 00 00 00 00 00 01 00 00 00 00 00 00 00 01 \
                     ff 00 00 ff 00 00 00"
                )),
                feature(&stops(
                    "5b 00 01 e8 03 00 00 00 00 00 00 00 00 00 00 00 00 00 00 01 00 00 00 00 00 00 00 01 \
                     01 02 03 01 02 03 00"
                )),
                feature(&stops(
                    "5b 00 00 a0 0f 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 03 \
                     00 00 00 00 00 00 00 ff 00 00 7f 00 00 00 80"
                )),
                feature(&stops(
                    "5b 00 00 e8 03 00 00 00 00 00 00 00 00 00 00 00 00 00 00 01 00 00 00 00 00 00 00 01 \
                     00 00 00 00 00 00 00"
                )),
            ]
        );
    }

    #[test]
    fn rival_310_buttons_and_save() {
        let (mut dev, log) = silent(0x1720);
        dev.apply(&button(4, key(0x04))).unwrap();
        dev.apply(&button(6, ButtonAction::Disabled)).unwrap();
        dev.apply(&button(2, ButtonAction::Mouse(MouseButton::Middle)))
            .unwrap();
        dev.save_onboard().unwrap();
        let written = drain(&log);
        assert_eq!(
            written[2],
            output(&hex(
                "31 00 01 00 00 00 00 03 00 00 00 00 03 00 00 00 00 51 04 00 00 00 05 00 00 00 00 \
                 00 00 00 00 00"
            ))
        );
        assert_eq!(written[3], output(&[0x59, 0x00]));
    }

    #[test]
    fn sensei_310_has_page_keys_on_the_right_side() {
        let (mut dev, log) = silent(0x1722);
        dev.apply(&button(1, ButtonAction::Default)).unwrap();
        assert_eq!(
            drain(&log),
            [output(&hex(
                "31 00 01 00 00 00 00 02 00 00 00 00 03 00 00 00 00 04 00 00 00 00 05 00 00 00 00 \
                 51 4e 00 00 00 51 4b 00 00 00 30 00 00 00 00"
            ))]
        );
        let caps = dev.capabilities();
        assert_eq!(caps.buttons.len(), 8);
        assert_eq!(caps.buttons[5].default_action, key(0x4e));
        assert_eq!(caps.buttons[6].default_action, key(0x4b));
    }

    #[test]
    fn sensei_ten_announces_stages_as_flags() {
        let written = run(
            0x1832,
            &[
                dpi_setting(&[400, 800, 1200, 2400, 3200], 4),
                dpi_setting(&[400, 18000, 1250], 1),
                Setting::ReportRate(125),
            ],
        );
        assert_eq!(
            written,
            [
                output(&hex("55 00 1f 05 08 00 10 00 18 00 30 00 40 00")),
                output(&hex("55 00 07 02 08 00 68 01 19 00")),
                output(&[0x54, 0x00, 0x04]),
            ]
        );
    }

    #[test]
    fn rival_600_commands() {
        let written = run(
            0x1724,
            &[
                dpi_setting(&[800, 1600], 0),
                color(0, 1, 2, 3),
                color(7, 1, 2, 3),
                button(4, key(0x04)),
                button(7, ButtonAction::Disabled),
                button(2, ButtonAction::Mouse(MouseButton::Middle)),
            ],
        );
        assert_eq!(written[0], output(&[0x03, 0x00, 1, 7, 0x00, 0x42]));
        assert_eq!(written[1], output(&[0x03, 0x00, 2, 15, 0x00, 0x42]));
        assert_eq!(
            written[2],
            feature(&hex(
                "05 00 00 00 00 00 00 00 e8 03 00 00 00 00 00 00 00 00 00 00 00 00 00 00 \
                 01 00 00 00 00 01 01 02 03 01 02 03 00"
            ))
        );
        assert_eq!(
            written[3],
            feature(&hex(
                "05 00 07 00 00 00 00 07 e8 03 00 00 00 00 00 00 00 00 00 00 00 00 00 00 \
                 01 00 00 00 00 01 01 02 03 01 02 03 00"
            ))
        );
        assert_eq!(
            written[6],
            feature(&hex(
                "31 00 01 00 00 00 00 03 00 00 00 00 03 00 00 00 00 51 04 00 00 00 05 00 00 00 00 00 00 \
                 00 00 00 00 00 00 00 00"
            ))
        );
    }

    // -- Rival 100 / 110 / 300 / Kana ---------------------------------------

    #[test]
    fn rival_100_commands() {
        let red = Rgb::new(255, 0, 0);
        let written = run(
            0x1702,
            &[
                dpi_setting(&[1250, 4000], 0),
                Setting::ReportRate(250),
                color(0, 1, 2, 3),
                Setting::Lighting {
                    zone: 0,
                    state: ZoneState {
                        effect: LightingEffect::Breathing {
                            color: red,
                            period_ms: 5000,
                        },
                        brightness: 100,
                    },
                },
            ],
        );
        assert_eq!(
            written,
            [
                output(&[0x03, 1, 5]),
                output(&[0x03, 2, 1]),
                output(&[0x04, 0x00, 0x03]),
                output(&[0x05, 0x00, 1, 2, 3]),
                output(&[0x07, 0x00, 1]),
                output(&[0x05, 0x00, 255, 0, 0]),
                output(&[0x07, 0x00, 3]),
            ]
        );
    }

    #[test]
    fn rival_110_pads_every_packet_to_32_bytes() {
        let written = run(
            0x1729,
            &[dpi_setting(&[800, 1600], 0), Setting::ReportRate(125)],
        );
        let padded = |head: &[u8]| {
            let mut p = head.to_vec();
            p.resize(32, 0);
            output(&p)
        };
        assert_eq!(
            written,
            [
                padded(&[0x03, 1, 0x12]),
                padded(&[0x03, 2, 0x24]),
                padded(&[0x04, 0x00, 0x04]),
            ]
        );
    }

    #[test]
    fn rival_300_lights_logo_and_wheel() {
        let written = run(
            0x1710,
            &[
                dpi_setting(&[800, 6500], 0),
                color(0, 1, 2, 3),
                color(1, 4, 5, 6),
                button(4, key(0x04)),
                button(6, ButtonAction::Disabled),
            ],
        );
        assert_eq!(written[0], output(&[0x03, 1, 0x10]));
        assert_eq!(written[1], output(&[0x03, 2, 0x82]));
        assert_eq!(written[2], output(&[0x08, 1, 1, 2, 3]));
        assert_eq!(written[3], output(&[0x07, 1, 1]));
        assert_eq!(written[4], output(&[0x08, 2, 4, 5, 6]));
        assert_eq!(
            written[7],
            output(&hex(
                "31 00 01 00 00 00 00 02 00 00 00 00 03 00 00 00 00 51 04 00 00 00 05 00 00 00 00 \
                 00 00 00 00 00"
            ))
        );
    }

    #[test]
    fn kana_v2_has_two_fixed_settings() {
        let written = run(
            0x137a,
            &[dpi_setting(&[1200, 4000], 0), Setting::ReportRate(500)],
        );
        assert_eq!(
            written,
            [
                output(&[0x03, 1, 6]),
                output(&[0x03, 2, 1]),
                output(&[0x04, 0x00, 0x02])
            ]
        );
        let (dev, _) = silent(0x137a);
        assert!(dev.capabilities().lighting.is_empty());
        assert!(dev.capabilities().buttons.is_empty());
    }

    // -- Prime ---------------------------------------------------------------

    #[test]
    fn prime_commands() {
        let written = run(
            0x182e,
            &[
                dpi_setting(&[400, 18000, 1250], 1),
                Setting::ReportRate(250),
                color(0, 1, 2, 3),
            ],
        );
        assert_eq!(
            written,
            [
                output(&hex("61 03 01 08 00 68 01 19 00")),
                output(&[0x5d, 0x03]),
                output(&hex(
                    "62 01 01 02 03 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 ff"
                )),
            ]
        );
        let (mut dev, log) = silent(0x182e);
        dev.save_onboard().unwrap();
        assert_eq!(drain(&log), [output(&[0x59])]);
    }

    #[test]
    fn prime_mini_commands() {
        let written = run(0x184d, &[dpi_setting(&[800, 1600], 0), color(0, 1, 2, 3)]);
        assert_eq!(
            written,
            [
                output(&hex("2d 02 00 10 00 20 00")),
                output(&[0x21, 0x00, 1, 2, 3])
            ]
        );
    }

    // -- Rival 3 Wireless ------------------------------------------------------

    #[test]
    fn rival_3_wireless_commands_and_battery() {
        let (mut dev, log) = open_with(0x1830, |op| match op {
            MockOp::Write(d) if d[1..] == [0xaa, 0x01] => MockReply::input(vec![80, 0, 1]),
            _ => MockReply::none(),
        });
        dev.apply(&dpi_setting(&[800, 1600, 3200], 2)).unwrap();
        dev.apply(&Setting::ReportRate(1000)).unwrap();
        dev.apply(&Setting::ReportRate(125)).unwrap();
        dev.save_onboard().unwrap();
        let state = dev.read_state().unwrap();
        assert_eq!(
            drain(&log),
            [
                output(&hex("20 03 03 09 00 12 00 26 00")),
                output(&[0x17, 0x00]),
                output(&[0x17, 0x03]),
                output(&[0x09]),
                output(&[0xaa, 0x01]),
            ]
        );
        assert_eq!(
            state.battery,
            Some(Battery {
                percent: Some(80),
                charging: true
            })
        );
        assert!(dev.capabilities().battery);
        assert!(dev.capabilities().lighting.is_empty());
    }

    // -- Validation ---------------------------------------------------------

    fn rejected(pid: u16, setting: &Setting) -> Error {
        let (mut dev, log) = silent(pid);
        let err = dev.apply(setting).expect_err("setting should be refused");
        assert!(drain(&log).is_empty(), "nothing may reach the mouse");
        err
    }

    #[test]
    fn refuses_bad_dpi_without_writing() {
        for bad in [
            dpi_setting(&[], 0),
            dpi_setting(&[800; 6], 0),
            dpi_setting(&[800, 1600], 2),
            dpi_setting(&[100], 0),
            dpi_setting(&[9000], 0),
        ] {
            assert!(
                matches!(rejected(0x1824, &bad), Error::InvalidValue(_)),
                "{bad:?}"
            );
        }
        let split = Setting::Dpi(DpiState {
            stages: vec![DpiStage { x: 800, y: 1600 }],
            active: 0,
        });
        assert!(matches!(rejected(0x1824, &split), Error::InvalidValue(_)));
        assert!(matches!(
            rejected(0x1824, &Setting::DpiActiveStage(5)),
            Error::InvalidValue(_)
        ));
    }

    #[test]
    fn two_stage_mice_cannot_switch_stages() {
        assert!(matches!(
            rejected(0x1720, &Setting::DpiActiveStage(1)),
            Error::Unsupported(_)
        ));
        assert!(matches!(
            rejected(0x1720, &dpi_setting(&[800; 3], 0)),
            Error::InvalidValue(_)
        ));
    }

    #[test]
    fn refuses_unlisted_rates() {
        assert!(matches!(
            rejected(0x1824, &Setting::ReportRate(2000)),
            Error::InvalidValue(_)
        ));
        assert!(matches!(
            rejected(0x1824, &Setting::ReportRate(0)),
            Error::InvalidValue(_)
        ));
    }

    #[test]
    fn refuses_bad_lighting() {
        assert!(matches!(
            rejected(0x1850, &color(3, 1, 2, 3)),
            Error::InvalidValue(_)
        ));
        let wave = Setting::Lighting {
            zone: 0,
            state: ZoneState {
                effect: LightingEffect::default_for(EffectKind::Wave, Rgb::WHITE),
                brightness: 100,
            },
        };
        assert!(matches!(rejected(0x1850, &wave), Error::Unsupported(_)));
        let breathing = Setting::Lighting {
            zone: 0,
            state: ZoneState {
                effect: LightingEffect::default_for(EffectKind::Breathing, Rgb::WHITE),
                brightness: 100,
            },
        };
        assert!(matches!(
            rejected(0x1850, &breathing),
            Error::Unsupported(_)
        ));
        // Mice without lighting.
        assert!(matches!(
            rejected(0x137a, &color(0, 1, 2, 3)),
            Error::Unsupported(_)
        ));
        assert!(matches!(
            rejected(0x1830, &color(0, 1, 2, 3)),
            Error::Unsupported(_)
        ));
    }

    #[test]
    fn refuses_bad_button_requests() {
        assert!(matches!(
            rejected(0x1824, &button(9, ButtonAction::Disabled)),
            Error::InvalidValue(_)
        ));
        // Rival 3 has no wheel-rotation entry among its remappable buttons.
        assert!(matches!(
            rejected(0x1824, &button(49, ButtonAction::Disabled)),
            Error::InvalidValue(_)
        ));
        assert!(matches!(
            rejected(0x1824, &button(1, ButtonAction::Macro("m".into()))),
            Error::Unsupported(_)
        ));
        assert!(matches!(
            rejected(
                0x1850,
                &button(1, ButtonAction::Dpi(DpiFunction::Shift(400)))
            ),
            Error::Unsupported(_)
        ));
        let chord = ButtonAction::Key {
            usage: 0x04,
            mods: Modifiers(
                Modifiers::LCTRL | Modifiers::LSHIFT | Modifiers::LALT | Modifiers::LGUI,
            ),
        };
        assert!(matches!(
            rejected(0x1850, &button(1, chord)),
            Error::Unsupported(_)
        ));
        // The original Rival 3 slots are two bytes: no modifiers.
        let ctrl_a = ButtonAction::Key {
            usage: 0x04,
            mods: Modifiers(Modifiers::LCTRL),
        };
        assert!(matches!(
            rejected(0x1824, &button(1, ctrl_a)),
            Error::Unsupported(_)
        ));
        // Mice without a table.
        assert!(matches!(
            rejected(0x137a, &button(1, ButtonAction::Disabled)),
            Error::Unsupported(_)
        ));
        assert!(matches!(
            rejected(0x185a, &button(1, ButtonAction::Disabled)),
            Error::Unsupported(_)
        ));
    }

    #[test]
    fn modifier_chords_use_the_modifier_usages() {
        let (mut dev, log) = silent(0x1850);
        let ctrl_shift_a = ButtonAction::Key {
            usage: 0x04,
            mods: Modifiers(Modifiers::LCTRL | Modifiers::LSHIFT),
        };
        dev.apply(&button(3, ctrl_shift_a)).unwrap();
        let written = drain(&log);
        let MockRecord::Write(bytes) = &written[0] else {
            panic!("expected an output report");
        };
        assert_eq!(bytes[1 + 1 + 10..1 + 1 + 15], [0x51, 0xe0, 0xe1, 0x04, 0]);
    }

    #[test]
    fn refuses_settings_these_mice_do_not_have() {
        for s in [
            Setting::PerKey(BTreeMap::new()),
            Setting::OnboardProfile(0),
            Setting::LiftOff(0),
            Setting::AngleSnapping(true),
            Setting::GameMode(true),
        ] {
            assert!(
                matches!(rejected(0x1850, &s), Error::Unsupported(_)),
                "{s:?}"
            );
        }
    }

    // -- State ---------------------------------------------------------------

    fn state_of(dev: &mut Box<dyn Device>) -> DeviceState {
        dev.read_state().unwrap()
    }

    #[test]
    fn state_starts_at_the_factory_defaults() {
        let (mut dev, _) = silent(0x1824);
        let s = state_of(&mut dev);
        assert_eq!(
            s.dpi.unwrap().stages,
            [DpiStage::uniform(800), DpiStage::uniform(1600)]
        );
        assert_eq!(s.report_rate, Some(1000));
        assert_eq!(s.lighting.len(), 4);
        assert_eq!(
            s.lighting[&3].effect,
            LightingEffect::Static {
                color: Rgb::new(128, 0, 128)
            }
        );
        assert!(s.buttons.is_empty());
        assert_eq!(s.battery, None);
    }

    #[test]
    fn applied_settings_are_remembered() {
        let (mut dev, _) = silent(0x1850);
        dev.apply(&dpi_setting(&[849, 3200], 1)).unwrap();
        dev.apply(&Setting::ReportRate(500)).unwrap();
        dev.apply(&color(1, 1, 2, 3)).unwrap();
        dev.apply(&button(4, key(0x04))).unwrap();
        dev.apply(&button(5, ButtonAction::Disabled)).unwrap();
        dev.apply(&button(5, ButtonAction::Default)).unwrap();
        let s = state_of(&mut dev);
        let dpi = s.dpi.unwrap();
        assert_eq!(
            dpi.stages,
            [DpiStage::uniform(800), DpiStage::uniform(3200)],
            "snapped"
        );
        assert_eq!(dpi.active, 1);
        assert_eq!(s.report_rate, Some(500));
        assert_eq!(
            s.lighting[&1].effect,
            LightingEffect::Static {
                color: Rgb::new(1, 2, 3)
            }
        );
        assert_eq!(s.buttons.get(&4), Some(&key(0x04)));
        assert!(
            !s.buttons.contains_key(&5),
            "default mappings are not stored"
        );
    }

    #[test]
    fn failed_writes_leave_the_state_alone() {
        use gearwise_core::transport::{HidTransport, MockTransport};
        struct Broken(MockTransport);
        impl HidTransport for Broken {
            fn write(&mut self, _: &[u8]) -> Result<usize> {
                Err(Error::Disconnected)
            }
            fn read_timeout(&mut self, b: &mut [u8], t: i32) -> Result<usize> {
                self.0.read_timeout(b, t)
            }
            fn send_feature_report(&mut self, d: &[u8]) -> Result<()> {
                self.0.send_feature_report(d)
            }
            fn get_feature_report(&mut self, b: &mut [u8]) -> Result<usize> {
                self.0.get_feature_report(b)
            }
        }
        let model = models::find(0x1850).unwrap();
        let link = Link::new(
            Box::new(Broken(MockTransport::new(|_| MockReply::none()))),
            model.spec.wire,
            Duration::ZERO,
        );
        let info = crate::testutil::iface(0x1850);
        let mut dev = SteelSeriesDevice::new(model, link, &info);
        let before = dev.read_state().unwrap();
        assert!(matches!(
            dev.apply(&Setting::ReportRate(500)),
            Err(Error::Disconnected)
        ));
        assert_eq!(dev.read_state().unwrap(), before);
    }

    // -- Capabilities --------------------------------------------------------

    #[test]
    fn capabilities_describe_the_hardware() {
        let (dev, _) = silent(0x1850);
        let caps = dev.capabilities();
        let dpi = caps.dpi.as_ref().unwrap();
        assert_eq!(
            (dpi.min, dpi.max, dpi.step, dpi.max_stages),
            (100, 18000, 100, 5)
        );
        assert!(!dpi.separate_xy);
        assert_eq!(caps.report_rates, [125, 250, 500, 1000]);
        let zones: Vec<&str> = caps.lighting.iter().map(|z| z.name.as_str()).collect();
        assert_eq!(zones, ["Front", "Middle", "Rear"]);
        assert_eq!(caps.lighting[2].id, 2);
        assert_eq!(
            caps.lighting[0].effects,
            [EffectKind::Off, EffectKind::Static]
        );
        assert!(caps.lighting.iter().all(|z| z.brightness));
        let buttons: Vec<(u16, &str)> = caps
            .buttons
            .iter()
            .map(|b| (b.id, b.name.as_str()))
            .collect();
        assert_eq!(
            buttons,
            [
                (1, "Left click"),
                (2, "Right click"),
                (3, "Middle click"),
                (4, "Back"),
                (5, "Forward"),
                (6, "DPI button")
            ]
        );
        assert_eq!(caps.buttons[0].hotspot, Some((0.30, 0.20)));
        assert_eq!(caps.buttons[3].hotspot, Some((0.10, 0.50)));
        assert_eq!(caps.buttons[4].hotspot, Some((0.11, 0.40)));
        assert_eq!(
            caps.buttons[5].default_action,
            ButtonAction::Dpi(DpiFunction::Cycle)
        );
        assert!(!caps.battery);
        assert_eq!(caps.onboard_profiles, 0);
    }

    #[test]
    fn colour_stop_mice_offer_breathing_and_cycling() {
        let (dev, _) = silent(0x1724);
        let zones = &dev.capabilities().lighting;
        assert_eq!(zones.len(), 8);
        assert!(zones[0].effects.contains(&EffectKind::ColorCycle));
        assert!(zones[0].effects.contains(&EffectKind::Breathing));
        assert_eq!(zones[1].name, "Logo");
    }

    // -- Battery events ----------------------------------------------------------

    fn battery_log(log: &Log) -> usize {
        drain(log).len()
    }

    #[test]
    fn battery_is_polled_once_a_minute() {
        let (mut dev, log) = open_with(0x1840, |op| match op {
            MockOp::Write(d) if d[1] == 0xd2 => MockReply::input(vec![0x92, 0x80 | 0x0b]),
            _ => MockReply::none(),
        });
        let events = dev.poll_events();
        assert_eq!(
            events,
            [DeviceEvent::BatteryChanged(Battery {
                percent: Some(50),
                charging: true
            })]
        );
        assert_eq!(battery_log(&log), 1);
        // Asked again only after the interval.
        assert!(dev.poll_events().is_empty());
        assert_eq!(battery_log(&log), 0);
    }

    #[test]
    fn mice_without_a_battery_are_never_queried() {
        let (mut dev, log) = silent(0x1850);
        assert!(dev.poll_events().is_empty());
        assert_eq!(battery_log(&log), 0);
    }

    #[test]
    fn a_sleeping_mouse_keeps_its_last_reading() {
        let (mut dev, _) = silent(0x1838);
        assert!(dev.poll_events().is_empty());
        assert_eq!(dev.read_state().unwrap().battery, None);
    }

    // -- Every model ----------------------------------------------------------

    #[test]
    fn every_model_accepts_its_own_factory_settings() {
        for model in MODELS {
            let (mut dev, log) = silent(model.pid);
            let state = dev.read_state().unwrap();
            let mut settings = vec![
                Setting::Dpi(state.dpi.clone().unwrap()),
                Setting::ReportRate(1000),
            ];
            settings.extend(state.lighting.iter().map(|(zone, st)| Setting::Lighting {
                zone: *zone,
                state: *st,
            }));
            settings.extend(
                dev.capabilities()
                    .buttons
                    .iter()
                    .map(|b| button(b.id, ButtonAction::Default)),
            );
            for s in &settings {
                dev.apply(s)
                    .unwrap_or_else(|e| panic!("{} refused {s:?}: {e}", model.name));
            }
            dev.save_onboard().unwrap();
            let written = drain(&log);
            assert!(written.len() > settings.len(), "{}", model.name);
            for record in written {
                let (MockRecord::Write(b) | MockRecord::SetFeature(b)) = record else {
                    panic!("{}: unexpected read", model.name);
                };
                assert_eq!(b[0], 0, "{}: report id", model.name);
                assert!(b.len() > 1, "{}: empty command", model.name);
            }
        }
    }
}
