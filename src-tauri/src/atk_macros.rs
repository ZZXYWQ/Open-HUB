//! ATK 384-byte native macro slots; independent implementation of the official
//! HUB 3.2.27 ko.setMacro / zv / Ct layout, not imported vendor application code.
use gearwise_core::model::{ButtonAction, Modifiers, MouseButton};
use gearwise_core::profile::{Macro, MacroMode, MacroStep};
use gearwise_core::{Error, Result};
pub const HEADER: [u8; 31] = [
    8, 2, 2, 2, 2, 2, 2, 2, 2, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255,
];
fn mouse(b: MouseButton) -> Result<u8> {
    match b {
        MouseButton::Left => Ok(1),
        MouseButton::Right => Ok(2),
        MouseButton::Middle => Ok(4),
        MouseButton::Back => Ok(8),
        MouseButton::Forward => Ok(16),
        _ => Err(Error::Unsupported("ATK macro mouse action")),
    }
}
fn key(usage: u8, down: bool) -> Result<[u8; 5]> {
    let (state, kind, value) = if (0xe0..=0xe7).contains(&usage) {
        (if down { 128 } else { 64 }, 0, 1 << (usage - 0xe0))
    } else if (4..=0xa4).contains(&usage) {
        (if down { 128 } else { 64 }, 1, usage)
    } else {
        return Err(Error::invalid("ATK native macro key usage"));
    };
    Ok([state | kind, value, 0, 0, 1])
}
pub fn mode(m: MacroMode) -> u8 {
    match m {
        MacroMode::Once => 1,
        MacroMode::WhileHeld => 254,
        MacroMode::Toggle => 253,
    }
}
pub fn keyboard(usage: u8, mods: Modifiers) -> Macro {
    let mut steps = Vec::new();
    for n in 0..8 {
        if mods.0 & (1 << n) != 0 {
            steps.push(MacroStep::KeyDown(0xe0 + n));
        }
    }
    steps.push(MacroStep::KeyDown(usage));
    steps.push(MacroStep::KeyUp(usage));
    for n in (0..8).rev() {
        if mods.0 & (1 << n) != 0 {
            steps.push(MacroStep::KeyUp(0xe0 + n));
        }
    }
    Macro {
        id: "atk-keyboard-tap".into(),
        name: "键盘快捷键（点按）".into(),
        mode: MacroMode::Once,
        steps,
    }
}
/// HID consumer usages verified in the official COMPX media event class.
/// Two-byte/browser usages are not inferred from the one-byte media table.
pub const MEDIA_USAGES: &[u16] = &[0xb5, 0xb6, 0xb7, 0xcd, 0xe2, 0xe9, 0xea];
pub fn validate_media(usage: u16) -> Result<()> {
    if MEDIA_USAGES.contains(&usage) {
        Ok(())
    } else {
        Err(Error::Unsupported(
            "COMPX native media usage is not verified",
        ))
    }
}
pub fn media(usage: u16) -> Macro {
    Macro {
        id: "atk-media-tap".into(),
        name: "多媒体快捷键（点按）".into(),
        mode: MacroMode::Once,
        steps: vec![MacroStep::Media(usage)],
    }
}
pub fn media_action(m: &Macro) -> Option<ButtonAction> {
    if m.mode != MacroMode::Once {
        return None;
    }
    match m.steps.as_slice() {
        [MacroStep::Media(usage)] if validate_media(*usage).is_ok() => {
            Some(ButtonAction::Media(*usage))
        }
        _ => None,
    }
}
pub fn encode(m: &Macro) -> Result<Vec<u8>> {
    crate::host_macros::validate(m).map_err(Error::invalid)?;
    let mut events = Vec::<[u8; 5]>::new();
    let mut delayed = false;
    for step in &m.steps {
        if !matches!(step, MacroStep::Delay(_)) {
            delayed = false;
        }
        match step{
  MacroStep::KeyDown(v)=>events.push(key(*v,true)?),MacroStep::KeyUp(v)=>events.push(key(*v,false)?),
  MacroStep::MouseDown(b)|MacroStep::MouseUp(b)=>events.push([if matches!(step,MacroStep::MouseDown(_)){132}else{68},mouse(*b)?,0,0,1]),
  MacroStep::Delay(ms)=>{if *ms==0{continue;}let prev=events.last_mut().ok_or(Error::Unsupported("ATK leading macro delay"))?;let delay=(if delayed{u16::from_be_bytes([prev[3],prev[4]])}else{0}).checked_add(u16::try_from(*ms).map_err(|_|Error::invalid("ATK delay overflow"))?).ok_or(Error::invalid("ATK delay overflow"))?;let bytes=delay.to_be_bytes();prev[3..5].copy_from_slice(&bytes);delayed=true;},
  MacroStep::Media(usage)=>{validate_media(*usage)?;events.push([0x82,*usage as u8,0,0,1]);events.push([0x42,*usage as u8,0,0,0]);},
  MacroStep::KeyTap{usage,mods}=>{let tap=keyboard(*usage,*mods);for s in tap.steps{match s{MacroStep::KeyDown(v)=>events.push(key(v,true)?),MacroStep::KeyUp(v)=>events.push(key(v,false)?),_=>unreachable!()}}},
  _=>return Err(Error::Unsupported("ATK native macros support key/mouse/media events and delays; text/physical-wait is unsupported")),
 }
    }
    if events.is_empty() || events.len() > 70 {
        return Err(Error::invalid(
            "ATK native macro supports 1–70 input events",
        ));
    }
    let mut out = HEADER.to_vec();
    out.push(events.len() as u8);
    for e in events {
        out.extend(e);
    }
    let sum = out[31..].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    out.push(0x55u8.wrapping_sub(sum));
    Ok(out)
}
pub fn decode(slot: u8, play: u8, data: &[u8]) -> Result<Macro> {
    if data.len() < 33 || data[..31] != HEADER {
        return Err(Error::invalid("ATK native macro header"));
    }
    let n = data[31] as usize;
    if n == 0
        || n > 70
        || data.len() != 33 + n * 5
        || data[31..].iter().fold(0u8, |a, b| a.wrapping_add(*b)) != 0x55
    {
        return Err(Error::invalid("ATK native macro count/checksum"));
    }
    let mode = match play {
        1 => MacroMode::Once,
        254 => MacroMode::WhileHeld,
        253 => MacroMode::Toggle,
        _ => return Err(Error::Unsupported("ATK native macro repeat count")),
    };
    let mut steps = Vec::new();
    let rows = data[32..data.len() - 1].chunks_exact(5).collect::<Vec<_>>();
    let mut at = 0;
    while at < rows.len() {
        let row = rows[at];
        if row[0] == 0x82 {
            let release = rows
                .get(at + 1)
                .ok_or(Error::invalid("ATK media release missing"))?;
            let usage = u16::from(row[1]);
            validate_media(usage)?;
            if row[2] != 0 || row[3..5] != [0, 1] || release[..3] != [0x42, row[1], 0] {
                return Err(Error::Unsupported("ATK media hold/release layout retained"));
            }
            steps.push(MacroStep::Media(usage));
            let delay = u16::from_be_bytes([release[3], release[4]]);
            if delay > 0 {
                steps.push(MacroStep::Delay(u32::from(delay)));
            }
            at += 2;
            continue;
        }
        at += 1;
        let down = match row[0] & 0xc0 {
            128 => true,
            64 => false,
            _ => return Err(Error::Unsupported("ATK native macro scroll/status")),
        };
        if row[2] != 0 {
            return Err(Error::Unsupported("ATK native macro extended key"));
        }
        match row[0] & 0x3f {
            0 => {
                if row[1] == 0 {
                    return Err(Error::invalid("ATK macro empty modifiers"));
                }
                for n in 0..8 {
                    if row[1] & (1 << n) != 0 {
                        steps.push(if down {
                            MacroStep::KeyDown(0xe0 + n)
                        } else {
                            MacroStep::KeyUp(0xe0 + n)
                        });
                    }
                }
            }
            1 if (4..=0xa4).contains(&row[1]) => steps.push(if down {
                MacroStep::KeyDown(row[1])
            } else {
                MacroStep::KeyUp(row[1])
            }),
            4 => {
                let b = match row[1] {
                    1 => MouseButton::Left,
                    2 => MouseButton::Right,
                    4 => MouseButton::Middle,
                    8 => MouseButton::Back,
                    16 => MouseButton::Forward,
                    _ => return Err(Error::Unsupported("ATK macro mouse mask")),
                };
                steps.push(if down {
                    MacroStep::MouseDown(b)
                } else {
                    MacroStep::MouseUp(b)
                });
            }
            _ => return Err(Error::Unsupported("ATK native macro event class")),
        }
        let ms = u16::from_be_bytes([row[3], row[4]]);
        if ms > 0 {
            steps.push(MacroStep::Delay(u32::from(ms)));
        }
    }
    let m = Macro {
        id: format!("atk-native:{slot}:{play}"),
        name: format!("板载宏 {}", slot + 1),
        mode,
        steps,
    };
    crate::host_macros::validate(&m).map_err(Error::invalid)?;
    Ok(m)
}
pub fn key_action(action: &ButtonAction) -> Option<Macro> {
    if let ButtonAction::Key { usage, mods } = action {
        Some(keyboard(*usage, *mods))
    } else if let ButtonAction::Media(usage) = action {
        Some(media(*usage))
    } else {
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atk_native_macro_wire_layout_and_all_modes() {
        for mode in [MacroMode::Once, MacroMode::WhileHeld, MacroMode::Toggle] {
            let m = Macro {
                id: "m".into(),
                name: "m".into(),
                mode,
                steps: vec![
                    MacroStep::KeyDown(4),
                    MacroStep::Delay(50),
                    MacroStep::KeyUp(4),
                ],
            };
            let p = encode(&m).unwrap();
            assert_eq!(&p[31..42], &[2, 129, 4, 0, 0, 50, 65, 4, 0, 0, 1]);
            let d = decode(0, super::mode(mode), &p).unwrap();
            assert_eq!(d.mode, mode);
            assert_eq!(
                &d.steps[..3],
                &[
                    MacroStep::KeyDown(4),
                    MacroStep::Delay(50),
                    MacroStep::KeyUp(4)
                ]
            );
        }
    }
    #[test]
    fn atk_native_macro_rejects_unsupported_or_bad_data() {
        let m = Macro {
            id: "m".into(),
            name: "m".into(),
            mode: MacroMode::Once,
            steps: vec![MacroStep::Text("test".into())],
        };
        assert!(encode(&m).is_err());
        let m = keyboard(4, Modifiers(3));
        let mut p = encode(&m).unwrap();
        assert!(decode(0, 1, &p).is_ok());
        *p.last_mut().unwrap() ^= 1;
        assert!(decode(0, 1, &p).is_err());
    }
}
#[cfg(test)]
mod media_tests {
    use super::*;
    #[test]
    fn media_tap_has_independent_official_event_class_and_consumer_usage_vectors() {
        let p = encode(&media(0xe2)).unwrap();
        assert_eq!(
            &p[31..],
            &[2, 0x82, 0xe2, 0, 0, 1, 0x42, 0xe2, 0, 0, 0, 0xca]
        );
        let p = encode(&media(0xe9)).unwrap();
        assert_eq!(
            &p[31..],
            &[2, 0x82, 0xe9, 0, 0, 1, 0x42, 0xe9, 0, 0, 0, 0xbc]
        );
        for usage in MEDIA_USAGES {
            for mode in [MacroMode::Once, MacroMode::WhileHeld, MacroMode::Toggle] {
                let mut m = media(*usage);
                m.mode = mode;
                let raw = encode(&m).unwrap();
                let got = decode(0, super::mode(mode), &raw).unwrap();
                assert_eq!(got.steps, m.steps);
                assert_eq!(got.mode, mode);
                assert_eq!(encode(&got).unwrap(), raw);
            }
        }
    }
    #[test]
    fn unknown_media_extended_usage_or_nonatomic_hold_is_retained() {
        assert!(encode(&media(0x183)).is_err());
        for at in [33, 34, 36, 38] {
            let mut raw = encode(&media(0xe2)).unwrap();
            raw[at] ^= 1;
            let n = raw.len() - 1;
            raw[n] = 0x55u8.wrapping_sub(raw[31..n].iter().fold(0u8, |a, v| a.wrapping_add(*v)));
            assert!(decode(0, 1, &raw).is_err());
        }
    }
}
