//! Source-derived vectors over the production COMPX device, no physical HID.
use super::*;
use gearwise_core::transport::MockRecord;
fn setting(id: u16, b: MouseButton) -> Setting {
    Setting::Button {
        id,
        action: ButtonAction::Mouse(b),
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
fn nine_transports_all_nonleft_buttons_roundtrip_without_other_eeprom_or_peer_changes() {
    let eligible = models()
        .iter()
        .filter(|m| horizontal_scroll_supported(m.vendor, m.product))
        .collect::<Vec<_>>();
    assert_eq!(eligible.len(), 9);
    for m in eligible {
        let (mut d, log) = memory_device(m.product).unwrap();
        let (mut peer, _) = memory_device(m.product).unwrap();
        let peer_before = peer.read_state().unwrap();
        for id in 2..=m.buttons {
            for direction in [MouseButton::WheelLeft, MouseButton::WheelRight] {
                let before = d.read_blob(0, 768 + 12 * 384).unwrap();
                log.lock().unwrap().clear();
                d.apply(&setting(id, direction)).unwrap();
                assert_eq!(
                    d.read_state().unwrap().buttons[&id],
                    ButtonAction::Mouse(direction)
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
            ButtonAction::Mouse(MouseButton::WheelRight)
        );
    }
}
#[test]
fn horizontal_binding_has_independent_four_byte_and_complete_report_vectors() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    log.lock().unwrap().clear();
    d.apply(&setting(4, MouseButton::WheelLeft)).unwrap();
    assert_eq!(
        sets(&log),
        vec![vec![8, 7, 0, 0, 108, 4, 3, 1, 0, 81, 0, 0, 0, 0, 0, 0, 129]]
    );
    assert!(log.lock().unwrap().iter().any(
        |r| matches!(r,MockRecord::Write(p)if *p==vec![8,8,0,0,108,4,0,0,0,0,0,0,0,0,0,0,213])
    ));
    log.lock().unwrap().clear();
    d.apply(&setting(4, MouseButton::WheelLeft)).unwrap();
    assert!(sets(&log).is_empty());
    log.lock().unwrap().clear();
    d.apply(&setting(4, MouseButton::WheelRight)).unwrap();
    assert_eq!(
        sets(&log),
        vec![vec![8, 7, 0, 0, 108, 4, 3, 2, 0, 80, 0, 0, 0, 0, 0, 0, 129]]
    );
}
#[test]
fn pulsar_and_protected_or_nonexistent_buttons_never_get_horizontal_writes() {
    for product in [0xf507, 0xf508, 0x1017] {
        let (mut d, log) = memory_device(product).unwrap();
        let ids = if product == 0x1017 {
            vec![0, 1, 7, 255]
        } else {
            vec![2, 4]
        };
        for id in ids {
            log.lock().unwrap().clear();
            assert!(d.apply(&setting(id, MouseButton::WheelLeft)).is_err());
            assert!(log.lock().unwrap().is_empty());
        }
    }
    assert!(!horizontal_scroll_supported(0x373b, 0xffff));
    assert!(!horizontal_scroll_supported(0x1532, 0x00b6));
}
#[test]
fn unknown_original_horizontal_records_remain_unreadable_and_unwritten() {
    for row in [[3, 3, 0, 79], [3, 1, 1, 80], [3, 1, 0, 0]] {
        let (mut d, log) = memory_device(0x1017).unwrap();
        d.io.write(&frame(7, 108, 4, &row).unwrap()).unwrap();
        assert!(d.read_action(&row).is_none());
        log.lock().unwrap().clear();
        assert!(d.apply(&setting(4, MouseButton::WheelLeft)).is_err());
        assert!(sets(&log).is_empty());
        assert_eq!(d.read(108, 4).unwrap(), row);
    }
}
struct Fault {
    inner: Box<dyn HidTransport>,
    mode: u8,
    wrote: bool,
    used: bool,
}
impl HidTransport for Fault {
    fn write(&mut self, p: &[u8]) -> Result<usize> {
        let n = self.inner.write(p)?;
        if p.len() == 17 && p[1] == 7 {
            self.wrote = true;
            if self.mode == 2 && !self.used {
                self.used = true;
                return Ok(16);
            }
        }
        Ok(n)
    }
    fn read_timeout(&mut self, p: &mut [u8], ms: i32) -> Result<usize> {
        let n = self.inner.read_timeout(p, ms)?;
        if n == 17 {
            if self.mode == 0 && p[1] == 0x10 {
                p[7] = 255;
                p[16] = 0x55u8.wrapping_sub(sum(&p[..16]));
            } else if self.mode == 1 && self.wrote && !self.used && p[1] == 8 && p[4] == 108 {
                self.used = true;
                p[6] ^= 1;
                p[16] = 0x55u8.wrapping_sub(sum(&p[..16]));
            }
        }
        Ok(n)
    }
    fn send_feature_report(&mut self, p: &[u8]) -> Result<()> {
        self.inner.send_feature_report(p)
    }
    fn get_feature_report(&mut self, p: &mut [u8]) -> Result<usize> {
        self.inner.get_feature_report(p)
    }
}
#[test]
fn horizontal_identity_change_stops_before_any_set() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    d.io = Box::new(Fault {
        inner: d.io,
        mode: 0,
        wrote: false,
        used: false,
    });
    log.lock().unwrap().clear();
    assert!(d.apply(&setting(4, MouseButton::WheelRight)).is_err());
    assert!(sets(&log).is_empty());
}
#[test]
fn horizontal_bad_readback_restores_only_original_row() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    let before = d.read(108, 4).unwrap();
    d.io = Box::new(Fault {
        inner: d.io,
        mode: 1,
        wrote: false,
        used: false,
    });
    log.lock().unwrap().clear();
    assert!(d.apply(&setting(4, MouseButton::WheelRight)).is_err());
    let writes = sets(&log);
    assert_eq!(writes.len(), 2);
    assert!(writes.iter().all(|p| p[3..6] == [0, 108, 4]));
    assert_eq!(&writes[1][6..10], &before);
    assert_eq!(d.read(108, 4).unwrap(), before);
}
#[test]
fn horizontal_short_write_is_uncertain_and_not_resent() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    d.io = Box::new(Fault {
        inner: d.io,
        mode: 2,
        wrote: false,
        used: false,
    });
    log.lock().unwrap().clear();
    assert!(d.apply(&setting(4, MouseButton::WheelLeft)).is_err());
    assert_eq!(sets(&log).len(), 1);
}

#[path="compx_vertical_tests.rs"]
mod vertical;
