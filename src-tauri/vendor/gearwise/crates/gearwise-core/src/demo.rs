//! Simulated devices: let the app be explored (and the UI developed) without
//! hardware. They behave like real drivers that keep state in memory.

use std::collections::BTreeMap;

use crate::device::*;
use crate::error::{Error, Result};
use crate::keys;
use crate::model::*;

pub struct DemoDevice {
    desc: DeviceDescriptor,
    caps: Capabilities,
    state: DeviceState,
    /// Battery drains a little per poll so the indicator visibly lives.
    ticks: u32,
}

impl DemoDevice {
    fn validate(&self, s: &Setting) -> Result<()> {
        let c = &self.caps;
        match s {
            Setting::Dpi(d) => {
                let caps = c.dpi.as_ref().ok_or(Error::Unsupported("dpi"))?;
                if d.stages.is_empty() || d.stages.len() > caps.max_stages as usize {
                    return Err(Error::invalid("stage count"));
                }
                if d.active as usize >= d.stages.len() {
                    return Err(Error::invalid("active stage out of range"));
                }
                for st in &d.stages {
                    for v in [st.x, st.y] {
                        if v < caps.min || v > caps.max {
                            return Err(Error::invalid(format!("{v} DPI out of range")));
                        }
                    }
                }
            }
            Setting::DpiActiveStage(i) => {
                let n = self.state.dpi.as_ref().map_or(0, |d| d.stages.len());
                if *i as usize >= n {
                    return Err(Error::invalid("active stage out of range"));
                }
            }
            Setting::ReportRate(r) if !c.report_rates.contains(r) => {
                return Err(Error::invalid(format!("{r} Hz")));
            }
            Setting::Lighting { zone, state } => {
                let z = c
                    .lighting
                    .iter()
                    .find(|z| z.id == *zone)
                    .ok_or(Error::invalid("zone"))?;
                if !z.effects.contains(&state.effect.kind()) {
                    return Err(Error::Unsupported("effect"));
                }
            }
            Setting::PerKey(_) if c.per_key.is_none() => return Err(Error::Unsupported("per-key")),
            Setting::Button { id, .. }
                if !c.buttons.iter().any(|b| b.id == *id && b.remappable) =>
            {
                return Err(Error::invalid("button"));
            }
            Setting::OnboardProfile(p) if *p >= c.onboard_profiles => {
                return Err(Error::invalid("profile slot"));
            }
            Setting::LiftOff(l) if *l as usize >= c.lift_off.len() => {
                return Err(Error::invalid("lift-off"));
            }
            Setting::AngleSnapping(_) if !c.angle_snapping => {
                return Err(Error::Unsupported("angle snapping"));
            }
            Setting::GameMode(_) if !c.game_mode => return Err(Error::Unsupported("game mode")),
            _ => {}
        }
        Ok(())
    }
}

impl Device for DemoDevice {
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
        setting.apply_to(&mut self.state);
        Ok(())
    }
    fn poll_events(&mut self) -> Vec<DeviceEvent> {
        self.ticks += 1;
        let Some(b) = self.state.battery.as_mut() else {
            return vec![];
        };
        // ~1% every 600 polls; recharge from 15%.
        if self.ticks % 600 == 0 {
            let p = b.percent.unwrap_or(100);
            b.percent = Some(if p <= 15 { 100 } else { p - 1 });
            return vec![DeviceEvent::BatteryChanged(*b)];
        }
        vec![]
    }
}

fn desc(
    vendor: Vendor,
    kind: DeviceKind,
    name: &str,
    pid: u16,
    conn: Connection,
    fw: &str,
) -> DeviceDescriptor {
    DeviceDescriptor {
        id: format!("demo:{}:{:04x}", vendor.key(), pid),
        model_key: DeviceDescriptor::model_key_for(vendor, pid),
        vendor,
        kind,
        name: name.into(),
        vendor_id: match vendor {
            Vendor::Logitech => VID_LOGITECH,
            Vendor::Razer => VID_RAZER,
            Vendor::Corsair => VID_CORSAIR,
            _ => 0,
        },
        product_id: pid,
        connection: conn,
        firmware: Some(fw.into()),
        serial: None,
        simulated: true,
    }
}

fn button(
    id: u16,
    name: &str,
    default_action: ButtonAction,
    hotspot: Option<(f32, f32)>,
) -> ButtonCaps {
    ButtonCaps {
        id,
        name: name.into(),
        remappable: true,
        default_action,
        hotspot,
    }
}

fn zone(id: u8, name: &str, effects: &[EffectKind]) -> LightingZoneCaps {
    LightingZoneCaps {
        keys: Vec::new(),
        id,
        name: name.into(),
        effects: effects.to_vec(),
        brightness: true,
    }
}

fn stages(v: &[u16]) -> Vec<DpiStage> {
    v.iter().map(|d| DpiStage::uniform(*d)).collect()
}

