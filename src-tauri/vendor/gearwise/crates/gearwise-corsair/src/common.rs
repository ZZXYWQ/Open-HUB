//! Pieces shared by both protocol families: descriptors, validation and
//! software-lit colour handling.

use std::collections::BTreeMap;

use gearwise_core::device::{DeviceDescriptor, DeviceState};
use gearwise_core::error::{Error, Result};
use gearwise_core::model::{
    Connection, DpiCaps, DpiState, EffectKind, LightingEffect, LightingZoneCaps, Rgb, VID_CORSAIR,
    Vendor, ZoneState,
};
use gearwise_core::transport::HidInterfaceInfo;

use crate::models::{DPI_STAGES, DPI_STEP, DpiSpec, Model, Zone};

/// Polling rates by Bragi property value (1..=7); NXP uses 1000 / interval.
pub(crate) const RATES_HZ: [u16; 7] = [125, 250, 500, 1000, 2000, 4000, 8000];

/// Build the descriptor for a device found behind `iface`. `index` is the
/// receiver slot for wireless devices.
pub(crate) fn describe(
    model: &Model,
    iface: &HidInterfaceInfo,
    index: Option<u8>,
    connection: Connection,
    firmware: Option<String>,
) -> DeviceDescriptor {
    let serial = iface.serial.clone().filter(|s| !s.is_empty());
    let place = serial.clone().unwrap_or_else(|| iface.path.clone());
    let mut id = format!("corsair:{:04x}:{place}", model.pid);
    if let Some(i) = index {
        id.push_str(&format!(":{i}"));
    }
    DeviceDescriptor {
        id,
        model_key: DeviceDescriptor::model_key_for(Vendor::Corsair, model.pid),
        vendor: Vendor::Corsair,
        kind: model.kind,
        name: model.name.to_string(),
        vendor_id: VID_CORSAIR,
        product_id: model.pid,
        connection,
        firmware,
        serial,
        simulated: false,
    }
}

pub(crate) fn dpi_caps(spec: &DpiSpec) -> DpiCaps {
    DpiCaps {
        min: spec.min,
        max: spec.max,
        step: DPI_STEP,
        max_stages: DPI_STAGES,
        separate_xy: spec.separate_xy,
    }
}

/// Check a DPI table against the sensor limits before anything is sent.
pub(crate) fn check_dpi(spec: &DpiSpec, dpi: &DpiState) -> Result<()> {
    if dpi.stages.is_empty() || dpi.stages.len() > DPI_STAGES as usize {
        return Err(Error::invalid(format!(
            "{} DPI stages (1 to {DPI_STAGES} allowed)",
            dpi.stages.len()
        )));
    }
    if dpi.active as usize >= dpi.stages.len() {
        return Err(Error::invalid("active stage out of range"));
    }
    for st in &dpi.stages {
        for v in [st.x, st.y] {
            if v < spec.min || v > spec.max {
                return Err(Error::invalid(format!(
                    "{v} DPI outside {}-{}",
                    spec.min, spec.max
                )));
            }
        }
        if !spec.separate_xy && st.x != st.y {
            return Err(Error::invalid(
                "this sensor has a single DPI value for both axes",
            ));
        }
    }
    Ok(())
}

/// Lighting zone capabilities: Off and Static, brightness applied by the host.
pub(crate) fn zone_caps(id: u8, name: &str) -> LightingZoneCaps {
    LightingZoneCaps {
        keys: Vec::new(),
        id,
        name: name.to_string(),
        effects: vec![EffectKind::Off, EffectKind::Static],
        brightness: true,
    }
}

pub(crate) fn zones_caps(zones: &[Zone]) -> Vec<LightingZoneCaps> {
    zones.iter().map(|z| zone_caps(z.id, z.name)).collect()
}

/// Reject effects other than Off/Static and out-of-range brightness.
pub(crate) fn check_zone_state(state: &ZoneState) -> Result<()> {
    if !matches!(
        state.effect,
        LightingEffect::Off | LightingEffect::Static { .. }
    ) {
        return Err(Error::Unsupported(
            "lighting effect (only Off and Static are implemented)",
        ));
    }
    if state.brightness > 100 {
        return Err(Error::invalid(format!(
            "brightness {} above 100",
            state.brightness
        )));
    }
    Ok(())
}

