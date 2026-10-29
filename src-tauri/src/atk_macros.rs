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
        MacroMode::Repeat(count) => count,
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
  MacroStep::ScrollTimed(v)=>{let pulse=v.pulse_ms.to_be_bytes();let settle=v.settle_ms.to_be_bytes();events.push([6,if matches!(v.direction,gearwise_core::model::ScrollDirection::Up){1}else{255},0,pulse[0],pulse[1]]);events.push([6,0,0,settle[0],settle[1]]);delayed=true;},
  MacroStep::Scroll(direction)=>{events.push([6,if matches!(direction,gearwise_core::model::ScrollDirection::Up){1}else{255},0,0,1]);events.push([6,0,0,0,0]);},
  MacroStep::Media(usage)=>{validate_media(*usage)?;events.push([0x82,*usage as u8,0,0,1]);events.push([0x42,*usage as u8,0,0,0]);},
  MacroStep::KeyTap{usage,mods}=>{let tap=keyboard(*usage,*mods);for s in tap.steps{match s{MacroStep::KeyDown(v)=>events.push(key(v,true)?),MacroStep::KeyUp(v)=>events.push(key(v,false)?),_=>unreachable!()}}},
  _=>return Err(Error::Unsupported("ATK native macros support key/mouse/media events and delays; text/physical-wait is unsupported")),
 }
    }
    if events.iter().any(|row| row[0] == 6 && u16::from_be_bytes([row[3],row[4]]) > 10000) {
        return Err(Error::invalid("COMPX combined wheel interval exceeds 10000 ms"));
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
        count @ 2..=252 => MacroMode::Repeat(count),
        _ => return Err(Error::Unsupported("ATK native macro repeat count")),
    };
    let mut steps = Vec::new();
    let rows = data[32..data.len() - 1].chunks_exact(5).collect::<Vec<_>>();
    let mut at = 0;
    while at < rows.len() {
        let row = rows[at];
        if row[0] == 6 {
            let neutral=rows.get(at+1).ok_or(Error::invalid("COMPX scroll neutral event missing"))?;
            let pulse=u16::from_be_bytes([row[3],row[4]]);
            if !matches!(row[1],1|255)||row[2]!=0||pulse==0||neutral[..3]!=[6,0,0]{
                return Err(Error::Unsupported("COMPX noncanonical scroll pair retained"));
            }
            let direction=if row[1]==1 {gearwise_core::model::ScrollDirection::Up}else{gearwise_core::model::ScrollDirection::Down};
            let delay=u16::from_be_bytes([neutral[3],neutral[4]]);
            if pulse==1 {
                steps.push(MacroStep::Scroll(direction));
                if delay>0{steps.push(MacroStep::Delay(u32::from(delay)));}
            } else {
                steps.push(MacroStep::ScrollTimed(gearwise_core::profile::TimedScroll{direction,pulse_ms:pulse,settle_ms:delay}));
            }
            at+=2;continue;
        }
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

#[cfg(test)]
mod scroll_tests{
 use super::*;use gearwise_core::model::ScrollDirection;
 fn definition(direction:ScrollDirection)->Macro{Macro{id:"scroll".into(),name:"native wheel".into(),mode:MacroMode::Once,steps:vec![MacroStep::Scroll(direction)]}}
 #[test]fn up_down_have_independent_complete_native_pair_vectors(){for (direction,body,checksum) in[(ScrollDirection::Up,[6,1,0,0,1,6,0,0,0,0],69),(ScrollDirection::Down,[6,255,0,0,1,6,0,0,0,0],71)]{let mut golden=HEADER.to_vec();golden.push(2);golden.extend(body);golden.push(checksum);let m=definition(direction);assert_eq!(encode(&m).unwrap(),golden);assert_eq!(decode(0,1,&golden).unwrap().steps,m.steps);}}
 #[test]fn scroll_delay_roundtrips_without_losing_direction_or_neutral_event(){let mut m=definition(ScrollDirection::Up);m.steps.extend([MacroStep::Delay(37),MacroStep::Scroll(ScrollDirection::Down)]);let raw=encode(&m).unwrap();assert_eq!(&raw[32..52],&[6,1,0,0,1,6,0,0,0,37,6,255,0,0,1,6,0,0,0,0]);assert_eq!(decode(2,1,&raw).unwrap().steps,m.steps);}
 #[test]fn unknown_scroll_shapes_remain_opaque_instead_of_normalized(){let original=encode(&definition(ScrollDirection::Up)).unwrap();for(at,value)in[(32,0x46),(33,2),(34,1),(36,0),(37,7),(38,1),(39,1)]{let mut raw=original.clone();raw[at]=value;let n=raw.len();raw[n-1]=0x55u8.wrapping_sub(raw[31..n-1].iter().fold(0u8,|a,b|a.wrapping_add(*b)));assert!(decode(0,1,&raw).is_err(),"offset {at}");}let mut raw=original[..37].to_vec();raw[31]=1;raw.push(0x55u8.wrapping_sub(raw[31..].iter().fold(0u8,|a,b|a.wrapping_add(*b))));assert!(decode(0,1,&raw).is_err());}
 #[test]fn scroll_uses_two_native_events_and_obeys_existing_total_limits(){let mut m=definition(ScrollDirection::Down);m.steps=vec![MacroStep::Scroll(ScrollDirection::Down);35];assert_eq!(encode(&m).unwrap().len(),383);m.steps.push(MacroStep::Scroll(ScrollDirection::Up));assert!(encode(&m).is_err());assert!(serde_json::from_value::<MacroStep>(serde_json::json!({"Scroll":"Left"})).is_err());}
}

#[cfg(test)]mod timed_wheel_tests {
 use super::*;use gearwise_core::{model::ScrollDirection,profile::TimedScroll};
 #[test]fn timed_wheel_has_independent_full_native_vectors_and_lossless_readback(){
  for(direction,value)in[(ScrollDirection::Up,1u8),(ScrollDirection::Down,255)]{for(pulse,settle)in[(2u16,0u16),(37,37),(257,511),(10000,10000)]{for playback in [MacroMode::Once,MacroMode::Repeat(3),MacroMode::Toggle,MacroMode::WhileHeld]{
   let m=Macro{id:"timed".into(),name:"wheel".into(),mode:playback,steps:vec![MacroStep::ScrollTimed(TimedScroll{direction,pulse_ms:pulse,settle_ms:settle})]};
   let mut golden=vec![8];golden.extend([2;8]);golden.extend([255;22]);golden.push(2);let[a,b]=pulse.to_be_bytes();let[c,d]=settle.to_be_bytes();golden.extend([6,value,0,a,b,6,0,0,c,d]);let crc=0x55u8.wrapping_sub(golden[31..].iter().fold(0u8,|x,y|x.wrapping_add(*y)));golden.push(crc);
   assert_eq!(encode(&m).unwrap(),golden);let read=decode(0,mode(playback),&golden).unwrap();assert_eq!(read.steps,m.steps);assert_eq!(read.mode,playback);
  }}}
 }
 #[test]fn timed_wheel_preserves_neutral_delay_extensions_and_rejects_invalid_intervals(){
  let mut m=Macro{id:"timed".into(),name:"wheel".into(),mode:MacroMode::Once,steps:vec![MacroStep::ScrollTimed(TimedScroll{direction:ScrollDirection::Up,pulse_ms:37,settle_ms:41}),MacroStep::Delay(7)]};let raw=encode(&m).unwrap();assert_eq!(&raw[32..42],&[6,1,0,0,37,6,0,0,0,48]);
  for(pulse,settle)in[(0,0),(1,1),(10001,1),(2,10001)]{m.steps=vec![MacroStep::ScrollTimed(TimedScroll{direction:ScrollDirection::Down,pulse_ms:pulse,settle_ms:settle})];assert!(encode(&m).is_err());}
  m.steps=vec![MacroStep::ScrollTimed(TimedScroll{direction:ScrollDirection::Down,pulse_ms:10000,settle_ms:10000});4];assert!(encode(&m).is_err());
 }
}
