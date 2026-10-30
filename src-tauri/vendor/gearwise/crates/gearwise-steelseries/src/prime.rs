//! Original wired Prime1038:182e, reviewed against GG122 and rivalcfg.
//! Native static wheel RGB/enable/0..256 brightness and two independent key tables.
use crate::{
    buttons,
    link::Packet,
    models::{Btn, ButtonSpec, Field, Preset, Report},
};
use gearwise_core::model::{
    ButtonAction, DpiFunction, LightingEffect, MouseButton, Rgb, ScrollDirection, ZoneState,
};
use gearwise_core::{DeviceState, Error, Result, Setting};
use serde::{Deserialize, Serialize};
pub const PID: u16 = 0x182e;
pub const SCHEMA: &str = "steelseries-prime-lighting/1";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub enabled: bool,
    pub color: Rgb,
    pub brightness: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<crate::prime_gradient::Program>,
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            enabled: true,
            color: Rgb::new(255, 82, 0),
            brightness: 256,
            program: None,
        }
    }
}
impl Profile {
    pub fn decode(pid: u16, schema: &str, data: &serde_json::Value) -> Result<Self> {
        if pid != PID || schema != SCHEMA {
            return Err(Error::Unsupported("original wired Prime lighting schema"));
        }
        if data
            .get("color")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|o| o.keys().any(|k| !["r", "g", "b"].contains(&k.as_str())))
        {
            return Err(Error::invalid("unknown Prime RGB field"));
        }
        let p: Self = serde_json::from_value(data.clone())
            .map_err(|e| Error::invalid(format!("Prime lighting: {e}")))?;
        if p.brightness > 256 {
            return Err(Error::invalid("Prime native brightness0..256"));
        }
        if let Some(program) = &p.program {
            program.packet()?;
            let native = &data["program"];
            for color in std::iter::once(&native["initial"]).chain(
                native["sections"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|s| &s["color"]),
            ) {
                if color
                    .as_object()
                    .is_some_and(|o| o.keys().any(|k| !["r", "g", "b"].contains(&k.as_str())))
                {
                    return Err(Error::invalid("unknown Prime program RGB field"));
                }
            }
        }
        Ok(p)
    }
}
pub fn setting(p: &Profile) -> Setting {
    Setting::VendorProfile {
        schema: SCHEMA.into(),
        data: serde_json::to_value(p).expect("Prime profile"),
    }
}
pub(crate) fn plan(
    pid: u16,
    schema: &str,
    data: &serde_json::Value,
) -> Result<(Setting, Vec<Packet>)> {
    let p = Profile::decode(pid, schema, data)?;
    let config = p
        .program
        .clone()
        .unwrap_or_else(|| crate::prime_gradient::Program::steady(p.color))
        .packet()?;
    let mut bright = vec![0x5f];
    bright.extend_from_slice(&p.brightness.to_le_bytes());
    let enable = Packet::output(vec![0x6c, u8::from(p.enabled)]);
    let bright = Packet::output(bright);
    Ok((
        setting(&p),
        if p.enabled {
            vec![config, bright, enable]
        } else {
            vec![enable, config, bright]
        },
    ))
}
pub fn cache(state: &mut DeviceState, data: &serde_json::Value) -> Result<()> {
    let p = Profile::decode(PID, SCHEMA, data)?;
    state.vendor_profile = Some(serde_json::to_value(&p).expect("Prime profile"));
    state.lighting.clear();
    if p.enabled && p.program.as_ref().is_some_and(|v| v.duration > 0) {
        return Ok(());
    }
    state.lighting.insert(
        0,
        ZoneState {
            effect: if p.enabled {
                LightingEffect::Static { color: p.color }
            } else {
                LightingEffect::Off
            },
            brightness: ((u32::from(p.brightness) * 100 + 128) / 256) as u8,
        },
    );
    Ok(())
}
pub(crate) fn static_zone(data: &serde_json::Value, zone: u8, want: &ZoneState) -> Result<Setting> {
    if zone != 0 || want.brightness > 100 {
        return Err(Error::invalid("Prime wheel zone/brightness"));
    }
    let mut p = Profile::decode(PID, SCHEMA, data)?;
    match want.effect {
        LightingEffect::Off => p.enabled = false,
        LightingEffect::Static { color } => {
            p.enabled = true;
            p.program = None;
            p.color = color
        }
        _ => return Err(Error::Unsupported("Prime dynamic lighting not reviewed")),
    }
    p.brightness = ((u32::from(want.brightness) * 256 + 50) / 100) as u16;
    Ok(setting(&p))
}

