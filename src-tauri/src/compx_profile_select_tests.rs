use super::*;
use gearwise_core::transport::MockRecord;
fn select(id: u16, index: u8) -> Setting {
    Setting::Button {
        id,
        action: ButtonAction::ProfileSelect(index),
    }
}
fn sets(log: &Arc<Mutex<Vec<MockRecord>>>) -> Vec<Vec<u8>> {
    log.lock()
        .unwrap()
        .iter()
        .filter_map(|r| match r {
            MockRecord::Write(p) if p[1] == 7 => Some(p.clone()),
            _ => None,
        })
        .collect()
}
#[test]
fn four_direct_targets_roundtrip_all_nonleft_keys_without_other_eeprom_or_peer_changes() {
    for m in models()
        .iter()
        .filter(|m| profile_cycle_supported(m.vendor, m.product))
    {
        let (mut d, log) = memory_device(m.product).unwrap();
        let (mut peer, _) = memory_device(m.product).unwrap();
        let peer_before = peer.read_state().unwrap();
        for id in 2..=m.buttons {
            for index in 0..4 {
                let before = d.read_blob(0, 768 + 12 * 384).unwrap();
                log.lock().unwrap().clear();
                d.apply(&select(id, index)).unwrap();
                assert_eq!(
                    d.read_state().unwrap().buttons[&id],
                    ButtonAction::ProfileSelect(index)
                );
                let at = 96 + usize::from(id - 1) * 4;
                let after = d.read_blob(0, before.len()).unwrap();
                assert_eq!(&after[..at], &before[..at]);
                assert_eq!(&after[at + 4..], &before[at + 4..]);
                assert!(sets(&log)
                    .iter()
                    .all(|p| u16::from_be_bytes([p[3], p[4]]) == at as u16));
            }
        }
        assert_eq!(peer.read_state().unwrap(), peer_before);
        let info = HidInterfaceInfo {
            vendor_id: m.vendor,
            product_id: m.product,
            usage_page: 0xff02,
            usage: 2,
            ..Default::default()
        };
        let mut reopened = Atk::open(d.io, &info).unwrap();
        assert_eq!(
            reopened.read_state().unwrap().buttons[&m.buttons],
            ButtonAction::ProfileSelect(3)
        );
    }
}
#[test]
fn direct_selection_complete_golden_reports_do_not_execute_a_bank_switch() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    for (index, value, crc) in [(0, 161, 171), (1, 162, 170), (2, 163, 169), (3, 164, 168)] {
        log.lock().unwrap().clear();
        d.apply(&select(4, index)).unwrap();
        assert_eq!(
            sets(&log),
            vec![vec![
                8, 7, 0, 0, 108, 4, 9, value, 0, crc, 0, 0, 0, 0, 0, 0, 129
            ]]
        );
        assert!(!log
            .lock()
            .unwrap()
            .iter()
            .any(|r| matches!(r,MockRecord::Write(p)if p[1]==15)));
        assert_eq!(d.profile, Some(0));
        log.lock().unwrap().clear();
        d.apply(&select(4, index)).unwrap();
        assert!(sets(&log).is_empty());
    }
    assert!(d.macro_definitions().is_empty());
}
#[test]
fn invalid_direct_indices_and_unproved_models_or_keys_reject_before_io() {
    for product in [0x1017, 0xf507, 0xf508] {
        let (mut d, log) = memory_device(product).unwrap();
        let cases = if product == 0x1017 {
            vec![(4, 4), (4, 255), (1, 0), (7, 0)]
        } else {
            vec![(4, 0), (4, 3)]
        };
        for (id, index) in cases {
            log.lock().unwrap().clear();
            assert!(d.apply(&select(id, index)).is_err());
            assert!(log.lock().unwrap().is_empty());
        }
    }
}
#[test]
fn direct_targets_cycle_and_unknown_rows_keep_distinct_semantics() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    for (index, value, crc) in [(0, 161, 171), (1, 162, 170), (2, 163, 169), (3, 164, 168)] {
        assert_eq!(
            d.read_action(2,&[9, value, 0, crc]).unwrap().0,
            ButtonAction::ProfileSelect(index)
        );
    }
    assert_eq!(
        d.read_action(2,&[9, 255, 0, 77]).unwrap().0,
        ButtonAction::ProfileCycle
    );
    for row in [
        [9, 160, 0, 172],
        [9, 165, 0, 167],
        [9, 161, 1, 170],
        [9, 161, 0, 0],
    ] {
        d.io.write(&frame(7, 108, 4, &row).unwrap()).unwrap();
        log.lock().unwrap().clear();
        assert!(d.read_action(2,&row).is_none());
        assert!(d.apply(&select(4, 0)).is_err());
        assert!(sets(&log).is_empty());
    }
}
