//! Stateful synthetic firmware, not a USB capture or physical-hardware test.
use super::*;
use crate::{commands, device::RazerDevice, link::Link, report::Report};
use gearwise_core::{
    model::{DpiStage, DpiState},
    transport::{MockOp, MockRecord, MockReply, MockTransport},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
#[derive(Default)]
struct Firmware {
    board: u8,
    mode: u8,
    buttons: BTreeMap<(u8, u8), [u8; 7]>,
    fault: Option<(u8, u8)>,
    switched_on_write: bool,
}
fn create(
    pid: u16,
) -> Result<(
    Box<dyn Device>,
    Arc<Mutex<Vec<MockRecord>>>,
    Arc<Mutex<Firmware>>,
)> {
    if !crate::basilisk::supported(0x1532, pid) {
        return Err(Error::Unsupported("exact Basilisk simulation model"));
    }
    let firmware = Arc::new(Mutex::new(Firmware {
        board: 1,
        ..Default::default()
    }));
    let fw = firmware.clone();
    let mut dpi = commands::set_dpi_stages(&DpiState {
        stages: vec![DpiStage::uniform(800)],
        active: 0,
    })
    .args;
    let mut rate = 1;
    let mut brightness = BTreeMap::new();
    let io = MockTransport::new(move |op| {
        let MockOp::SetFeature(bytes) = op else {
            return MockReply::none();
        };
        let req = Report::from_wire(bytes).unwrap();
        let mut r = req.clone();
        r.status = 2;
        r.args = [0; 80];
        let mut fw = fw.lock().unwrap();
        match (req.class, req.command) {
            (0, 0x82) => r.args[..12].copy_from_slice(b"SIM-BASILISK"),
            (0, 0x81) => r.args[..2].copy_from_slice(&[2, 50]),
            (0, 0x84) => r.args[0] = fw.mode,
            (5, 0x84) => r.args[0] = fw.board,
            (2, 0x8c) => {
                r.args[..3].copy_from_slice(&req.args[..3]);
                let block = fw
                    .buttons
                    .get(&(req.args[0], req.args[1]))
                    .copied()
                    .or_else(|| crate::basilisk::default_block(u16::from(req.args[1])))
                    .unwrap_or([0xff; 7]);
                r.args[3..10].copy_from_slice(&block);
            }
            (2, 0x0c) => {
                fw.buttons.insert(
                    (req.args[0], req.args[1]),
                    req.args[3..10].try_into().unwrap(),
                );
                r.args = req.args;
                if fw.switched_on_write {
                    fw.board = 2;
                }
            }
            (4, 0x86) => r.args = dpi,
            (4, 6) => dpi = req.args,
            (0, 0x85) => r.args[0] = rate,
            (0, 5) => rate = req.args[0],
            (15, 2) => {}
            (15, 4) => {
                brightness.insert(req.args[1], req.args[2]);
            }
            (15, 0x84) => r.args[..3].copy_from_slice(&[
                1,
                req.args[1],
                *brightness.get(&req.args[1]).unwrap_or(&255),
            ]),
            (7, 0x80) => r.args[1] = 180,
            (7, 0x84) => {}
            _ => r.status = 5,
        }
        let mut wire = r.to_wire().to_vec();
        if fw.fault == Some((req.class, req.command)) {
            fw.fault = None;
            wire[89] ^= 1;
        }
        MockReply::feature(wire)
    });
    let log = io.log();
    let model = models::find(pid).unwrap();
    let link = Link::new(
        Box::new(io),
        model.transaction_id,
        Timing::new(Duration::ZERO, Duration::ZERO, 1),
    );
    let iface = HidInterfaceInfo {
        path: format!("simulation:basilisk:{pid:04x}"),
        vendor_id: 0x1532,
        product_id: pid,
        interface_number: 0,
        usage_page: 1,
        usage: 2,
        ..Default::default()
    };
    Ok((
        Box::new(RazerDevice::open(model, link, &iface)?),
        log,
        firmware,
    ))
}
pub fn mouse(pid: u16) -> Result<(Box<dyn Device>, Arc<Mutex<Vec<MockRecord>>>)> {
    let (d, l, _) = create(pid)?;
    Ok((d, l))
}
#[cfg(test)]
mod tests {
    use super::*;
    use gearwise_core::{
        model::{ButtonAction, Modifiers},
        Setting,
    };
    fn writes(log: &Arc<Mutex<Vec<MockRecord>>>) -> Vec<Vec<u8>> {
        log.lock()
            .unwrap()
            .iter()
            .filter_map(|r| {
                if let MockRecord::SetFeature(p) = r {
                    (p[8] & 0x80 == 0).then(|| p.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    #[test]
    fn published_ctrl_shift_t_vector_matches_all_91_bytes() {
        let (mut d, log, _) = create(0x00ab).unwrap();
        log.lock().unwrap().clear();
        d.apply(&Setting::Button {
            id: 96,
            action: ButtonAction::Key {
                usage: 0x17,
                mods: Modifiers(3),
            },
        })
        .unwrap();
        let p = writes(&log).pop().unwrap();
        let mut expected = vec![0; 91];
        expected[..19]
            .copy_from_slice(&[0, 0, 31, 0, 0, 0, 10, 2, 12, 1, 96, 0, 2, 2, 3, 23, 0, 0, 0]);
        expected[89] = 0x71;
        assert_eq!(p, expected);
    }
    #[test]
    fn both_transports_all_controls_and_all_modifier_masks_roundtrip() {
        for pid in [0x00aa, 0x00ab] {
            let (mut d, _, _) = create(pid).unwrap();
            assert!(d.descriptor().id.ends_with(":board:1"));
            assert_eq!(d.read_state().unwrap().buttons.len(), 13);
            for (id, _) in crate::basilisk::BUTTONS
                .into_iter()
                .filter(|(id, _)| *id != 1)
            {
                for mods in [0, 3, 15, 0xff] {
                    let action = ButtonAction::Key {
                        usage: 0x17,
                        mods: Modifiers(mods),
                    };
                    d.apply(&Setting::Button {
                        id,
                        action: action.clone(),
                    })
                    .unwrap();
                    assert_eq!(d.read_state().unwrap().buttons[&id], action);
                }
                d.apply(&Setting::Button {
                    id,
                    action: ButtonAction::Default,
                })
                .unwrap();
                assert_eq!(
                    d.read_state().unwrap().buttons[&id],
                    crate::basilisk::decode(id, &crate::basilisk::default_block(id).unwrap())
                        .unwrap()
                );
            }
        }
    }
    #[test]
    fn unknown_native_macro_or_foreign_function_is_locked_and_retained() {
        for block in [
            [3, 3, 0, 1, 1, 0, 0],
            [2, 2, 3, 23, 0, 0, 7],
            [6, 5, 5, 1, 144, 1, 144],
        ] {
            let (mut d, log, fw) = create(0x00ab).unwrap();
            fw.lock().unwrap().buttons.insert((1, 4), block);
            assert!(!d.read_state().unwrap().buttons.contains_key(&4));
            assert!(
                !d.capabilities()
                    .buttons
                    .iter()
                    .find(|b| b.id == 4)
                    .unwrap()
                    .remappable
            );
            log.lock().unwrap().clear();
            assert!(d
                .apply(&Setting::Button {
                    id: 4,
                    action: ButtonAction::Disabled
                })
                .is_err());
            assert!(writes(&log).is_empty());
            assert_eq!(fw.lock().unwrap().buttons[&(1, 4)], block);
        }
    }
    #[test]
    fn changed_bank_driver_mode_left_and_other_ids_never_receive_writes() {
        let (mut d, log, fw) = create(0x00ab).unwrap();
        for (board, mode, id) in [(2, 0, 4), (1, 3, 4), (1, 0, 1), (1, 0, 64)] {
            fw.lock().unwrap().board = board;
            fw.lock().unwrap().mode = mode;
            log.lock().unwrap().clear();
            assert!(d
                .apply(&Setting::Button {
                    id,
                    action: ButtonAction::Disabled
                })
                .is_err());
            assert!(writes(&log).is_empty());
        }
        fw.lock().unwrap().board = 0;
        assert!(d.read_state().is_err());
        assert!(mouse(0x0099).is_err());
    }
    #[test]
    fn uncertain_write_restores_exact_original_without_repeating_write() {
        let (mut d, log, fw) = create(0x00ab).unwrap();
        fw.lock().unwrap().fault = Some((2, 12));
        log.lock().unwrap().clear();
        assert!(d
            .apply(&Setting::Button {
                id: 15,
                action: ButtonAction::Key {
                    usage: 4,
                    mods: Modifiers(1)
                }
            })
            .is_err());
        let p = writes(&log);
        assert_eq!(p.len(), 2);
        assert_eq!(&p[1][12..19], &crate::basilisk::default_block(15).unwrap());
        assert_eq!(
            fw.lock().unwrap().buttons[&(1, 15)],
            crate::basilisk::default_block(15).unwrap()
        );
    }
    #[test]
    fn switch_during_write_never_restores_to_different_bank() {
        let (mut d, log, fw) = create(0x00ab).unwrap();
        fw.lock().unwrap().switched_on_write = true;
        log.lock().unwrap().clear();
        assert!(d
            .apply(&Setting::Button {
                id: 4,
                action: ButtonAction::Disabled
            })
            .is_err());
        let p = writes(&log);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0][9], 1);
        assert!(!fw
            .lock()
            .unwrap()
            .buttons
            .keys()
            .any(|(board, _)| *board != 1));
        assert!(d.read_state().is_err());
    }
}
