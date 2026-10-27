use super::*;
use gearwise_core::transport::MockRecord;
use std::sync::atomic::{AtomicU8, Ordering};
fn cycle(id: u16) -> Setting {
    Setting::Button {
        id,
        action: ButtonAction::ProfileCycle,
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
fn profile_cycle_nine_transports_and_all_nonleft_keys_keep_other_eeprom_and_peer() {
    for m in models()
        .iter()
        .filter(|m| profile_cycle_supported(m.vendor, m.product))
    {
        let (mut d, log) = memory_device(m.product).unwrap();
        let (mut peer, _) = memory_device(m.product).unwrap();
        let peer_before = peer.read_state().unwrap();
        for id in 2..=m.buttons {
            let before = d.read_blob(0, 768 + 12 * 384).unwrap();
            log.lock().unwrap().clear();
            d.apply(&cycle(id)).unwrap();
            assert_eq!(
                d.read_state().unwrap().buttons[&id],
                ButtonAction::ProfileCycle
            );
            let at = 96 + usize::from(id - 1) * 4;
            let after = d.read_blob(0, before.len()).unwrap();
            assert_eq!(&after[..at], &before[..at]);
            assert_eq!(&after[at + 4..], &before[at + 4..]);
            assert!(sets(&log)
                .iter()
                .all(|p| u16::from_be_bytes([p[3], p[4]]) == at as u16));
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
            ButtonAction::ProfileCycle
        );
    }
}
#[test]
fn profile_cycle_complete_vector_and_noop_use_no_macro_or_bank_set() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    log.lock().unwrap().clear();
    d.apply(&cycle(4)).unwrap();
    assert_eq!(
        sets(&log),
        vec![vec![
            8, 7, 0, 0, 108, 4, 9, 255, 0, 77, 0, 0, 0, 0, 0, 0, 129
        ]]
    );
    assert!(!log
        .lock()
        .unwrap()
        .iter()
        .any(|r| matches!(r,MockRecord::Write(p)if p[1]==15)));
    log.lock().unwrap().clear();
    d.apply(&cycle(4)).unwrap();
    assert!(sets(&log).is_empty());
    assert!(d.macro_definitions().is_empty());
}
#[test]
fn profile_cycle_does_not_alias_direct_target_unknown_values_or_pulsar() {
    for row in [
        [9, 161, 0, 171],
        [9, 164, 0, 168],
        [9, 0, 0, 76],
        [9, 255, 1, 76],
        [9, 255, 0, 0],
    ] {
        let (mut d, log) = memory_device(0x1017).unwrap();
        d.io.write(&frame(7, 108, 4, &row).unwrap()).unwrap();
        assert!(d.read_action(&row).is_none());
        log.lock().unwrap().clear();
        assert!(d.apply(&cycle(4)).is_err());
        assert!(sets(&log).is_empty());
    }
    for product in [0xf507, 0xf508] {
        let (mut d, log) = memory_device(product).unwrap();
        log.lock().unwrap().clear();
        assert!(d.apply(&cycle(4)).is_err());
        assert!(log.lock().unwrap().is_empty());
    }
    for id in [0, 1, 7, 255] {
        let (mut d, log) = memory_device(0x1017).unwrap();
        log.lock().unwrap().clear();
        assert!(d.apply(&cycle(id)).is_err());
        assert!(log.lock().unwrap().is_empty());
    }
}
struct Bank {
    inner: Box<dyn HidTransport>,
    token: Arc<AtomicU8>,
    flip_after_row: bool,
    short: bool,
}
impl HidTransport for Bank {
    fn write(&mut self, p: &[u8]) -> Result<usize> {
        self.inner.write(p)
    }
    fn read_timeout(&mut self, p: &mut [u8], ms: i32) -> Result<usize> {
        let n = self.inner.read_timeout(p, ms)?;
        if n == 17 && p[1] == 14 {
            let raw = if self.short {
                vec![]
            } else {
                vec![self.token.load(Ordering::SeqCst)]
            };
            p[..17].copy_from_slice(&frame(14, 0, raw.len(), &raw)?);
        } else if n == 17 && p[1] == 8 && p[3..5] == [0, 108] && self.flip_after_row {
            self.flip_after_row = false;
            self.token.store(1, Ordering::SeqCst);
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
fn changed_bank_rejects_reads_and_all_setting_writes_without_retargeting() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    let token = Arc::new(AtomicU8::new(1));
    d.io = Box::new(Bank {
        inner: d.io,
        token,
        flip_after_row: false,
        short: false,
    });
    log.lock().unwrap().clear();
    assert!(d.read_state().is_err());
    for s in [
        cycle(4),
        Setting::ReportRate(500),
        Setting::DpiActiveStage(1),
    ] {
        assert!(d.apply(&s).is_err());
    }
    assert!(sets(&log).is_empty());
    assert_eq!(d.profile, Some(0));
}
#[test]
fn board_switch_between_row_get_and_set_stops_before_first_set() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    let token = Arc::new(AtomicU8::new(0));
    d.io = Box::new(Bank {
        inner: d.io,
        token: token.clone(),
        flip_after_row: true,
        short: false,
    });
    log.lock().unwrap().clear();
    assert!(d.write_verified(108, &[9, 255, 0, 77]).is_err());
    assert_eq!(token.load(Ordering::SeqCst), 1);
    assert!(sets(&log).is_empty());
    assert_eq!(d.profile, Some(0));
}
#[test]
fn missing_bank_token_is_not_assumed_zero_and_opaque_tokens_are_not_renumbered() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    d.io = Box::new(Bank {
        inner: d.io,
        token: Arc::new(AtomicU8::new(0)),
        flip_after_row: false,
        short: true,
    });
    log.lock().unwrap().clear();
    assert!(d.apply(&cycle(4)).is_err());
    assert!(sets(&log).is_empty());
    let (mut d, _) = memory_device(0x1017).unwrap();
    d.profile = None;
    d.io = Box::new(Bank {
        inner: d.io,
        token: Arc::new(AtomicU8::new(164)),
        flip_after_row: false,
        short: false,
    });
    d.identify().unwrap();
    assert_eq!(d.profile, Some(164));
}