fn spec(wheel: bool) -> ButtonSpec {
    ButtonSpec {
        cmd: if wheel { &[0x5c] } else { &[0x5b] },
        report: Report::Output,
        width: 5,
        keyboard: 0x51,
        consumer: 0x61,
        fields: if wheel { &WHEEL } else { &PHYSICAL },
    }
}
const PHYSICAL: [Field; 6] = [
    Field::Button(Btn {
        id: 1,
        name: "Left click",
        default: Preset::Click(MouseButton::Left),
        hotspot: (0.3, 0.2),
    }),
    Field::Button(Btn {
        id: 2,
        name: "Right click",
        default: Preset::Click(MouseButton::Right),
        hotspot: (0.7, 0.2),
    }),
    Field::Button(Btn {
        id: 3,
        name: "Middle click",
        default: Preset::Click(MouseButton::Middle),
        hotspot: (0.5, 0.17),
    }),
    Field::Button(Btn {
        id: 4,
        name: "Back",
        default: Preset::Click(MouseButton::Back),
        hotspot: (0.1, 0.5),
    }),
    Field::Button(Btn {
        id: 5,
        name: "Forward",
        default: Preset::Click(MouseButton::Forward),
        hotspot: (0.11, 0.4),
    }),
    Field::Fixed(&[0x30, 0, 0, 0, 0]),
];
const WHEEL: [Field; 2] = [
    Field::Button(Btn {
        id: 7,
        name: "Wheel up",
        default: Preset::Scroll(ScrollDirection::Up),
        hotspot: (0.5, 0.12),
    }),
    Field::Button(Btn {
        id: 8,
        name: "Wheel down",
        default: Preset::Scroll(ScrollDirection::Down),
        hotspot: (0.5, 0.23),
    }),
];
pub fn caps() -> Vec<gearwise_core::model::ButtonCaps> {
    let mut out = Vec::new();
    for f in PHYSICAL.iter().chain(WHEEL.iter()) {
        if let Field::Button(b) = f {
            out.push(gearwise_core::model::ButtonCaps {
                id: b.id,
                name: b.name.into(),
                remappable: true,
                default_action: b.default.action(),
                hotspot: Some(b.hotspot),
            });
        }
    }
    out.push(gearwise_core::model::ButtonCaps {
        id: 6,
        name: "Bottom CPI (fixed)".into(),
        remappable: false,
        default_action: ButtonAction::Dpi(DpiFunction::Cycle),
        hotspot: None,
    });
    out.sort_by_key(|b| b.id);
    out
}
pub(crate) fn button_plan(
    current: &std::collections::BTreeMap<u16, ButtonAction>,
    id: u16,
    action: &ButtonAction,
) -> Result<Packet> {
    if id == 6 {
        return Err(Error::Unsupported("Prime bottom CPI key is fixed"));
    }
    buttons::plan(&spec(id >= 7), current, id, action)
}
pub(crate) fn table(actions: &[(u16, ButtonAction)]) -> Result<Vec<Packet>> {
    use std::collections::{BTreeMap, BTreeSet};
    let ids = actions.iter().map(|(id, _)| *id).collect::<BTreeSet<_>>();
    if ids != [2, 3, 4, 5, 7, 8].into_iter().collect() || ids.len() != actions.len() {
        return Err(Error::invalid(
            "complete Prime physical and wheel mappings required",
        ));
    }
    let mut map = BTreeMap::new();
    for (id, a) in actions {
        button_plan(&map, *id, a)?;
        map.insert(*id, a.clone());
    }
    Ok(vec![
        button_plan(&map, 2, map.get(&2).unwrap())?,
        button_plan(&map, 7, map.get(&7).unwrap())?,
    ])
}
