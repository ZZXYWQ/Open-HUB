//! Exact B6/B7 normal/Hypershift layers. Eight firmware IDs are in the official 182 UI.
//! Official RzCtl_00B6 uses 02/8C and 02/0C with ten-byte assignments.
//! Layer selectors follow the official record; momentary switch facts are documented
//! by razerqdhid 4363a883 (MIT). Host macros and unknown originals stay locked.
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
/// Stable driver-local ID: normal=physical; secondary=0x100|physical.
/// Never truncate an arbitrary integer into an on-wire input or layer.
pub fn wire(id: u16) -> Option<(u8, u8)> {
    if id > 0x1ff {
        return None;
    }
    let layer = (id >> 8) as u8;
    let physical = id as u8;
    BUTTONS
        .iter()
        .any(|b| b.0 == u16::from(physical))
        .then_some((physical, layer))
}
pub fn controls() -> Vec<(u16, String)> {
    [0, 0x100]
        .into_iter()
        .flat_map(|layer| {
            BUTTONS.into_iter().map(move |(id, name)| {
                (
                    layer | id,
                    if layer == 0 {
                        name.into()
                    } else {
                        format!("Hypershift · {name}")
                    },
                )
            })
        })
        .collect()
}
pub fn default_block(id: u16) -> Option<[u8; 7]> {
    let (physical, _) = wire(id)?;
    basilisk::default_block(u16::from(physical))
}
pub fn encode(id: u16, action: &ButtonAction) -> Result<[u8; 7]> {
    if id == 1 || wire(id).is_none() {
        return Err(Error::Unsupported(
            "DeathAdder protected/unknown physical input",
        ));
    }
    let (physical, layer) = wire(id).unwrap();
    if matches!(
        action,
        ButtonAction::Mouse(MouseButton::WheelLeft | MouseButton::WheelRight)
    ) {
        return Err(Error::Unsupported(
            "DeathAdder horizontal scroll output not enabled",
        ));
    }
    if matches!(physical, 9 | 10) && matches!(action, ButtonAction::Dpi(_)) {
        return Err(Error::Unsupported(
            "DeathAdder wheel assignment does not offer sensitivity actions",
        ));
    }
    match action {
        ButtonAction::Default => Ok(default_block(id).unwrap()),
        ButtonAction::Hypershift if layer == 0 && matches!(physical, 2..=5) => {
            Ok([12, 1, 1, 0, 0, 0, 0])
        }
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
        _ => basilisk::encode(
            if physical == 1 {
                2
            } else {
                u16::from(physical)
            },
            action,
        ),
    }
}
pub fn decode(id: u16, raw: &[u8]) -> Option<ButtonAction> {
    if raw.len() != 7 || default_block(id).is_none() {
        return None;
    }
    let (physical, layer) = wire(id)?;
    if raw == [12, 1, 1, 0, 0, 0, 0] {
        return (layer == 0 && matches!(physical, 2..=5)).then_some(ButtonAction::Hypershift);
    }
    if raw[..2] == [10, 2] && raw[4..] == [0, 0, 0] {
        let usage = u16::from_be_bytes([raw[2], raw[3]]);
        return MEDIA.contains(&usage).then_some(ButtonAction::Media(usage));
    }
    if !matches!(physical, 9 | 10) && raw[..2] == [6, 1] && raw[3..] == [0; 4] {
        match raw[2] {
            1 => return Some(ButtonAction::Dpi(DpiFunction::Up)),
            2 => return Some(ButtonAction::Dpi(DpiFunction::Down)),
            _ => {}
        }
    }
    if !matches!(physical, 9 | 10) && raw[..3] == [6, 5, 5] && raw[3..5] == raw[5..7] {
        let v = u16::from_be_bytes([raw[3], raw[4]]);
        return (100..=30000)
            .contains(&v)
            .then_some(ButtonAction::Dpi(DpiFunction::Shift(v)));
    }
    let action = basilisk::decode(u16::from(physical), raw)?;
    if matches!(
        action,
        ButtonAction::Mouse(MouseButton::WheelLeft | MouseButton::WheelRight)
    ) || (matches!(physical, 9 | 10) && matches!(action, ButtonAction::Dpi(_)))
    {
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
    let (physical, layer) = wire(id).ok_or(Error::Unsupported("DeathAdder input/layer"))?;
    let selector = [1, physical, layer];
    let r = link.exec(Report::new(2, 0x8c, 10, &selector))?;
    if r.data_size != 10 || r.args[..3] != selector {
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
    for (id, _) in controls() {
        if let Ok(raw) = get(link, id) {
            if let Some(action) = decode(id, &raw[3..]) {
                out.insert(id, action);
            }
        }
    }
    Ok(out)
}
// Synapse can leave the primary physical input reassigned before Open HUB opens.
// Protecting physical input1 alone therefore cannot protect the last left click.
fn keep_normal_left(link: &mut Link, id: u16, from: &[u8; 10], to: &[u8; 10]) -> Result<()> {
    const LEFT: [u8; 7] = [1, 1, 1, 0, 0, 0, 0];
    if from[2] != 0 || from[3..] != LEFT || to[3..] == LEFT {
        return Ok(());
    }
    for (other, _) in BUTTONS {
        if other != id {
            if let Ok(raw) = get(link, other) {
                if raw[3..] == LEFT {
                    return Ok(());
                }
            }
        }
    }
    Err(Error::invalid(
        "retain at least one verified normal-layer left click before replacing this binding",
    ))
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
    keep_normal_left(link, id, &before, &want)?;
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
            keep_normal_left(link, id, &actual, &before)?;
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
        for id in [0, 1, 6, 7, 8, 14, 106, 512, 65535] {
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
        assert_eq!(
            decode(2, &[12, 1, 1, 0, 0, 0, 0]),
            Some(ButtonAction::Hypershift)
        );
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
            blocks: controls()
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
                    assert_eq!(req.args[0], 1);
                    assert!(req.args[2] <= 1);
                    let id = u16::from(req.args[1]) | (u16::from(req.args[2]) << 8);
                    let mut s = shared.lock().unwrap();
                    s.gets += 1;
                    r.args[..3].copy_from_slice(&req.args[..3]);
                    r.args[3..10].copy_from_slice(&s.blocks[&id]);
                    if fault == Fault::UnknownOriginal && matches!(id, 2 | 258) {
                        r.args[3..10].copy_from_slice(&[3, 3, 0, 1, 1, 0, 0]);
                    }
                    corrupt = fault == Fault::BadReadback && s.gets == 2;
                }
                (2, 0x0c) => {
                    let id = u16::from(req.args[1]) | (u16::from(req.args[2]) << 8);
                    assert_eq!(req.args[0], 1);
                    assert!(req.args[2] <= 1);
                    let mut block = [0; 7];
                    block.copy_from_slice(&req.args[3..10]);
                    let mut s = shared.lock().unwrap();
                    s.blocks.insert(id, block);
                    s.writes.push((id, block));
                    r.args[..10].copy_from_slice(&req.args[..10]);
                    if s.writes.len() == 1 {
                        match fault {
                            Fault::Busy => r.status = 1,
                            Fault::OtherLayer => r.args[2] ^= 1,
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
            assert_eq!(initial.len(), 16);
            assert_eq!(
                d.capabilities()
                    .buttons
                    .iter()
                    .filter(|b| b.remappable)
                    .count(),
                15
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
                    assert!(!state.buttons.contains_key(&258));
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
            for id in [1, 6, 512, 65535] {
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
    #[test]
    fn secondary_layer_ids_and_native_momentary_switch_match_independent_wire_facts() {
        assert_eq!(controls().len(), 16);
        for id in 0..=u16::MAX {
            let expected = [
                1, 2, 3, 4, 5, 9, 10, 96, 257, 258, 259, 260, 261, 265, 266, 352,
            ]
            .contains(&id);
            assert_eq!(wire(id).is_some(), expected);
        }
        for (id, args, crc) in [
            (257, [1, 1, 1, 1, 1, 2, 0, 0, 0, 0], 0x07),
            (4, [1, 4, 0, 12, 1, 1, 0, 0, 0, 0], 0x0d),
        ] {
            let action = if id == 4 {
                ButtonAction::Hypershift
            } else {
                ButtonAction::Mouse(MouseButton::Right)
            };
            let mut full = [0; 91];
            full[..9].copy_from_slice(&[0, 0, 31, 0, 0, 0, 10, 2, 12]);
            full[9..19].copy_from_slice(&args);
            full[89] = crc;
            let (physical, layer) = wire(id).unwrap();
            let mut data = [1, physical, layer, 0, 0, 0, 0, 0, 0, 0];
            data[3..].copy_from_slice(&encode(id, &action).unwrap());
            let mut r = Report::new(2, 12, 10, &data);
            r.transaction_id = 31;
            assert_eq!(r.to_wire(), full);
            assert_eq!(decode(id, &data[3..]), Some(action));
        }
        for id in [1, 9, 10, 96, 257, 258, 259, 260, 261, 265, 266, 352] {
            assert!(encode(id, &ButtonAction::Hypershift).is_err());
        }
        for id in [9, 10, 265, 266] {
            assert!(encode(id, &ButtonAction::Dpi(DpiFunction::Up)).is_err());
        }
        assert!(decode(258, &[12, 1, 1, 0, 0, 0, 0]).is_none());
        for id in [9, 10, 265, 266] {
            assert!(decode(id, &[6, 1, 6, 0, 0, 0, 0]).is_none());
        }
    }
    #[test]
    fn all_secondary_bindings_and_switch_are_independent_from_normal_records() {
        for pid in [0xb6, 0xb7] {
            let (mut d, s) = rig(pid, Fault::None);
            let before = d.read_state().unwrap().buttons;
            d.apply(&Setting::Button {
                id: 4,
                action: ButtonAction::Hypershift,
            })
            .unwrap();
            for (id, _) in controls().into_iter().filter(|b| b.0 >= 256) {
                let action = ButtonAction::Key {
                    usage: (id as u8 % 20) + 4,
                    mods: Modifiers(0x80),
                };
                d.apply(&Setting::Button {
                    id,
                    action: action.clone(),
                })
                .unwrap();
                let now = d.read_state().unwrap().buttons;
                assert_eq!(now.get(&id), Some(&action));
                assert_eq!(now.get(&4), Some(&ButtonAction::Hypershift));
                for normal in [1, 2, 3, 5, 9, 10, 96] {
                    assert_eq!(now.get(&normal), before.get(&normal));
                }
            }
            assert_eq!(s.lock().unwrap().writes.len(), 9);
            for (id, _) in controls().into_iter().filter(|b| b.0 != 1) {
                d.apply(&Setting::Button {
                    id,
                    action: ButtonAction::Default,
                })
                .unwrap();
            }
            assert_eq!(d.read_state().unwrap().buttons, before);
        }
    }
    #[test]
    fn failed_secondary_write_restores_only_its_exact_layer_and_retains_foreign_changes() {
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
                let id = 258;
                let action = ButtonAction::Key {
                    usage: 6,
                    mods: Modifiers(3),
                };
                assert!(
                    d.apply(&Setting::Button {
                        id,
                        action: action.clone()
                    })
                    .is_err()
                );
                let s = s.lock().unwrap();
                let wanted = encode(id, &action).unwrap();
                assert_eq!(s.writes.iter().filter(|(_, b)| *b == wanted).count(), 1);
                if fault == Fault::Foreign {
                    assert_eq!(s.writes.len(), 1);
                    assert_eq!(s.blocks[&id], [2, 2, 0, 5, 0, 0, 0]);
                } else {
                    assert_eq!(s.writes.len(), 2);
                    assert_eq!(s.blocks[&id], default_block(id).unwrap());
                }
                for (other, _) in controls().into_iter().filter(|b| b.0 != id) {
                    assert_eq!(s.blocks[&other], default_block(other).unwrap());
                }
            }
        }
    }
    #[test]
    fn last_normal_left_click_is_preserved_even_when_primary_was_reassigned_before_open() {
        for pid in [0xb6, 0xb7] {
            let (mut d, s) = rig(pid, Fault::None);
            {
                let mut s = s.lock().unwrap();
                s.blocks.insert(1, [2, 2, 0, 4, 0, 0, 0]);
                s.blocks.insert(2, [1, 1, 1, 0, 0, 0, 0]);
            }
            let before = s.lock().unwrap().blocks.clone();
            assert!(
                d.apply(&Setting::Button {
                    id: 2,
                    action: ButtonAction::Disabled
                })
                .is_err()
            );
            assert_eq!(s.lock().unwrap().blocks, before);
            assert!(s.lock().unwrap().writes.is_empty());
            // A shifted click cannot replace the always-accessible normal click.
            d.apply(&Setting::Button {
                id: 259,
                action: ButtonAction::Mouse(MouseButton::Left),
            })
            .unwrap();
            assert!(
                d.apply(&Setting::Button {
                    id: 2,
                    action: ButtonAction::Disabled
                })
                .is_err()
            );
            // Move the normal primary-click function to another actual input first.
            d.apply(&Setting::Button {
                id: 3,
                action: ButtonAction::Mouse(MouseButton::Left),
            })
            .unwrap();
            d.apply(&Setting::Button {
                id: 2,
                action: ButtonAction::Disabled,
            })
            .unwrap();
            let now = d.read_state().unwrap();
            assert_eq!(
                now.buttons.get(&3),
                Some(&ButtonAction::Mouse(MouseButton::Left))
            );
            assert_eq!(now.buttons.get(&2), Some(&ButtonAction::Disabled));
        }
    }
    #[test]
    fn recovery_never_removes_the_last_normal_click_that_the_attempt_just_created() {
        for pid in [0xb6, 0xb7] {
            let (mut d, s) = rig(pid, Fault::SetterFailure);
            s.lock().unwrap().blocks.insert(1, [2, 2, 0, 4, 0, 0, 0]);
            assert!(
                d.apply(&Setting::Button {
                    id: 2,
                    action: ButtonAction::Mouse(MouseButton::Left)
                })
                .is_err()
            );
            let s = s.lock().unwrap();
            assert_eq!(s.writes.len(), 1);
            assert_eq!(s.blocks[&2], [1, 1, 1, 0, 0, 0, 0]);
        }
    }
}
