//! Vertical class11 is not class3 tilt or a macro. Shared fault harness only.
use super::*;
use gearwise_core::model::ScrollDirection;
fn vertical(id: u16, direction: ScrollDirection) -> Setting {
    Setting::Button {
        id,
        action: ButtonAction::Scroll(direction),
    }
}
#[test]
fn vertical_all_nine_transports_nonleft_keys_preserve_raw_eeprom_and_peer() {
    for m in models()
        .iter()
        .filter(|m| vertical_scroll_supported(m.vendor, m.product))
    {
        let (mut d, log) = memory_device(m.product).unwrap();
        let (mut peer, _) = memory_device(m.product).unwrap();
        let before_peer = peer.read_state().unwrap();
        for id in 2..=m.buttons {
            for direction in [ScrollDirection::Up, ScrollDirection::Down] {
                let before = d.read_blob(0, 768 + 12 * 384).unwrap();
                log.lock().unwrap().clear();
                d.apply(&vertical(id, direction)).unwrap();
                assert_eq!(
                    d.read_state().unwrap().buttons[&id],
                    ButtonAction::Scroll(direction)
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
            ButtonAction::Scroll(ScrollDirection::Down)
        );
        assert_eq!(peer.read_state().unwrap(), before_peer);
    }
}
#[test]
fn vertical_complete_golden_reports_and_noop_never_allocate_macro() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    log.lock().unwrap().clear();
    d.apply(&vertical(4, ScrollDirection::Up)).unwrap();
    assert_eq!(
        sets(&log),
        vec![vec![
            8, 7, 0, 0, 108, 4, 11, 1, 0, 73, 0, 0, 0, 0, 0, 0, 129
        ]]
    );
    log.lock().unwrap().clear();
    d.apply(&vertical(4, ScrollDirection::Up)).unwrap();
    assert!(sets(&log).is_empty());
    log.lock().unwrap().clear();
    d.apply(&vertical(4, ScrollDirection::Down)).unwrap();
    assert_eq!(
        sets(&log),
        vec![vec![
            8, 7, 0, 0, 108, 4, 11, 2, 0, 72, 0, 0, 0, 0, 0, 0, 129
        ]]
    );
    assert!(d.macro_definitions().is_empty());
}
#[test]
fn vertical_pulsar_left_unknown_id_and_unknown_raw_rows_are_protected() {
    for product in [0xf507, 0xf508, 0x1017] {
        let (mut d, log) = memory_device(product).unwrap();
        let ids = if product == 0x1017 {
            vec![0, 1, 7]
        } else {
            vec![2, 4]
        };
        for id in ids {
            log.lock().unwrap().clear();
            assert!(d.apply(&vertical(id, ScrollDirection::Up)).is_err());
            assert!(log.lock().unwrap().is_empty());
        }
    }
    for row in [
        [11, 0, 0, 74],
        [11, 3, 0, 71],
        [11, 1, 1, 72],
        [11, 1, 0, 0],
    ] {
        let (mut d, log) = memory_device(0x1017).unwrap();
        d.io.write(&frame(7, 108, 4, &row).unwrap()).unwrap();
        assert!(d.read_action(&row).is_none());
        log.lock().unwrap().clear();
        assert!(d.apply(&vertical(4, ScrollDirection::Up)).is_err());
        assert!(sets(&log).is_empty());
        assert_eq!(d.read(108, 4).unwrap(), row);
    }
}
#[test]
fn vertical_identity_change_prevents_set() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    d.io = Box::new(Fault {
        inner: d.io,
        mode: 0,
        wrote: false,
        used: false,
    });
    log.lock().unwrap().clear();
    assert!(d.apply(&vertical(4, ScrollDirection::Up)).is_err());
    assert!(sets(&log).is_empty());
}
#[test]
fn vertical_corrupt_readback_restores_only_target_row() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    let before = d.read(108, 4).unwrap();
    d.io = Box::new(Fault {
        inner: d.io,
        mode: 1,
        wrote: false,
        used: false,
    });
    log.lock().unwrap().clear();
    assert!(d.apply(&vertical(4, ScrollDirection::Down)).is_err());
    let writes = sets(&log);
    assert_eq!(writes.len(), 2);
    assert!(writes.iter().all(|p| p[3..6] == [0, 108, 4]));
    assert_eq!(&writes[1][6..10], &before);
    assert_eq!(d.read(108, 4).unwrap(), before);
}
#[test]
fn vertical_short_write_remains_uncertain_without_resend() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    d.io = Box::new(Fault {
        inner: d.io,
        mode: 2,
        wrote: false,
        used: false,
    });
    log.lock().unwrap().clear();
    assert!(d.apply(&vertical(4, ScrollDirection::Up)).is_err());
    assert_eq!(sets(&log).len(), 1);
}