/// Colour a zone is currently showing, brightness applied. Zones the host has
/// not touched yet are white (or off for indicators).
pub(crate) fn zone_color(lighting: &BTreeMap<u8, ZoneState>, zone: u8, default_on: bool) -> Rgb {
    match lighting.get(&zone) {
        Some(ZoneState {
            effect: LightingEffect::Static { color },
            brightness,
        }) => color.scaled(*brightness),
        Some(_) => Rgb::BLACK,
        None if default_on => Rgb::WHITE,
        None => Rgb::BLACK,
    }
}

/// Colour of one key on a per-key keyboard: its own colour if set, else the
/// base colour of the keys zone. Black when the keys zone is off.
pub(crate) fn key_color(state: &DeviceState, zone: u8, key: u16) -> Rgb {
    let brightness = match state.lighting.get(&zone) {
        Some(z) if matches!(z.effect, LightingEffect::Off) => return Rgb::BLACK,
        Some(z) => z.brightness,
        None => 100,
    };
    match state.per_key.get(&key) {
        Some(c) => c.scaled(brightness),
        None => zone_color(&state.lighting, zone, true),
    }
}

/// Which firmware errors mean "this optional feature is absent" rather than
/// "the device is broken".
pub(crate) fn optional<T>(r: Result<T>) -> Result<Option<T>> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(Error::Unsupported(_) | Error::Protocol(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gearwise_core::model::DpiStage;

    fn spec(separate: bool) -> DpiSpec {
        DpiSpec {
            min: 100,
            max: 18_000,
            separate_xy: separate,
        }
    }

    #[test]
    fn dpi_validation() {
        let ok = DpiState {
            stages: vec![DpiStage::uniform(800), DpiStage { x: 1600, y: 1200 }],
            active: 1,
        };
        assert!(check_dpi(&spec(true), &ok).is_ok());
        assert!(matches!(
            check_dpi(&spec(false), &ok),
            Err(Error::InvalidValue(_))
        ));
        let too_high = DpiState {
            stages: vec![DpiStage::uniform(20_000)],
            active: 0,
        };
        assert!(check_dpi(&spec(true), &too_high).is_err());
        let bad_active = DpiState {
            stages: vec![DpiStage::uniform(800)],
            active: 1,
        };
        assert!(check_dpi(&spec(true), &bad_active).is_err());
        let too_many = DpiState {
            stages: vec![DpiStage::uniform(800); 6],
            active: 0,
        };
        assert!(check_dpi(&spec(true), &too_many).is_err());
    }

    #[test]
    fn zone_colors_honour_brightness_and_defaults() {
        let mut l = BTreeMap::new();
        assert_eq!(zone_color(&l, 0, true), Rgb::WHITE);
        assert_eq!(zone_color(&l, 0, false), Rgb::BLACK);
        l.insert(
            0,
            ZoneState {
                effect: LightingEffect::Static {
                    color: Rgb::new(200, 100, 0),
                },
                brightness: 50,
            },
        );
        assert_eq!(zone_color(&l, 0, true), Rgb::new(100, 50, 0));
        l.insert(
            0,
            ZoneState {
                effect: LightingEffect::Off,
                brightness: 100,
            },
        );
        assert_eq!(zone_color(&l, 0, true), Rgb::BLACK);
    }

    #[test]
    fn unsupported_effects_are_rejected() {
        let breathing = ZoneState {
            effect: LightingEffect::Breathing {
                color: Rgb::WHITE,
                period_ms: 1000,
            },
            brightness: 100,
        };
        assert!(matches!(
            check_zone_state(&breathing),
            Err(Error::Unsupported(_))
        ));
        let loud = ZoneState {
            effect: LightingEffect::Off,
            brightness: 101,
        };
        assert!(matches!(
            check_zone_state(&loud),
            Err(Error::InvalidValue(_))
        ));
    }

    #[test]
    fn key_colors_fall_back_to_the_keys_zone() {
        let mut s = DeviceState::default();
        s.per_key.insert(4, Rgb::new(255, 0, 0));
        assert_eq!(key_color(&s, 0, 4), Rgb::new(255, 0, 0));
        assert_eq!(key_color(&s, 0, 5), Rgb::WHITE);
        s.lighting.insert(
            0,
            ZoneState {
                effect: LightingEffect::Static {
                    color: Rgb::new(0, 0, 200),
                },
                brightness: 50,
            },
        );
        assert_eq!(key_color(&s, 0, 4), Rgb::new(127, 0, 0));
        assert_eq!(key_color(&s, 0, 5), Rgb::new(0, 0, 100));
        s.lighting.insert(
            0,
            ZoneState {
                effect: LightingEffect::Off,
                brightness: 100,
            },
        );
        assert_eq!(key_color(&s, 0, 4), Rgb::BLACK);
    }
}
