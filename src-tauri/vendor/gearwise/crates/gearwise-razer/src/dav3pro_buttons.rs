//! Exact B6/B7 normal layer. Eight firmware IDs are in the official 182 UI.
//! Official RzCtl_00B6 uses 02/8C and 02/0C with ten-byte assignments.
//! Hypershift, host macro ownership and unknown original actions stay locked.
use crate::{basilisk, link::Link, report::Report};
use gearwise_core::{
    Error, Result,
    model::{ButtonAction, DpiFunction, MouseButton},
};
use std::collections::BTreeMap;
pub const BUTTONS: [(u16, &str); 8] = [
    (1, "左键"),
    (2, "右键"),
    (3, "滚轮中键"),
    (4, "侧键后退"),
    (5, "侧键前进"),
    (9, "滚轮向上"),
    (10, "滚轮向下"),
    (96, "底部 DPI 键"),
];
pub const MEDIA: [u16; 7] = [0xb5, 0xb6, 0xb7, 0xcd, 0xe2, 0xe9, 0xea];
pub fn default_block(id: u16) -> Option<[u8; 7]> {
    BUTTONS
        .iter()
        .any(|b| b.0 == id)
        .then(|| basilisk::default_block(id))
        .flatten()
}
pub fn encode(id: u16, action: &ButtonAction) -> Result<[u8; 7]> {
    if id == 1 || default_block(id).is_none() {
        return Err(Error::Unsupported(
            "DeathAdder protected/unknown physical input",
        ));
    }
    if matches!(
        action,
        ButtonAction::Mouse(MouseButton::WheelLeft | MouseButton::WheelRight)
    ) {
        return Err(Error::Unsupported(
            "DeathAdder horizontal scroll output not enabled",
        ));
    }
    if matches!(id, 9 | 10) && matches!(action, ButtonAction::Dpi(_)) {
        return Err(Error::Unsupported(
            "DeathAdder wheel assignment does not offer sensitivity actions",
        ));
    }
    match action {
        ButtonAction::Media(usage) if MEDIA.contains(usage) => {
            let [hi, lo] = usage.to_be_bytes();
            Ok([10, 2, hi, lo, 0, 0, 0])
        }
        ButtonAction::Dpi(DpiFunction::Up) => Ok([6, 1, 1, 0, 0, 0, 0]),
        ButtonAction::Dpi(DpiFunction::Down) => Ok([6, 1, 2, 0, 0, 0, 0]),
        ButtonAction::Dpi(DpiFunction::Shift(v)) if (100..=30000).contains(v) => {
            let [hi, lo] = v.to_be_bytes();
            Ok([6, 5, 5, hi, lo, hi, lo])
        }
        _ => basilisk::encode(id, action),
    }
}
pub fn decode(id: u16, raw: &[u8]) -> Option<ButtonAction> {
    if raw.len() != 7 || default_block(id).is_none() {
        return None;
    }
    if raw[..2] == [10, 2] && raw[4..] == [0, 0, 0] {
        let usage = u16::from_be_bytes([raw[2], raw[3]]);
        return MEDIA.contains(&usage).then_some(ButtonAction::Media(usage));
    }
    if !matches!(id, 9 | 10) && raw[..2] == [6, 1] && raw[3..] == [0; 4] {
        match raw[2] {
            1 => return Some(ButtonAction::Dpi(DpiFunction::Up)),
            2 => return Some(ButtonAction::Dpi(DpiFunction::Down)),
            _ => {}
        }
    }
    if !matches!(id, 9 | 10) && raw[..3] == [6, 5, 5] && raw[3..5] == raw[5..7] {
        let v = u16::from_be_bytes([raw[3], raw[4]]);
        return (100..=30000)
            .contains(&v)
            .then_some(ButtonAction::Dpi(DpiFunction::Shift(v)));
    }
    let action = basilisk::decode(id, raw)?;
    if matches!(
        action,
        ButtonAction::Mouse(MouseButton::WheelLeft | MouseButton::WheelRight)
    ) {
        return None;
    }
    Some(action)
}
fn mode(link: &mut Link) -> Result<()> {
    let r = link.exec(Report::new(0, 0x84, 2, &[]))?;
    if r.data_size != 2 || r.args[0] != 0 {
        return Err(Error::protocol(
            "DeathAdder is not in normal mode; close Synapse and read again",
        ));
    }
    Ok(())
}
fn get(link: &mut Link, id: u16) -> Result<[u8; 10]> {
    let r = link.exec(Report::new(2, 0x8c, 10, &[1, id as u8, 0]))?;
    if r.data_size != 10 || r.args[..3] != [1, id as u8, 0] {
        return Err(Error::protocol(
            "DeathAdder assignment profile/input/layer mismatch",
        ));
    }
    let mut raw = [0; 10];
    raw.copy_from_slice(&r.args[..10]);
    Ok(raw)
}
pub fn read(link: &mut Link) -> Result<BTreeMap<u16, ButtonAction>> {
    mode(link)?;
    let mut out = BTreeMap::new();
    for (id, _) in BUTTONS {
        if let Ok(raw) = get(link, id) {
            if let Some(action) = decode(id, &raw[3..]) {
                out.insert(id, action);
            }
        }
    }
    Ok(out)
}
pub fn apply(link: &mut Link, id: u16, action: &ButtonAction) -> Result<()> {
    let block = encode(id, action)?;
    mode(link)?;
    let before = get(link, id)?;
    if decode(id, &before[3..]).is_none() {
        return Err(Error::Unsupported(
            "unknown original DeathAdder binding retained",
        ));
    }
    let mut want = before;
    want[3..].copy_from_slice(&block);
    if want == before {
        return Ok(());
    }
    mode(link)?;
    let result = link
        .exec(Report::new(2, 0x0c, 10, &want))
        .and_then(|_| get(link, id))
        .and_then(|raw| {
            if raw == want {
                Ok(())
            } else {
                Err(Error::protocol("DeathAdder button readback mismatch"))
            }
        })
        .and_then(|_| mode(link));
    if let Err(error) = result {
        let restored = (|| {
            mode(link)?;
            let actual = get(link, id)?;
            if actual == before {
                return Ok(());
            }
            if actual != want {
                return Err(Error::protocol("foreign DeathAdder binding retained"));
            }
            mode(link)?;
            link.exec(Report::new(2, 0x0c, 10, &before))?;
            if get(link, id)? != before {
                return Err(Error::protocol(
                    "DeathAdder original assignment restore mismatch",
                ));
            }
            mode(link)
        })();
        return Err(Error::protocol(format!(
            "{error}; original binding recovery: {restored:?}; uncertain writes not retried"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RazerDevice, Timing, models};
    use gearwise_core::{
        Device, Setting,
        model::Modifiers,
        transport::{HidInterfaceInfo, MockOp, MockReply, MockTransport},
    };
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };
    #[test]
    fn official_eight_ids_and_fixed_assignment_record_match_independent_static_vector() {
        assert_eq!(BUTTONS.map(|v| v.0), [1, 2, 3, 4, 5, 9, 10, 96]);
        let block = encode(
            2,
            &ButtonAction::Key {
                usage: 4,
                mods: Modifiers(0x81),
            },
        )
        .unwrap();
        assert_eq!(block, [2, 2, 0x81, 4, 0, 0, 0]);
        let mut args = [0; 10];
        args[..3].copy_from_slice(&[1, 2, 0]);
        args[3..].copy_from_slice(&block);
        let mut r = Report::new(2, 12, 10, &args);
        r.transaction_id = 31;
        let mut expected = [0; 91];
        expected[..19].copy_from_slice(&[
            0, 0, 31, 0, 0, 0, 10, 2, 12, 1, 2, 0, 2, 2, 0x81, 4, 0, 0, 0,
        ]);
        expected[89] = 0x82;
        assert_eq!(r.to_wire(), expected);
        for id in [1, 2, 3, 4, 5, 9, 10] {
            assert_eq!(default_block(id).unwrap(), [1, 1, id as u8, 0, 0, 0, 0]);
        }
        assert_eq!(default_block(96).unwrap(), [6, 1, 6, 0, 0, 0, 0]);
    }
    #[test]
    fn modifiers_media_dpi_and_unknown_records_are_exactly_scoped() {
        for id in BUTTONS.map(|b| b.0).into_iter().filter(|id| *id != 1) {
            for mods in 0..=255 {
                let a = ButtonAction::Key {
                    usage: 4,
                    mods: Modifiers(mods),
                };
                assert_eq!(decode(id, &encode(id, &a).unwrap()), Some(a));
            }
            for usage in MEDIA {
                let a = ButtonAction::Media(usage);
                let b = encode(id, &a).unwrap();
                assert_eq!(&b[..4], &[10, 2, (usage >> 8) as u8, usage as u8]);
                assert_eq!(decode(id, &b), Some(a));
            }
            if !matches!(id, 9 | 10) {
                for function in [
                    DpiFunction::Up,
                    DpiFunction::Down,
                    DpiFunction::Cycle,
                    DpiFunction::Shift(30000),
                ] {
                    let a = ButtonAction::Dpi(function);
                    assert_eq!(decode(id, &encode(id, &a).unwrap()), Some(a));
                }
            }
        }
        for id in [0, 1, 6, 7, 8, 14, 106, 258] {
            assert!(encode(id, &ButtonAction::Disabled).is_err());
        }
        for a in [
            ButtonAction::Media(184),
            ButtonAction::Key {
                usage: 3,
                mods: Modifiers(0),
            },
            ButtonAction::Macro("m".into()),
            ButtonAction::Dpi(DpiFunction::Shift(30001)),
        ] {
            assert!(encode(2, &a).is_err());
        }
        assert!(encode(9, &ButtonAction::Dpi(DpiFunction::Cycle)).is_err());
        assert!(decode(2, &[2, 2, 1, 4, 1, 0, 0]).is_none());
        assert!(decode(2, &[12, 1, 1, 0, 0, 0, 0]).is_none());
        assert!(decode(96, &[6, 5, 5, 1, 144, 3, 32]).is_none());
    }
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Fault {
        None,
        SetterFailure,
        Busy,
        BadReadback,
        Foreign,
        RestoreFailure,
        OtherLayer,
        UnknownOriginal,
        DriverMode,
    }
    struct State {
        blocks: BTreeMap<u16, [u8; 7]>,
        writes: Vec<(u16, [u8; 7])>,
        gets: usize,
    }
    fn rig(pid: u16, fault: Fault) -> (RazerDevice, Arc<Mutex<State>>) {
        let state = Arc::new(Mutex::new(State {
            blocks: BUTTONS
                .into_iter()
                .map(|(id, _)| (id, default_block(id).unwrap()))
                .collect(),
            writes: vec![],
            gets: 0,
        }));
        let shared = state.clone();
        let io = MockTransport::new(move |op| {
            let MockOp::SetFeature(bytes) = op else {
                return MockReply::none();
            };
            let req = Report::from_wire(bytes).unwrap();
            let mut r = req.clone();
            r.status = 2;
            r.args = [0; 80];
            let mut corrupt = false;
            match (req.class, req.command) {
                (0, 0x82) => r.args[..19].copy_from_slice(b"SIMULATED-DAV3PRO-A"),
                (0, 0x81) => r.args[..2].copy_from_slice(&[2, 1]),
                (0, 0x84) => r.args[0] = u8::from(fault == Fault::DriverMode),
                (2, 0x8c) => {
                    assert_eq!(&req.args[..3], &[1, req.args[1], 0]);
                    let id = u16::from(req.args[1]);
                    let mut s = shared.lock().unwrap();
                    s.gets += 1;
                    r.args[..3].copy_from_slice(&req.args[..3]);
                    r.args[3..10].copy_from_slice(&s.blocks[&id]);
                    if fault == Fault::UnknownOriginal && id == 2 {
                        r.args[3] = 12;
                        r.args[4] = 1;
                        r.args[5] = 1;
                    }
                    corrupt = fault == Fault::BadReadback && s.gets == 2;
                }
                (2, 0x0c) => {
                    let id = u16::from(req.args[1]);
                    assert_eq!(req.args[..3], [1, id as u8, 0]);
                    let mut block = [0; 7];
                    block.copy_from_slice(&req.args[3..10]);
                    let mut s = shared.lock().unwrap();
                    s.blocks.insert(id, block);
                    s.writes.push((id, block));
                    r.args[..10].copy_from_slice(&req.args[..10]);
                    if s.writes.len() == 1 {
                        match fault {
                            Fault::Busy => r.status = 1,
                            Fault::OtherLayer => r.args[2] = 1,
                            Fault::SetterFailure | Fault::Foreign | Fault::RestoreFailure => {
                                r.status = 3
                            }
                            _ => {}
                        }
                        if fault == Fault::Foreign {
                            s.blocks.insert(id, [2, 2, 0, 5, 0, 0, 0]);
                        }
                    } else if fault == Fault::RestoreFailure {
                        r.status = 3;
                    }
                }
                _ => r.status = 5,
            }
            let mut bytes = r.to_wire().to_vec();
            if corrupt {
                bytes[89] ^= 1;
            }
            MockReply::feature(bytes)
        });
        let model = models::find(pid).unwrap();
        let link = Link::new(
            Box::new(io),
            model.transaction_id,
            Timing::new(Duration::ZERO, Duration::ZERO, 1),
        );
        (
            RazerDevice::open(model, link, &HidInterfaceInfo::default()).unwrap(),
            state,
        )
    }
    #[test]
    fn every_nonprimary_control_roundtrips_through_production_device_without_touching_other_inputs()
    {
        for pid in [0xb6, 0xb7] {
            let (mut d, s) = rig(pid, Fault::None);
            let initial = d.read_state().unwrap().buttons;
            assert_eq!(initial.len(), 8);
            assert_eq!(
                d.capabilities()
                    .buttons
                    .iter()
                    .filter(|b| b.remappable)
                    .count(),
                7
            );
            for id in [2, 3, 4, 5, 9, 10, 96] {
                let action = ButtonAction::Key {
                    usage: 6,
                    mods: Modifiers(0x83),
                };
                d.apply(&Setting::Button {
                    id,
                    action: action.clone(),
                })
                .unwrap();
                let now = d.read_state().unwrap().buttons;
                assert_eq!(now.get(&id), Some(&action));
                assert_eq!(now.get(&1), initial.get(&1));
            }
            assert_eq!(s.lock().unwrap().writes.len(), 7);
            for id in [2, 3, 4, 5, 9, 10, 96] {
                d.apply(&Setting::Button {
                    id,
                    action: ButtonAction::Default,
                })
                .unwrap();
            }
            assert_eq!(d.read_state().unwrap().buttons, initial);
        }
    }
    #[test]
    fn uncertain_button_write_is_not_resent_and_restores_only_owned_normal_layer_record() {
        for pid in [0xb6, 0xb7] {
            for fault in [
                Fault::SetterFailure,
                Fault::Busy,
                Fault::BadReadback,
                Fault::Foreign,
                Fault::RestoreFailure,
                Fault::OtherLayer,
            ] {
                let (mut d, s) = rig(pid, fault);
                let action = ButtonAction::Key {
                    usage: 6,
                    mods: Modifiers(3),
                };
                let err = d
                    .apply(&Setting::Button {
                        id: 2,
                        action: action.clone(),
                    })
                    .unwrap_err()
                    .to_string();
                assert!(err.contains("not retried"));
                let s = s.lock().unwrap();
                let wanted = encode(2, &action).unwrap();
                assert_eq!(s.writes.iter().filter(|(_, b)| *b == wanted).count(), 1);
                if fault == Fault::Foreign {
                    assert_eq!(s.writes.len(), 1);
                    assert_eq!(s.blocks[&2], [2, 2, 0, 5, 0, 0, 0]);
                } else {
                    assert_eq!(s.writes.len(), 2);
                    assert_eq!(s.blocks[&2], default_block(2).unwrap());
                }
                for id in [1, 3, 4, 5, 9, 10, 96] {
                    assert_eq!(s.blocks[&id], default_block(id).unwrap());
                }
            }
        }
    }
    #[test]
    fn unknown_original_driver_mode_and_invalid_inputs_never_write() {
        for pid in [0xb6, 0xb7] {
            for fault in [Fault::UnknownOriginal, Fault::DriverMode] {
                let (mut d, s) = rig(pid, fault);
                assert!(
                    d.apply(&Setting::Button {
                        id: 2,
                        action: ButtonAction::Disabled
                    })
                    .is_err()
                );
                assert!(s.lock().unwrap().writes.is_empty());
                let state = d.read_state().unwrap();
                if fault == Fault::UnknownOriginal {
                    assert!(!state.buttons.contains_key(&2));
                    assert!(
                        !d.capabilities()
                            .buttons
                            .iter()
                            .find(|b| b.id == 2)
                            .unwrap()
                            .remappable
                    );
                } else {
                    assert!(state.buttons.is_empty());
                }
            }
            let (mut d, s) = rig(pid, Fault::None);
            for id in [1, 6, 258] {
                assert!(
                    d.apply(&Setting::Button {
                        id,
                        action: ButtonAction::Disabled
                    })
                    .is_err()
                );
            }
            let s = s.lock().unwrap();
            assert!(s.writes.is_empty());
            assert_eq!(s.gets, 0);
        }
    }
}
