//! HID++ 8070 zone effects, dynamically discovered. No universal model claim.
//! Output is volatile only; firmware effects/parameters not implemented stay
//! readable/opaque. 8071 software-ownership writes are deliberately separate.
use crate::{
    device::Device,
    hidpp::{Error, FeatureRequest, Result},
};
use serde::{Deserialize, Serialize};
pub const COLOR: u16 = 0x8070;
pub const RGB: u16 = 0x8071;
fn fail(message: &str) -> Error {
    Error::Other(format!("Logitech lighting: {message}"))
}
fn len<'a>(r: &'a [u8], need: usize) -> Result<&'a [u8]> {
    if r.len() < need {
        Err(fail("short feature response"))
    } else {
        Ok(r)
    }
}
fn u16be(r: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([r[at], r[at + 1]])
}
fn mouse(d: &Device) -> Result<()> {
    if d.vendor_id != 0x046d || !d.kind.eq_ignore_ascii_case("mouse") {
        return Err(fail("requires a Logitech mouse"));
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Effect {
    Off,
    Fixed { color: [u8; 3] },
}
impl Effect {
    fn id(&self) -> u16 {
        match self {
            Self::Off => 0,
            Self::Fixed { .. } => 1,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Slot {
    pub index: u8,
    pub effect_id: u16,
    pub capabilities: u16,
    pub period: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Zone {
    pub index: u8,
    pub location: u16,
    pub persistency: u8,
    pub effects: Vec<Slot>,
    /// Actual slot + ten parameter bytes. Used only as a compare-before-write
    /// baseline, never as an arbitrary caller-supplied output report.
    pub current: Option<[u8; 11]>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct View {
    pub feature: u16,
    pub version: u8,
    pub nv_caps: u16,
    pub ext_caps: u16,
    pub writable: bool,
    pub zones: Vec<Zone>,
}
fn raw(d: &Device, t: &dyn FeatureRequest, zone: u8) -> Result<[u8; 11]> {
    let reply = d.call(t, COLOR, 14, &[zone])?;
    let r = len(&reply, 12)?;
    if r[0] != zone {
        return Err(fail("current effect belongs to another zone"));
    }
    Ok(r[1..12].try_into().unwrap())
}
pub fn read(d: &Device, t: &dyn FeatureRequest) -> Result<View> {
    mouse(d)?;
    if !d.has(COLOR) {
        return Err(fail(if d.has(RGB) {
            "this mouse exposes 8071; its software-control transaction is not enabled yet"
        } else {
            "this mouse does not expose 8070 zone lighting"
        }));
    }
    let version = d.features[&COLOR].version;
    let reply = d.call(t, COLOR, 0, &[])?;
    let info = len(&reply, 5)?;
    if info[0] > 16 {
        return Err(fail("unverified zone count"));
    }
    let nv_caps = u16be(info, 1);
    let ext_caps = u16be(info, 3);
    let can_read = ext_caps & 1 != 0 && ext_caps & 2 == 0;
    let mut zones = Vec::new();
    let mut total = 0usize;
    for index in 0..info[0] {
        let reply = d.call(t, COLOR, 1, &[index])?;
        let r = len(&reply, 5)?;
        if r[0] != index || r[3] > 32 {
            return Err(fail("zone descriptor echo/count mismatch"));
        }
        total += usize::from(r[3]);
        if total > 128 {
            return Err(fail("effect descriptor limit exceeded"));
        }
        let mut zone = Zone {
            index,
            location: u16be(r, 1),
            persistency: r[4],
            effects: Vec::new(),
            current: None,
        };
        for slot in 0..r[3] {
            let reply = d.call(t, COLOR, 2, &[index, slot])?;
            let e = len(&reply, 8)?;
            if e[0] != index || e[1] != slot {
                return Err(fail("effect descriptor echo mismatch"));
            }
            zone.effects.push(Slot {
                index: slot,
                effect_id: u16be(e, 2),
                capabilities: u16be(e, 4),
                period: u16be(e, 6),
            });
        }
        if can_read {
            zone.current = Some(raw(d, t, index)?);
        }
        zones.push(zone);
    }
    Ok(View {
        feature: COLOR,
        version,
        nv_caps,
        ext_caps,
        writable: d.index == crate::hidpp::DIRECT && version <= 4 && can_read,
        zones,
    })
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub zone: u8,
    pub expected: [u8; 11],
    pub effect: Effect,
}
#[derive(Debug, Serialize)]
pub struct Applied {
    pub view: View,
    pub unchanged: bool,
}
fn compile(view: &View, change: &Change) -> Result<[u8; 13]> {
    if !view.writable {
        return Err(fail(
            "writes require direct USB and a verified readable 8070 version",
        ));
    }
    let zone = view
        .zones
        .iter()
        .find(|z| z.index == change.zone)
        .ok_or_else(|| fail("unknown zone"))?;
    if zone.current != Some(change.expected) {
        return Err(fail("zone changed since preview; read it again"));
    }
    if !zone
        .effects
        .iter()
        .any(|slot| slot.index == change.expected[0])
    {
        return Err(fail("unknown current slot cannot be safely restored"));
    }
    let mut slots = zone
        .effects
        .iter()
        .filter(|slot| slot.effect_id == change.effect.id());
    let slot = slots
        .next()
        .ok_or_else(|| fail("effect not advertised by this zone"))?;
    if slots.next().is_some() {
        return Err(fail("ambiguous effect descriptor"));
    }
    let mut out = [0; 13];
    out[0] = zone.index;
    out[1] = slot.index;
    if let Effect::Fixed { color } = change.effect {
        out[2..5].copy_from_slice(&color);
        out[5] = 2;
    }
    // out[12] remains zero: RAM only. Never write flash while synchronizing.
    Ok(out)
}
#[derive(Debug, PartialEq, Eq)]
struct Owner {
    mode: u8,
    profile: Option<Vec<u8>>,
}
fn owner(d: &Device, t: &dyn FeatureRequest) -> Result<Option<Owner>> {
    if !d.has(crate::device::feature::ONBOARD_PROFILES) {
        return Ok(None);
    }
    let reply = d.call(t, 0x8100, 2, &[])?;
    let mode = *len(&reply, 1)?.first().unwrap();
    if !matches!(mode, 1 | 2) {
        return Err(fail("unknown onboard ownership mode"));
    }
    let profile = if mode == 1 {
        let r = d.call(t, 0x8100, 4, &[])?;
        Some(len(&r, 2)?[..2].to_vec())
    } else {
        None
    };
    Ok(Some(Owner { mode, profile }))
}
fn same_caps(a: &View, b: &View) -> bool {
    a.feature == b.feature
        && a.version == b.version
        && a.nv_caps == b.nv_caps
        && a.ext_caps == b.ext_caps
        && a.zones.len() == b.zones.len()
        && a.zones.iter().zip(&b.zones).all(|(a, b)| {
            a.index == b.index
                && a.location == b.location
                && a.persistency == b.persistency
                && a.effects == b.effects
        })
}
pub fn apply(d: &Device, t: &dyn FeatureRequest, change: &Change) -> Result<Applied> {
    mouse(d)?;
    if d.index != crate::hidpp::DIRECT {
        return Err(fail(
            "receiver writes require verified physical unit pinning; use direct USB",
        ));
    }
    let before = read(d, t)?;
    let packet = compile(&before, change)?;
    let expected: [u8; 11] = packet[1..12].try_into().unwrap();
    if change.expected == expected {
        return Ok(Applied {
            view: before,
            unchanged: true,
        });
    }
    let ownership = owner(d, t)?;
    let write = (|| -> Result<()> {
        d.call(t, COLOR, 3, &packet)?;
        if raw(d, t, change.zone)? != expected {
            return Err(fail("effect readback differs from requested value"));
        }
        if owner(d, t)? != ownership {
            return Err(fail("profile ownership changed during lighting write"));
        }
        Ok(())
    })();
    if let Err(error) = write {
        let restore = (|| -> Result<()> {
            if owner(d, t)? != ownership {
                return Err(fail(
                    "ownership changed; original effect was not sent to a different profile",
                ));
            }
            let actual = read(d, t)?;
            if !same_caps(&before, &actual) {
                return Err(fail(
                    "lighting capabilities changed; restoration not attempted",
                ));
            }
            let current = actual
                .zones
                .iter()
                .find(|z| z.index == change.zone)
                .and_then(|z| z.current)
                .ok_or_else(|| fail("no readable restoration baseline"))?;
            if current == change.expected {
                return Ok(());
            }
            if current != expected {
                return Err(fail("zone changed to a third value; no blind restoration"));
            }
            let mut rollback = [0; 13];
            rollback[0] = change.zone;
            rollback[1..12].copy_from_slice(&change.expected);
            if owner(d, t)? != ownership {
                return Err(fail("ownership changed before restoration"));
            }
            d.call(t, COLOR, 3, &rollback)?;
            if raw(d, t, change.zone)? != change.expected {
                return Err(fail("original effect could not be verified"));
            }
            if owner(d, t)? != ownership {
                return Err(fail("ownership changed during restoration"));
            }
            Ok(())
        })();
        return Err(fail(&format!(
            "{error}; {}",
            match restore {
                Ok(()) => "original zone restored/verified".to_string(),
                Err(e) => format!("state uncertain: {e}"),
            }
        )));
    }
    let mut view = before;
    view.zones
        .iter_mut()
        .find(|z| z.index == change.zone)
        .unwrap()
        .current = Some(expected);
    Ok(Applied {
        view,
        unchanged: false,
    })
}
#[cfg(test)]
#[path = "native_logitech_lighting_tests.rs"]
mod tests;
