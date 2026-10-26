//! Exact B6/B7 normal/Hypershift layers. Eight firmware IDs are in the official 182 UI.
//! Official RzCtl_00B6 uses 02/8C and 02/0C with ten-byte assignments.
//! Layer selectors follow the official record; momentary switch facts are documented
//! by razerqdhid 4363a883 (MIT). Host macros and unknown originals stay locked.
//! Scroll and reverse-cycle outputs combine exact OEM logical support with
//! reconstructed scalar records from that protocol reference; no hardware capture.
use crate::{basilisk, link::Link, report::Report};
use gearwise_core::{
    Error, Result,
    model::{ButtonAction, DpiFunction, MouseButton, ScrollDirection},
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
    if wire(id).is_none() {
        return Err(Error::Unsupported(
            "DeathAdder unknown physical input",
        ));
    }
    let (physical, _) = wire(id).unwrap();
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
        ButtonAction::Hypershift if matches!(physical, 1..=5) => {
            Ok([12, 1, 1, 0, 0, 0, 0])
        }
        ButtonAction::Scroll(direction) => Ok([
            1,
            1,
            match direction {
                ScrollDirection::Up => 9,
                ScrollDirection::Down => 10,
            },
            0,
            0,
            0,
            0,
        ]),
        ButtonAction::Media(usage) if MEDIA.contains(usage) => {
            let [hi, lo] = usage.to_be_bytes();
            Ok([10, 2, hi, lo, 0, 0, 0])
        }
        ButtonAction::Dpi(DpiFunction::Up) => Ok([6, 1, 1, 0, 0, 0, 0]),
        ButtonAction::Dpi(DpiFunction::Down) => Ok([6, 1, 2, 0, 0, 0, 0]),
        ButtonAction::Dpi(DpiFunction::CycleDown) => Ok([6, 1, 7, 0, 0, 0, 0]),
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
    let (physical, _) = wire(id)?;
    if raw == [12, 1, 1, 0, 0, 0, 0] {
        return matches!(physical, 1..=5).then_some(ButtonAction::Hypershift);
    }
    if raw[..2] == [1, 1] && raw[3..] == [0; 4] {
        match raw[2] {
            9 => return Some(ButtonAction::Scroll(ScrollDirection::Up)),
            10 => return Some(ButtonAction::Scroll(ScrollDirection::Down)),
            _ => {}
        }
    }
    if raw[..2] == [10, 2] && raw[4..] == [0, 0, 0] {
        let usage = u16::from_be_bytes([raw[2], raw[3]]);
        return MEDIA.contains(&usage).then_some(ButtonAction::Media(usage));
    }
    if !matches!(physical, 9 | 10) && raw[..2] == [6, 1] && raw[3..] == [0; 4] {
        match raw[2] {
            1 => return Some(ButtonAction::Dpi(DpiFunction::Up)),
            2 => return Some(ButtonAction::Dpi(DpiFunction::Down)),
            7 => return Some(ButtonAction::Dpi(DpiFunction::CycleDown)),
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
fn keep_normal_left(link: &mut Link, id: u16, from: &[u8; 10], to: &[u8; 10]) -> Result<Option<(u16, [u8; 10])>> {
    const LEFT: [u8; 7] = [1, 1, 1, 0, 0, 0, 0];
    // Reassigning primary also requires a witness when it was already non-left
    // before this session. A secondary-layer click is never an ordinary witness.
    if from[2] != 0 || to[3..] == LEFT || (id != 1 && from[3..] != LEFT) {
        return Ok(None);
    }
    for (other, _) in BUTTONS {
        if other != id {
            if let Ok(raw) = get(link, other) {
                if raw[3..] == LEFT {
                    return Ok(Some((other, raw)));
                }
            }
        }
    }
    Err(Error::invalid(
        "retain at least one verified normal-layer left click before replacing this binding",
    ))
}
fn verify_owned_before_write(link: &mut Link, id: u16, from: &[u8; 10], to: &[u8; 10], parent: Option<(u16, [u8; 10])>) -> Result<()> {
    mode(link)?;
    if let Some((witness, expected)) = keep_normal_left(link, id, from, to)? {
        if get(link, witness)? != expected {
            return Err(Error::protocol("DeathAdder normal left-click witness changed; read again"));
        }
    }
    verify_parent(link, parent)?;
    if get(link, id)? != *from {
        return Err(Error::protocol("foreign DeathAdder binding retained before write"));
    }
    Ok(())
}
fn verify_parent(link: &mut Link, parent: Option<(u16, [u8; 10])>) -> Result<()> {
    if let Some((id, expected)) = parent {
        if get(link, id)? != expected { return Err(Error::protocol("foreign DeathAdder normal-layer context retained")); }
    }
    Ok(())
}
fn desired(before: &[u8; 10], block: &[u8; 7]) -> [u8; 10] {
    let mut want = *before;
    want[3..].copy_from_slice(block);
    want
}
fn recover_owned(link: &mut Link, id: u16, before: &[u8; 10], want: &[u8; 10], parent: Option<(u16, [u8; 10])>) -> Result<()> {
    mode(link)?;
    let actual = get(link, id)?;
    if actual == *before { return Ok(()); }
    if actual != *want { return Err(Error::protocol("foreign DeathAdder binding retained")); }
    verify_owned_before_write(link, id, &actual, before, parent)?;
    link.exec(Report::new(2, 0x0c, 10, before))?;
    if get(link, id)? != *before {
        return Err(Error::protocol("DeathAdder original assignment restore mismatch"));
    }
    mode(link)
}
fn apply_record(link: &mut Link, id: u16, before: &[u8; 10], want: &[u8; 10], parent: Option<(u16, [u8; 10])>) -> Result<()> {
    if want == before {
        if parent.is_some() { verify_parent(link, parent)?; if get(link, id)? != *before { return Err(Error::protocol("foreign DeathAdder paired binding retained")); } }
        return Ok(());
    }
    verify_owned_before_write(link, id, before, want, parent)?;
    let result = link.exec(Report::new(2, 0x0c, 10, want))
        .and_then(|_| get(link, id))
        .and_then(|raw| if raw == *want { Ok(()) } else { Err(Error::protocol("DeathAdder button readback mismatch")) })
        .and_then(|_| verify_parent(link, parent))
        .and_then(|_| mode(link));
    if let Err(error) = result {
        let restored = recover_owned(link, id, before, want, parent);
        return Err(Error::protocol(format!("{error}; original binding recovery: {restored:?}; uncertain writes not retried")));
    }
    Ok(())
}
pub fn apply(link: &mut Link, id: u16, action: &ButtonAction) -> Result<()> {
    let block = encode(id, action)?;
    mode(link)?;
    let before = get(link, id)?;
    let original = decode(id, &before[3..]).ok_or(Error::Unsupported("unknown original DeathAdder binding retained"))?;
    let want = desired(&before, &block);
    let (physical, layer) = wire(id).unwrap();
    if layer == 1 {
        // A paired OEM Hypershift record may be preserved, but it is not an
        // independent secondary action. Release must still leave the held layer.
        if want == before { return Ok(()); }
        let normal = get(link, u16::from(physical))?;
        let normal_action = decode(u16::from(physical), &normal[3..]).ok_or(Error::Unsupported("unknown DeathAdder normal binding retained with its secondary input"))?;
        if matches!(normal_action, ButtonAction::Hypershift) || matches!(action, ButtonAction::Hypershift) {
            return Err(Error::Unsupported("DeathAdder Hypershift actuator owns its secondary binding; change its normal action first"));
        }
        return apply_record(link, id, &before, &want, Some((u16::from(physical), normal)));
    }
    let entering = matches!(action, ButtonAction::Hypershift);
    let leaving = matches!(original, ButtonAction::Hypershift) && !entering;
    if !entering && !leaving { return apply_record(link, id, &before, &want, None); }
    let secondary = id | 0x100;
    let paired_before = get(link, secondary)?;
    let paired_want = if entering {
        // Unknown original second-layer actions are never erased as a side effect.
        if decode(secondary, &paired_before[3..]).is_none() {
            return Err(Error::Unsupported("unknown original DeathAdder secondary binding retained before Hypershift"));
        }
        desired(&paired_before, &block)
    } else if paired_before[3..] == [12, 1, 1, 0, 0, 0, 0] {
        // OEM removes this override on leaving Hypershift: use physical fallback,
        // rather than resurrecting an old pre-Hypershift assignment.
        desired(&paired_before, &default_block(secondary).unwrap())
    } else { paired_before };
    apply_record(link, id, &before, &want, None)?;
    let second = apply_record(link, secondary, &paired_before, &paired_want, Some((id, want)));
    let second_completed = second.is_ok();
    let result = second.and_then(|_| {
        mode(link)?;
        if get(link, id)? != want || get(link, secondary)? != paired_want {
            return Err(Error::protocol("DeathAdder paired binding final readback mismatch"));
        }
        Ok(())
    });
    if let Err(error) = result {
        let secondary_restored = if second_completed {Some(recover_owned(link, secondary, &paired_before, &paired_want, Some((id, want))))} else {None};
        let restored = recover_owned(link, id, &before, &want, None);
        return Err(Error::protocol(format!("{error}; paired secondary recovery: {secondary_restored:?}; paired normal binding recovery: {restored:?}; uncertain writes not retried")));
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
        for id in BUTTONS.map(|b| b.0) {
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
                    DpiFunction::CycleDown,
                    DpiFunction::Shift(30000),
                ] {
                    let a = ButtonAction::Dpi(function);
                    assert_eq!(decode(id, &encode(id, &a).unwrap()), Some(a));
                }
            }
        }
        for id in [0, 6, 7, 8, 14, 106, 512, 65535] {
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
        ForeignBeforeWrite,
        WitnessChanged,
        PairedFailure,
        PairedForeign,
        PairedUnrestored,
        UnknownSecondary,
        UnknownNormal,
        ParentChangedBeforeSecondary,
        ParentChangedAfterSecondary,
    }
    struct State {
        blocks: BTreeMap<u16, [u8; 7]>,
        writes: Vec<(u16, [u8; 7])>,
        gets: usize,
        bad_readback_sent: bool,
    }
    fn rig(pid: u16, fault: Fault) -> (RazerDevice, Arc<Mutex<State>>) {
        let state = Arc::new(Mutex::new(State {
            blocks: controls()
                .into_iter()
                .map(|(id, _)| (id, default_block(id).unwrap()))
                .collect(),
            writes: vec![],
            gets: 0,
            bad_readback_sent: false,
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
                    if fault == Fault::ForeignBeforeWrite && s.gets == 2 && s.writes.is_empty() {
                        s.blocks.insert(id, [2, 2, 0, 5, 0, 0, 0]);
                    }
                    if fault == Fault::WitnessChanged && id == 2 && s.gets == 3 {
                        s.blocks.insert(id, default_block(id).unwrap());
                    }
                    if fault == Fault::ParentChangedBeforeSecondary && id == 2 && s.gets == 3 {
                        s.blocks.insert(2, [12,1,1,0,0,0,0]);s.blocks.insert(258, [12,1,1,0,0,0,0]);
                    }
                    r.args[..3].copy_from_slice(&req.args[..3]);
                    r.args[3..10].copy_from_slice(&s.blocks[&id]);
                    if (fault == Fault::UnknownOriginal && matches!(id, 2 | 258)) || (fault == Fault::UnknownSecondary && id == 260) || (fault == Fault::UnknownNormal && id == 2) {
                        r.args[3..10].copy_from_slice(&[3, 3, 0, 1, 1, 0, 0]);
                    }
                    corrupt = fault == Fault::BadReadback && s.writes.len() == 1 && !s.bad_readback_sent;
                    s.bad_readback_sent |= corrupt;
                }
                (2, 0x0c) => {
                    let id = u16::from(req.args[1]) | (u16::from(req.args[2]) << 8);
                    assert_eq!(req.args[0], 1);
                    assert!(req.args[2] <= 1);
                    let mut block = [0; 7];
                    block.copy_from_slice(&req.args[3..10]);
                    let mut s = shared.lock().unwrap();
                    if !(fault == Fault::PairedUnrestored && s.writes.len() == 2) {s.blocks.insert(id, block);}
                    s.writes.push((id, block));
                    if fault == Fault::ParentChangedAfterSecondary && id == 260 { s.blocks.insert(4, [2,2,0,5,0,0,0]); }
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
                    } else if s.writes.len() == 2 && matches!(fault, Fault::PairedFailure | Fault::PairedForeign | Fault::PairedUnrestored) {
                        r.status = 3;
                        if fault == Fault::PairedForeign { s.blocks.insert(id, [2, 2, 0, 5, 0, 0, 0]); }
                    } else if fault == Fault::PairedUnrestored && s.writes.len() == 3 {
                        r.status = 3;
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
                16
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
            for id in [6, 512, 65535] {
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
        for id in [9, 10, 96, 265, 266, 352] {
            assert!(encode(id, &ButtonAction::Hypershift).is_err());
        }
        for id in [9, 10, 265, 266] {
            assert!(encode(id, &ButtonAction::Dpi(DpiFunction::Up)).is_err());
        }
        assert_eq!(decode(258, &[12, 1, 1, 0, 0, 0, 0]), Some(ButtonAction::Hypershift));
        for id in [9, 10, 265, 266] {
            assert!(decode(id, &[6, 1, 6, 0, 0, 0, 0]).is_none());
        }
    }
    #[test]
    fn secondary_bindings_are_independent_except_the_paired_hypershift_actuator() {
        for pid in [0xb6, 0xb7] {
            let (mut d, s) = rig(pid, Fault::None);
            let before = d.read_state().unwrap().buttons;
            d.apply(&Setting::Button {
                id: 4,
                action: ButtonAction::Hypershift,
            })
            .unwrap();
            for (id, _) in controls().into_iter().filter(|b| b.0 >= 256 && b.0 != 260) {
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
            assert!(d.apply(&Setting::Button { id: 260, action: ButtonAction::Disabled }).is_err());
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
    #[test]
    fn primary_remap_requires_a_fresh_normal_left_and_can_be_restored() {
        for pid in [0xb6, 0xb7] {
            let (mut d, state) = rig(pid, Fault::None);
            let key = ButtonAction::Key { usage: 6, mods: Modifiers(0x81) };
            assert!(d.apply(&Setting::Button { id: 1, action: key.clone() }).is_err());
            assert!(state.lock().unwrap().writes.is_empty());
            d.apply(&Setting::Button { id: 257, action: ButtonAction::Mouse(MouseButton::Left) }).unwrap();
            assert!(d.apply(&Setting::Button { id: 1, action: key.clone() }).is_err());
            d.apply(&Setting::Button { id: 2, action: ButtonAction::Mouse(MouseButton::Left) }).unwrap();
            d.apply(&Setting::Button { id: 1, action: key.clone() }).unwrap();
            assert_eq!(d.read_state().unwrap().buttons.get(&1), Some(&key));
            assert!(d.apply(&Setting::Button { id: 2, action: ButtonAction::Default }).is_err());
            d.apply(&Setting::Button { id: 1, action: ButtonAction::Default }).unwrap();
            d.apply(&Setting::Button { id: 2, action: ButtonAction::Default }).unwrap();
            assert_eq!(state.lock().unwrap().blocks[&1], default_block(1).unwrap());
            // Starting without any normal click is not a license to change primary again.
            state.lock().unwrap().blocks.insert(1, encode(1, &key).unwrap());
            assert!(d.apply(&Setting::Button { id: 1, action: ButtonAction::Disabled }).is_err());
            d.apply(&Setting::Button { id: 1, action: ButtonAction::Default }).unwrap();
        }
    }
    #[test]
    fn changed_target_or_click_witness_is_retained_without_any_setter() {
        for pid in [0xb6, 0xb7] {
            let (mut d, state) = rig(pid, Fault::ForeignBeforeWrite);
            assert!(d.apply(&Setting::Button { id: 2, action: ButtonAction::Disabled }).is_err());
            let state = state.lock().unwrap();
            assert!(state.writes.is_empty());
            assert_eq!(state.blocks[&2], [2, 2, 0, 5, 0, 0, 0]);
            drop(state);
            let (mut d, state) = rig(pid, Fault::WitnessChanged);
            state.lock().unwrap().blocks.insert(2, [1, 1, 1, 0, 0, 0, 0]);
            assert!(d.apply(&Setting::Button { id: 1, action: ButtonAction::Disabled }).is_err());
            let state = state.lock().unwrap();
            assert!(state.writes.is_empty());
            assert_eq!(state.blocks[&1], default_block(1).unwrap());
        }
    }

    #[test]
    fn hypershift_pairs_both_layers_and_restores_owned_pair_on_second_failure() {
        for pid in [0xb6, 0xb7] {
            let (mut d, state) = rig(pid, Fault::None);
            d.apply(&Setting::Button { id: 260, action: ButtonAction::Media(0xcd) }).unwrap();
            d.apply(&Setting::Button { id: 4, action: ButtonAction::Hypershift }).unwrap();
            let now = d.read_state().unwrap();
            assert_eq!(now.buttons.get(&4), Some(&ButtonAction::Hypershift));
            assert_eq!(now.buttons.get(&260), Some(&ButtonAction::Hypershift));
            assert!(d.apply(&Setting::Button { id: 260, action: ButtonAction::Disabled }).is_err());
            d.apply(&Setting::Button { id: 4, action: ButtonAction::Default }).unwrap();
            assert_eq!(state.lock().unwrap().blocks[&260], default_block(260).unwrap());
            for fault in [Fault::PairedFailure, Fault::PairedForeign, Fault::PairedUnrestored] {
                let (mut d, state) = rig(pid, fault);
                assert!(d.apply(&Setting::Button { id: 4, action: ButtonAction::Hypershift }).is_err());
                let state=state.lock().unwrap();
                assert_eq!(state.blocks[&4], default_block(4).unwrap());
                if fault == Fault::PairedForeign { assert_eq!(state.blocks[&260], [2,2,0,5,0,0,0]); }
                else if fault == Fault::PairedUnrestored {assert_eq!(state.blocks[&260], [12,1,1,0,0,0,0]);assert_eq!(state.writes.iter().filter(|(id,_)|*id==260).count(),2);}
                else { assert_eq!(state.blocks[&260], default_block(260).unwrap()); }
                assert_eq!(state.writes.iter().filter(|(id,block)|*id==4&&block[0]==12).count(),1);
                assert_eq!(state.writes.iter().filter(|(id,block)|*id==260&&block[0]==12).count(),1);
            }
            let (mut d, state) = rig(pid, Fault::UnknownSecondary);
            assert!(d.apply(&Setting::Button { id: 4, action: ButtonAction::Hypershift }).is_err());
            assert!(state.lock().unwrap().writes.is_empty());
        }
    }

    #[test]
    fn foreign_normal_context_never_reports_a_successful_hypershift_pair() {
        for pid in [0xb6, 0xb7] {
            let (mut d, state) = rig(pid, Fault::ParentChangedBeforeSecondary);
            assert!(d.apply(&Setting::Button { id: 258, action: ButtonAction::Disabled }).is_err());
            let state=state.lock().unwrap();assert!(state.writes.is_empty());assert_eq!(state.blocks[&258], [12,1,1,0,0,0,0]);drop(state);
            let (mut d, state) = rig(pid, Fault::ParentChangedAfterSecondary);
            assert!(d.apply(&Setting::Button { id: 4, action: ButtonAction::Hypershift }).is_err());
            let state=state.lock().unwrap();assert_eq!(state.blocks[&4],[2,2,0,5,0,0,0]);assert_eq!(state.writes.iter().filter(|(id,_)|*id==4).count(),1);
        }
    }

    #[test]
    fn unknown_parent_binding_locks_secondary_edits_but_preserves_known_noop() {
        for pid in [0xb6,0xb7] {
            let (mut d,state)=rig(pid,Fault::UnknownNormal);
            d.apply(&Setting::Button{id:258,action:ButtonAction::Default}).unwrap();
            assert!(d.apply(&Setting::Button{id:258,action:ButtonAction::Disabled}).is_err());
            assert!(state.lock().unwrap().writes.is_empty());
            let (mut d,state)=rig(pid,Fault::UnknownSecondary);
            state.lock().unwrap().blocks.insert(4,[12,1,1,0,0,0,0]);
            d.apply(&Setting::Button{id:4,action:ButtonAction::Default}).unwrap();
            let state=state.lock().unwrap();assert_eq!(state.writes.len(),1);assert_eq!(state.writes[0].0,4);
        }
    }

    #[test]
    fn scroll_and_reverse_cycle_match_reconstructed_scalar_vectors_with_exact_layer_identity() {
        // Exact OEM logical outputs plus scalar protocol facts, not captured HID traffic.
        for (id, action, args, crc) in [
            (4, ButtonAction::Scroll(ScrollDirection::Up), [1, 4, 0, 1, 1, 9, 0, 0, 0, 0], 0x08),
            (260, ButtonAction::Scroll(ScrollDirection::Down), [1, 4, 1, 1, 1, 10, 0, 0, 0, 0], 0x0a),
            (2, ButtonAction::Dpi(DpiFunction::CycleDown), [1, 2, 0, 6, 1, 7, 0, 0, 0, 0], 0x07),
            (258, ButtonAction::Dpi(DpiFunction::CycleDown), [1, 2, 1, 6, 1, 7, 0, 0, 0, 0], 0x06),
        ] {
            assert_eq!(encode(id, &action).unwrap(), args[3..]);
            assert_eq!(decode(id, &args[3..]), Some(action));
            let mut expected = [0; 91];
            expected[..9].copy_from_slice(&[0, 0, 31, 0, 0, 0, 10, 2, 12]);
            expected[9..19].copy_from_slice(&args);
            expected[89] = crc;
            let mut report = Report::new(2, 12, 10, &args);
            report.transaction_id = 31;
            assert_eq!(report.to_wire(), expected);
            let mut unknown = args;
            unknown[9] = 1;
            assert!(decode(id, &unknown[3..]).is_none());
        }
        for id in [9, 10, 265, 266] {
            for function in [DpiFunction::Up, DpiFunction::Down, DpiFunction::Cycle, DpiFunction::CycleDown, DpiFunction::Shift(400)] {
                assert!(encode(id, &ButtonAction::Dpi(function)).is_err());
            }
            assert!(decode(id, &[6, 1, 7, 0, 0, 0, 0]).is_none());
        }
        // Sharing scalar types must not enable these assignments on another driver.
        for action in [ButtonAction::Scroll(ScrollDirection::Up), ButtonAction::Scroll(ScrollDirection::Down), ButtonAction::Dpi(DpiFunction::CycleDown)] {
            assert!(basilisk::encode(2, &action).is_err());
        }
        assert!(basilisk::decode(2, &[6, 1, 7, 0, 0, 0, 0]).is_none());
    }

    #[test]
    fn scroll_and_reverse_cycle_roundtrip_exact_controls_without_changing_other_records() {
        for pid in [0xb6, 0xb7] {
            for (id, _) in controls() {
                for action in [ButtonAction::Scroll(ScrollDirection::Up), ButtonAction::Scroll(ScrollDirection::Down), ButtonAction::Dpi(DpiFunction::CycleDown)] {
                    if matches!(id & 255, 9 | 10) && matches!(action, ButtonAction::Dpi(_)) {
                        continue;
                    }
                    let (mut d, state) = rig(pid, Fault::None);
                    if id == 1 {
                        state.lock().unwrap().blocks.insert(2, [1, 1, 1, 0, 0, 0, 0]);
                    }
                    let before = state.lock().unwrap().blocks.clone();
                    let wanted = encode(id, &action).unwrap();
                    d.apply(&Setting::Button { id, action: action.clone() }).unwrap();
                    assert_eq!(d.read_state().unwrap().buttons.get(&id), Some(&action));
                    {
                        let state = state.lock().unwrap();
                        assert_eq!(state.blocks[&id], wanted);
                        assert_eq!(state.writes.len(), usize::from(before[&id] != wanted));
                        for (other, block) in &before {
                            if *other != id { assert_eq!(state.blocks[other], *block); }
                        }
                    }
                    d.apply(&Setting::Button { id, action: ButtonAction::Default }).unwrap();
                    assert_eq!(state.lock().unwrap().blocks, before);
                }
            }
        }
    }

    #[test]
    fn new_scalar_actions_keep_mode_original_parent_click_and_hypershift_guards() {
        for pid in [0xb6, 0xb7] {
            for action in [ButtonAction::Scroll(ScrollDirection::Up), ButtonAction::Scroll(ScrollDirection::Down), ButtonAction::Dpi(DpiFunction::CycleDown)] {
                for (id, fault) in [(2, Fault::UnknownOriginal), (258, Fault::UnknownOriginal), (2, Fault::DriverMode), (258, Fault::DriverMode), (258, Fault::UnknownNormal), (1, Fault::None)] {
                    let (mut d, state) = rig(pid, fault);
                    assert!(d.apply(&Setting::Button { id, action: action.clone() }).is_err());
                    assert!(state.lock().unwrap().writes.is_empty());
                }
                let (mut d, state) = rig(pid, Fault::None);
                {
                    let mut state = state.lock().unwrap();
                    state.blocks.insert(4, [12, 1, 1, 0, 0, 0, 0]);
                    state.blocks.insert(260, [12, 1, 1, 0, 0, 0, 0]);
                }
                assert!(d.apply(&Setting::Button { id: 260, action: action.clone() }).is_err());
                assert!(state.lock().unwrap().writes.is_empty());
                d.apply(&Setting::Button { id: 4, action: action.clone() }).unwrap();
                let state = state.lock().unwrap();
                assert_eq!(state.blocks[&4], encode(4, &action).unwrap());
                assert_eq!(state.blocks[&260], default_block(260).unwrap());
                assert_eq!(state.writes.len(), 2);
            }
        }
    }

    #[test]
    fn failed_new_scalar_writes_restore_exact_new_originals_once_and_preserve_foreign_records() {
        for pid in [0xb6, 0xb7] {
            for id in [2, 258] {
                for (action, original) in [
                    (ButtonAction::Scroll(ScrollDirection::Up), [6, 1, 7, 0, 0, 0, 0]),
                    (ButtonAction::Scroll(ScrollDirection::Down), [1, 1, 9, 0, 0, 0, 0]),
                    (ButtonAction::Dpi(DpiFunction::CycleDown), [1, 1, 10, 0, 0, 0, 0]),
                ] {
                    for fault in [Fault::SetterFailure, Fault::Busy, Fault::BadReadback, Fault::Foreign, Fault::RestoreFailure, Fault::OtherLayer] {
                        let (mut d, state) = rig(pid, fault);
                        state.lock().unwrap().blocks.insert(id, original);
                        let before = state.lock().unwrap().blocks.clone();
                        let error = d.apply(&Setting::Button { id, action: action.clone() }).unwrap_err().to_string();
                        assert!(error.contains("not retried"));
                        let state = state.lock().unwrap();
                        let wanted = encode(id, &action).unwrap();
                        assert_eq!(state.writes.iter().filter(|(input, block)| *input == id && *block == wanted).count(), 1);
                        if fault == Fault::Foreign {
                            assert_eq!(state.writes.len(), 1);
                            assert_eq!(state.blocks[&id], [2, 2, 0, 5, 0, 0, 0]);
                        } else {
                            assert_eq!(state.writes.len(), 2);
                            assert_eq!(state.writes[1], (id, original));
                            assert_eq!(state.blocks[&id], original);
                        }
                        for (other, block) in &before {
                            if *other != id { assert_eq!(state.blocks[other], *block); }
                        }
                    }
                }
            }
        }
    }

}