fn logitech_mouse() -> DemoDevice {
    use EffectKind::*;
    let caps = Capabilities {
        layout: None,
        dpi: Some(DpiCaps {
            min: 100,
            max: 25_600,
            step: 50,
            max_stages: 5,
            separate_xy: true,
        }),
        report_rates: vec![125, 250, 500, 1000],
        lighting: vec![
            zone(0, "Logo", &[Off, Static, Breathing, ColorCycle]),
            zone(1, "Primary", &[Off, Static, Breathing, ColorCycle, Wave]),
        ],
        per_key: None,
        buttons: vec![
            button(
                0x50,
                "Left click",
                ButtonAction::Mouse(MouseButton::Left),
                Some((0.30, 0.20)),
            ),
            button(
                0x51,
                "Right click",
                ButtonAction::Mouse(MouseButton::Right),
                Some((0.70, 0.20)),
            ),
            button(
                0x52,
                "Middle click",
                ButtonAction::Mouse(MouseButton::Middle),
                Some((0.50, 0.17)),
            ),
            button(
                0x5b,
                "Scroll left",
                ButtonAction::Mouse(MouseButton::WheelLeft),
                Some((0.43, 0.12)),
            ),
            button(
                0x5d,
                "Scroll right",
                ButtonAction::Mouse(MouseButton::WheelRight),
                Some((0.57, 0.12)),
            ),
            button(
                0x53,
                "G4 · Back",
                ButtonAction::Mouse(MouseButton::Back),
                Some((0.10, 0.50)),
            ),
            button(
                0x56,
                "G5 · Forward",
                ButtonAction::Mouse(MouseButton::Forward),
                Some((0.11, 0.40)),
            ),
            button(
                0xfd,
                "G6 · DPI shift",
                ButtonAction::Dpi(DpiFunction::Shift(400)),
                Some((0.12, 0.30)),
            ),
            button(
                0xa0,
                "G7 · DPI down",
                ButtonAction::Dpi(DpiFunction::Down),
                Some((0.20, 0.10)),
            ),
            button(
                0xa1,
                "G8 · DPI up",
                ButtonAction::Dpi(DpiFunction::Up),
                Some((0.20, 0.20)),
            ),
            button(
                0xa2,
                "G9 · Profile",
                ButtonAction::ProfileCycle,
                Some((0.50, 0.33)),
            ),
        ],
        onboard_profiles: 5,
        battery: true,
        lift_off: vec![],
        angle_snapping: false,
        game_mode: false,
    };
    let mut lighting = BTreeMap::new();
    lighting.insert(
        0,
        ZoneState {
            effect: LightingEffect::Static {
                color: Rgb::new(0, 212, 170),
            },
            brightness: 80,
        },
    );
    lighting.insert(
        1,
        ZoneState {
            effect: LightingEffect::ColorCycle { period_ms: 10_000 },
            brightness: 60,
        },
    );
    let state = DeviceState {
        dpi: Some(DpiState {
            stages: stages(&[400, 800, 1600, 3200, 6400]),
            active: 2,
        }),
        report_rate: Some(1000),
        lighting,
        onboard_profile: Some(0),
        battery: Some(Battery {
            percent: Some(78),
            charging: false,
        }),
        ..Default::default()
    };
    DemoDevice {
        desc: desc(
            Vendor::Logitech,
            DeviceKind::Mouse,
            "G502 X Plus (demo)",
            0xc095,
            Connection::Wireless,
            "U1 49.1.6",
        ),
        caps,
        state,
        ticks: 0,
    }
}

fn razer_keyboard() -> DemoDevice {
    use EffectKind::*;
    let layout = keys::ansi_full();
    let width = layout.width();
    let per_key: BTreeMap<u16, Rgb> = layout
        .keys
        .iter()
        .map(|k| (k.id, Rgb::from_hue(280.0 * (k.x + k.w / 2.0) / width)))
        .collect();
    let mut buttons: Vec<ButtonCaps> = (1..=5)
        .map(|i| ButtonCaps {
            id: 0x100 + i,
            name: format!("M{i}"),
            remappable: true,
            default_action: ButtonAction::Disabled,
            hotspot: None,
        })
        .collect();
    buttons.push(ButtonCaps {
        id: 0x39,
        name: "Caps Lock".into(),
        remappable: true,
        default_action: ButtonAction::Key {
            usage: 0x39,
            mods: Modifiers(0),
        },
        hotspot: None,
    });
    let caps = Capabilities {
        layout: None,
        dpi: None,
        report_rates: vec![125, 250, 500, 1000, 2000, 4000, 8000],
        lighting: vec![
            zone(
                0,
                "Keys",
                &[Off, Static, Breathing, ColorCycle, Wave, Reactive],
            ),
            zone(1, "Underglow", &[Off, Static, Breathing, ColorCycle, Wave]),
        ],
        per_key: Some(layout),
        buttons,
        onboard_profiles: 0,
        battery: false,
        lift_off: vec![],
        angle_snapping: false,
        game_mode: true,
    };
    let mut lighting = BTreeMap::new();
    lighting.insert(
        0,
        ZoneState {
            effect: LightingEffect::Wave {
                period_ms: 3000,
                direction: WaveDirection::LeftToRight,
            },
            brightness: 90,
        },
    );
    lighting.insert(
        1,
        ZoneState {
            effect: LightingEffect::Static {
                color: Rgb::new(140, 60, 255),
            },
            brightness: 70,
        },
    );
    let state = DeviceState {
        report_rate: Some(1000),
        lighting,
        per_key,
        game_mode: Some(false),
        ..Default::default()
    };
    DemoDevice {
        desc: desc(
            Vendor::Razer,
            DeviceKind::Keyboard,
            "BlackWidow V4 Pro (demo)",
            0x028d,
            Connection::Wired,
            "1.02",
        ),
        caps,
        state,
        ticks: 0,
    }
}

