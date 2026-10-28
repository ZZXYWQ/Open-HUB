//! COMPX class5 shortcut codec and OEM-file chord conversion.
//! qn/V3 rows are [binary kind, state, value1, value2]; fixed maximum six.
use gearwise_core::{
    model::{ButtonAction, Modifiers},
    Error, Result,
};
pub fn chord(rows: &[[u8; 4]]) -> Result<ButtonAction> {
    if !(2..=6).contains(&rows.len()) {
        return Err(Error::invalid(
            "COMPX shortcut requires 2–6 complete events",
        ));
    }
    let mut mods = 0u8;
    let mut held = 0u8;
    let mut usage = None;
    let mut released = false;
    for &[kind, state, value, extra] in rows {
        if extra != 0 {
            return Err(Error::Unsupported("COMPX shortcut extended value"));
        }
        match (kind, state) {
            (0, 0) if usage.is_none() && value != 0 && held & value == 0 => {
                held |= value;
                mods |= value;
            }
            (1, 0) if usage.is_none() && (4..=0xa4).contains(&value) => usage = Some(value),
            (1, 1) if usage == Some(value) && !released => released = true,
            (0, 1) if released && value != 0 && held & value == value => held &= !value,
            _ => {
                return Err(Error::Unsupported(
                    "COMPX shortcut is not a complete single-key chord",
                ))
            }
        }
    }
    if held != 0 || !released {
        return Err(Error::invalid("COMPX shortcut has unbalanced held events"));
    }
    Ok(ButtonAction::Key {
        usage: usage.ok_or(Error::invalid("COMPX shortcut missing key"))?,
        mods: Modifiers(mods),
    })
}
pub fn decode(raw: &[u8]) -> Result<ButtonAction> {
    if raw.len() != 32 || !(2..=6).contains(&raw[0]) {
        return Err(Error::invalid("COMPX shortcut table geometry"));
    }
    let end = 1 + usize::from(raw[0]) * 3;
    if raw[..=end].iter().fold(0u8, |a, b| a.wrapping_add(*b)) != 0x55 {
        return Err(Error::invalid("COMPX shortcut table checksum"));
    }
    let mut rows = vec![];
    for r in raw[1..end].chunks_exact(3) {
        let state = match r[0] & 0xc0 {
            0x80 => 0,
            0x40 => 1,
            _ => return Err(Error::Unsupported("COMPX shortcut event status")),
        };
        if r[0] & 0x38 != 0 {
            return Err(Error::Unsupported("COMPX shortcut reserved event flags"));
        }
        rows.push([r[0] & 7, state, r[1], r[2]]);
    }
    chord(&rows)
}

/// Canonical finite class5 chord, with no delay fields or macro allocation.
/// Modifier bitmasks are one record, as defined by qn/V3.
pub fn encode(usage: u8, mods: Modifiers) -> Result<Vec<u8>> {
    if !(4..=0xa4).contains(&usage) {
        return Err(Error::invalid("COMPX shortcut keyboard usage"));
    }
    let mut rows = vec![];
    if mods.0 != 0 {
        rows.push([0x80, mods.0, 0]);
    }
    rows.push([0x81, usage, 0]);
    rows.push([0x41, usage, 0]);
    if mods.0 != 0 {
        rows.push([0x40, mods.0, 0]);
    }
    let mut bytes = vec![rows.len() as u8];
    for row in rows {
        bytes.extend_from_slice(&row);
    }
    let crc = 0x55u8.wrapping_sub(bytes.iter().fold(0u8, |a, b| a.wrapping_add(*b)));
    bytes.push(crc);
    Ok(bytes)
}
