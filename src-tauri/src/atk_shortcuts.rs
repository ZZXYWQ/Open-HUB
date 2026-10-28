//! COMPX class5 shortcut codec and OEM-file chord conversion.
//! qn/V3 rows are [binary kind, state, value1, value2]; fixed maximum six.
use gearwise_core::{
    model::{ButtonAction, Modifiers, ShortcutEvent, ShortcutKind},
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
    from_rows(&rows)
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

pub fn encode_events(events: &[ShortcutEvent]) -> Result<Vec<u8>> {
    if !(2..=6).contains(&events.len()) {
        return Err(Error::invalid("COMPX native shortcut requires 2–6 events"));
    }
    let mut mods = 0u8;
    let mut keys = std::collections::BTreeSet::new();
    let mut mice = 0u8;
    let mut media = std::collections::BTreeSet::new();
    let mut bytes = vec![events.len() as u8];
    for e in events {
        let (kind, held) = match e.kind {
            ShortcutKind::Modifier => {
                if e.value == 0 {
                    return Err(Error::invalid("Empty shortcut modifier mask"));
                }
                if e.down {
                    if mods & e.value != 0 {
                        return Err(Error::invalid("Duplicate shortcut modifier down"));
                    }
                    mods |= e.value;
                } else {
                    if mods & e.value != e.value {
                        return Err(Error::invalid("Unmatched shortcut modifier up"));
                    }
                    mods &= !e.value;
                }
                (0, true)
            }
            ShortcutKind::Mouse => {
                if !matches!(e.value, 1 | 2 | 4 | 8 | 16) {
                    return Err(Error::Unsupported("COMPX shortcut mouse mask"));
                }
                if e.down {
                    if mice & e.value != 0 {
                        return Err(Error::invalid("Duplicate shortcut mouse down"));
                    }
                    mice |= e.value;
                } else {
                    if mice & e.value == 0 {
                        return Err(Error::invalid("Unmatched shortcut mouse up"));
                    }
                    mice &= !e.value;
                }
                (4, true)
            }
            ShortcutKind::Key => {
                if !(4..=0xa4).contains(&e.value) {
                    return Err(Error::invalid("Shortcut key usage"));
                }
                (
                    1,
                    if e.down {
                        keys.insert(e.value)
                    } else {
                        keys.remove(&e.value)
                    },
                )
            }
            ShortcutKind::Media => {
                super::native_macro::validate_media(u16::from(e.value))?;
                (
                    2,
                    if e.down {
                        media.insert(e.value)
                    } else {
                        media.remove(&e.value)
                    },
                )
            }
        };
        if !held {
            return Err(Error::invalid("Duplicate or unmatched shortcut key event"));
        }
        bytes.extend_from_slice(&[kind | if e.down { 0x80 } else { 0x40 }, e.value, 0]);
    }
    if mods != 0 || mice != 0 || !keys.is_empty() || !media.is_empty() {
        return Err(Error::invalid("Native shortcut has held inputs at end"));
    }
    let crc = 0x55u8.wrapping_sub(bytes.iter().fold(0u8, |a, b| a.wrapping_add(*b)));
    bytes.push(crc);
    Ok(bytes)
}
pub fn from_rows(rows: &[[u8; 4]]) -> Result<ButtonAction> {
    let mut events = vec![];
    for &[kind, state, value, extra] in rows {
        if extra != 0 || state > 1 {
            return Err(Error::Unsupported("Shortcut state/extended value"));
        }
        let kind = match kind {
            0 => ShortcutKind::Modifier,
            1 => ShortcutKind::Key,
            2 => ShortcutKind::Media,
            4 => ShortcutKind::Mouse,
            _ => return Err(Error::Unsupported("Unknown native shortcut event kind")),
        };
        events.push(ShortcutEvent {
            kind,
            down: state == 0,
            value,
        });
    }
    let encoded = encode_events(&events)?;
    if let Ok(action) = chord(rows) {
        if let ButtonAction::Key { usage, mods } = action {
            if encode(usage, mods)? == encoded {
                return Ok(ButtonAction::Key { usage, mods });
            }
        }
    }
    Ok(ButtonAction::Shortcut(events))
}