fn corsair_mouse() -> DemoDevice {
    use EffectKind::*;
    let caps = Capabilities {
        layout: None,
        dpi: Some(DpiCaps {
            min: 100,
            max: 26_000,
            step: 10,
            max_stages: 5,
            separate_xy: true,
        }),
        report_rates: vec![125, 250, 500, 1000, 2000, 4000, 8000],
        lighting: vec![
            zone(0, "Logo", &[Off, Static, Breathing, ColorCycle]),
            zone(1, "Scroll wheel", &[Off, Static, Breathing, ColorCycle]),
        ],
        per_key: None,
        buttons: vec![
            button(
                1,
                "Left click",
                ButtonAction::Mouse(MouseButton::Left),
                Some((0.30, 0.20)),
            ),
            button(
                2,
                "Right click",
                ButtonAction::Mouse(MouseButton::Right),
                Some((0.70, 0.20)),
            ),
            button(
                3,
                "Middle click",
                ButtonAction::Mouse(MouseButton::Middle),
                Some((0.50, 0.17)),
            ),
            button(
                4,
                "Back",
                ButtonAction::Mouse(MouseButton::Back),
                Some((0.10, 0.48)),
            ),
            button(
                5,
                "Forward",
                ButtonAction::Mouse(MouseButton::Forward),
                Some((0.11, 0.38)),
            ),
            button(
                6,
                "Sniper",
                ButtonAction::Dpi(DpiFunction::Shift(400)),
                Some((0.08, 0.28)),
            ),
            button(
                7,
                "DPI cycle",
                ButtonAction::Dpi(DpiFunction::Cycle),
                Some((0.50, 0.32)),
            ),
        ],
        onboard_profiles: 3,
        battery: false,
        lift_off: vec!["Low".into(), "Medium".into(), "High".into()],
        angle_snapping: true,
        game_mode: false,
    };
    let mut lighting = BTreeMap::new();
    lighting.insert(
        0,
        ZoneState {
            effect: LightingEffect::Static {
                color: Rgb::new(255, 200, 0),
            },
            brightness: 100,
        },
    );
    lighting.insert(
        1,
        ZoneState {
            effect: LightingEffect::Breathing {
                color: Rgb::new(255, 200, 0),
                period_ms: 4000,
            },
            brightness: 100,
        },
    );
    let state = DeviceState {
        dpi: Some(DpiState {
            stages: stages(&[800, 1500, 3000]),
            active: 0,
        }),
        report_rate: Some(2000),
        lighting,
        onboard_profile: Some(0),
        lift_off: Some(0),
        angle_snapping: Some(false),
        ..Default::default()
    };
    DemoDevice {
        desc: desc(
            Vendor::Corsair,
            DeviceKind::Mouse,
            "M65 RGB Ultra (demo)",
            0x1b9e,
            Connection::Wired,
            "5.12.48",
        ),
        caps,
        state,
        ticks: 0,
    }
}

/// One simulated device per supported brand.
pub fn devices() -> Vec<Box<dyn Device>> {
    vec![
        Box::new(logitech_mouse()),
        Box::new(razer_keyboard()),
        Box::new(corsair_mouse()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_devices_accept_their_own_state() {
        for mut d in devices() {
            let caps = d.capabilities().clone();
            let state = d.read_state().unwrap();
            for s in crate::profile::settings_from_state(&caps, &state) {
                d.apply(&s)
                    .unwrap_or_else(|e| panic!("{}: {:?}: {e}", d.descriptor().name, s));
            }
            assert_eq!(d.read_state().unwrap(), state);
        }
    }

    #[test]
    fn demo_rejects_out_of_range() {
        let mut d = logitech_mouse();
        assert!(d.apply(&Setting::ReportRate(8000)).is_err());
        assert!(
            d.apply(&Setting::Dpi(DpiState {
                stages: stages(&[50]),
                active: 0
            }))
            .is_err()
        );
        assert!(d.apply(&Setting::GameMode(true)).is_err());
        d.apply(&Setting::DpiActiveStage(4)).unwrap();
        assert_eq!(d.read_state().unwrap().dpi.unwrap().active, 4);
    }
}
