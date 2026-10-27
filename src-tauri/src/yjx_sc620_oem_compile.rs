//! OEM SQL event -> static native library conversion. Never applies to hardware.
use super::{
    bindings::{MacroBinding, Playback},
    invalid,
    macros::{Action, Image, Record},
    oem_library::Macro,
};
use gearwise_core::Result;
fn key(v: i64) -> Result<u8> {
    Ok(match v {
        0x41..=0x5a => (v - 0x3d) as u8,
        0x31..=0x39 => (v - 0x13) as u8,
        0x70..=0x7b => (v - 0x36) as u8,
        0x7c..=0x87 => (v - 0x14) as u8,
        0x61..=0x69 => (v - 8) as u8,
        0x30 => 0x27,
        0xd => 0x28,
        0x1b => 0x29,
        0x8 => 0x2a,
        0x9 => 0x2b,
        0x20 => 0x2c,
        0xbd => 0x2d,
        0xbb => 0x2e,
        0xdb => 0x2f,
        0xdd => 0x30,
        0xdc => 0x31,
        0x29a => 0x32,
        0xba => 0x33,
        0xde => 0x34,
        0xc0 => 0x35,
        0xbc => 0x36,
        0xbe => 0x37,
        0xbf => 0x38,
        0x14 => 0x39,
        0x2c => 0x46,
        0x91 => 0x47,
        0x13 => 0x48,
        0x2d => 0x49,
        0x24 => 0x4a,
        0x21 => 0x4b,
        0x2e => 0x4c,
        0x23 => 0x4d,
        0x22 => 0x4e,
        0x27 => 0x4f,
        0x25 => 0x50,
        0x28 => 0x51,
        0x26 => 0x52,
        0x90 => 0x53,
        0x6f => 0x54,
        0x6a => 0x55,
        0x6d => 0x56,
        0x6b => 0x57,
        0x6c => 0x58,
        0x60 => 0x62,
        0x6e => 0x63,
        0x5d => 0x65,
        _ => return Err(invalid("unverified SC620 OEM key/modifier code")),
    })
}
fn event(kind: i64, value: i64, description: &str) -> Result<Action> {
    Ok(match kind {
        2 => Action::KeyDown(key(value)?),
        3 => Action::KeyUp(key(value)?),
        4 | 5 if description.contains("Scroll Up") => Action::Wheel(1),
        4 | 5 if description.contains("Scroll Down") => Action::Wheel(-1),
        4 | 5 if matches!(value, 1 | 2 | 4 | 8 | 16) => {
            if kind == 4 {
                Action::MouseDown(value as u8)
            } else {
                Action::MouseUp(value as u8)
            }
        }
        6 => Action::Wheel(1),
        7 => Action::Wheel(-1),
        _ => return Err(invalid("unverified SC620 OEM macro event")),
    })
}
pub fn records(m: &Macro) -> Result<Vec<Record>> {
    if m.events.len() > 4096 {
        return Err(invalid("OEM event count"));
    }
    if Playback::from_oem(m.play_type, m.play_times)? != m.playback {
        return Err(invalid("OEM playback fields disagree"));
    }
    let mut records = Vec::<Record>::new();
    let mut pending = 0u32;
    let mut trailing = false;
    let mut previous = None;
    for e in &m.events {
        if e.index < 0 || previous.is_some_and(|i| i >= e.index) {
            return Err(invalid("OEM event order"));
        }
        previous = Some(e.index);
        if e.kind == 1 {
            if records.is_empty() {
                return Err(invalid(
                    "leading OEM delay cannot be preserved by the verified native format",
                ));
            }
            if !(0..=65535).contains(&e.value) {
                return Err(invalid("OEM delay outside native16-bit field"));
            }
            pending = pending
                .checked_add(e.value as u32)
                .filter(|n| *n <= 65535)
                .ok_or_else(|| invalid("accumulated OEM delay would truncate"))?;
            trailing = true;
        } else {
            let action = event(e.kind, e.value, &e.description)
                .map_err(|err| invalid(&format!("macro {} event {}: {err}", m.id, e.index)))?;
            if let Some(last) = records.last_mut() {
                last.delay_raw = if pending == 0 { 10 } else { pending as u16 };
            }
            records.push(Record {
                delay_raw: 0,
                action,
            });
            pending = 0;
            trailing = false;
        }
    }
    if trailing {
        records.last_mut().unwrap().delay_raw = pending as u16;
    }
    Ok(records)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    pub oem_id: i64,
    pub binding: MacroBinding,
}
pub struct Prepared {
    pub image: Image,
    pub slots: Vec<Slot>,
}
/// New whole-library image with explicit ID->slot mapping. Existing library
/// preservation still requires library_plan's six-page reference validation.
pub fn prepare(macros: &[Macro]) -> Result<Prepared> {
    if macros.len() > 32 {
        return Err(invalid("more than32 OEM macros"));
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut bodies = Vec::new();
    let mut slots = Vec::new();
    for (i, m) in macros.iter().enumerate() {
        if m.id <= 0 || !ids.insert(m.id) {
            return Err(invalid("duplicate or invalid OEM macro ID"));
        }
        bodies.push(records(m)?);
        slots.push(Slot {
            oem_id: m.id,
            binding: MacroBinding {
                slot: i as u8,
                playback: m.playback.wire()?,
            },
        });
    }
    let mut bytes = Image::compile(&bodies)?.bytes().to_vec();
    // OEM cursor is global, but its delay accumulator starts at1 for every
    // macro. First action of a later nonempty macro ORs1 into the preceding
    // record's delay, including an empty sentinel. Preserve that exact output.
    for i in 1..macros.len() {
        if !macros[i].events.is_empty() {
            let start = u16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]) as usize;
            bytes[start - 4] |= 1;
        }
    }
    Ok(Prepared {
        image: Image::from_bytes(bytes)?,
        slots,
    })
}
#[cfg(test)]
#[path = "yjx_sc620_oem_compile_tests.rs"]
mod tests;
