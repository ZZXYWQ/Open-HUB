//! Passive report 03/55/40/01; no SET_REPORT/query or global mouse hook.
//! MIT source pin and native collection evidence: docs/X11-ADAPTER.md.
use super::*;
use gearwise_core::transport::{BusKind, HidInterfaceInfo};
const FRESH: Duration = Duration::from_secs(90);
#[derive(Default)]
pub(super) struct BatteryInput {
    pub io: Option<Box<dyn HidTransport>>,
    level: Option<u8>,
    at: Option<Instant>,
    pending: Option<u8>,
}
impl BatteryInput {
    fn clear(&mut self) {
        self.level = None;
        self.at = None;
        self.pending = None;
    }
    fn accept(&mut self, data: &[u8], now: Instant) {
        if data.len() < 5 || data.len() > 64 || data[..4] != [3, 0x55, 0x40, 1] || data[4] > 100 {
            self.pending = None;
            return;
        }
        let percent = data[4];
        if let Some(old) = self.level {
            if old.abs_diff(percent) > 5 && self.pending != Some(percent) {
                self.pending = Some(percent);
                return;
            }
        }
        self.level = Some(percent);
        self.at = Some(now);
        self.pending = None;
    }
    pub fn poll(&mut self, now: Instant) -> Option<u8> {
        if self
            .at
            .is_some_and(|at| now.saturating_duration_since(at) > FRESH)
        {
            self.clear();
        }
        let mut buf = [0u8; 65];
        for _ in 0..16 {
            let Some(io) = self.io.as_mut() else {
                self.clear();
                break;
            };
            match io.read_timeout(&mut buf, 0) {
                Ok(0) => break,
                Ok(n) if n <= buf.len() => self.accept(&buf[..n], now),
                _ => {
                    self.clear();
                    break;
                }
            }
        }
        self.level
    }
}
/// Select only one input collection in the exact control collection's
/// Windows container. Serial-less mice do not fall back to "first PID".
pub(super) fn choose_input<'a>(
    control: &HidInterfaceInfo,
    all: &'a [HidInterfaceInfo],
    mut container: impl FnMut(&str) -> Option<[u8; 16]>,
) -> Option<&'a HidInterfaceInfo> {
    if control.vendor_id != VID
        || control.product_id != WIRELESS
        || control.bus != BusKind::Usb
        || control.interface_number != 2
    {
        return None;
    }
    let target = container(&control.path)?;
    if target == [0; 16] || target == [0xff; 16] {
        return None;
    }
    let mut matches = all.iter().filter(|i| {
        i.vendor_id == VID
            && i.product_id == WIRELESS
            && i.bus == BusKind::Usb
            && i.interface_number == 2
            && i.usage_page == 0x0a
            && i.path != control.path
            && container(&i.path) == Some(target)
    });
    let found = matches.next()?;
    if matches.next().is_some() {
        None
    } else {
        Some(found)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };
    struct Io {
        reports: VecDeque<Vec<u8>>,
        reads: Arc<Mutex<Vec<i32>>>,
        fail: bool,
    }
    impl HidTransport for Io {
        fn write(&mut self, _: &[u8]) -> Result<usize> {
            panic!("battery must never write")
        }
        fn send_feature_report(&mut self, _: &[u8]) -> Result<()> {
            panic!("battery must never set feature")
        }
        fn get_feature_report(&mut self, _: &mut [u8]) -> Result<usize> {
            panic!("battery must never get feature")
        }
        fn read_timeout(&mut self, out: &mut [u8], timeout: i32) -> Result<usize> {
            self.reads.lock().unwrap().push(timeout);
            if self.fail {
                return Err(invalid("disconnected"));
            }
            if let Some(r) = self.reports.pop_front() {
                out[..r.len()].copy_from_slice(&r);
                Ok(r.len())
            } else {
                Ok(0)
            }
        }
    }
    fn report(level: u8) -> Vec<u8> {
        vec![3, 0x55, 0x40, 1, level]
    }
    fn io(reports: Vec<Vec<u8>>) -> (Io, Arc<Mutex<Vec<i32>>>) {
        let reads = Arc::new(Mutex::new(vec![]));
        (
            Io {
                reports: reports.into(),
                reads: reads.clone(),
                fail: false,
            },
            reads,
        )
    }
    #[test]
    fn passive_battery_checks_signature_range_and_confirmed_large_jumps() {
        let now = Instant::now();
        let mut input = BatteryInput::default();
        for r in [
            vec![],
            vec![3, 0x55, 0x40, 1],
            vec![3, 0x55, 0x41, 1, 80],
            report(101),
        ] {
            input.accept(&r, now);
            assert_eq!(input.level, None);
        }
        input.accept(&report(80), now);
        assert_eq!(input.level, Some(80));
        input.accept(&report(100), now);
        assert_eq!(input.level, Some(80));
        input.accept(&report(80), now);
        input.accept(&report(100), now);
        assert_eq!(input.level, Some(80));
        input.accept(&report(100), now);
        assert_eq!(input.level, Some(100));
        input.accept(&report(0), now);
        input.accept(&report(0), now);
        assert_eq!(input.level, Some(0));
    }
    #[test]
    fn poll_only_reads_zero_timeout_and_never_invents_zero_for_missing_stale_or_failed_data() {
        let now = Instant::now();
        let (transport, reads) = io(vec![report(80)]);
        let mut input = BatteryInput {
            io: Some(Box::new(transport)),
            ..Default::default()
        };
        assert_eq!(input.poll(now), Some(80));
        assert_eq!(input.poll(now + FRESH), Some(80));
        assert_eq!(input.poll(now + FRESH + Duration::from_secs(1)), None);
        assert!(reads.lock().unwrap().iter().all(|n| *n == 0));
        let (mut broken, _) = io(vec![]);
        broken.fail = true;
        input.io = Some(Box::new(broken));
        input.accept(&report(80), now);
        assert_eq!(input.poll(now), None);
        let (busy, reads) = io(vec![report(60); 100]);
        input.io = Some(Box::new(busy));
        assert_eq!(input.poll(now), Some(60));
        assert_eq!(reads.lock().unwrap().len(), 16);
    }
    fn info(path: &str) -> HidInterfaceInfo {
        HidInterfaceInfo {
            path: path.into(),
            vendor_id: VID,
            product_id: WIRELESS,
            bus: BusKind::Usb,
            interface_number: 2,
            usage_page: 0x0a,
            ..Default::default()
        }
    }
    #[test]
    fn sibling_selection_uses_physical_container_and_refuses_ambiguity_missing_id_or_other_models()
    {
        let control = info("control");
        let a = info("input-A");
        let b = info("input-B");
        let group = |p: &str| Some(if p == "input-B" { [2; 16] } else { [1; 16] });
        let all = [b.clone(), a.clone()];
        assert_eq!(choose_input(&control, &all, group).unwrap().path, "input-A");
        assert!(choose_input(&control, &[a.clone(), info("duplicate")], group).is_none());
        assert!(choose_input(&control, &all, |_| None).is_none());
        assert!(choose_input(&control, &all, |_| Some([0; 16])).is_none());
        let mut candidates = [a.clone(), a.clone(), a.clone(), a.clone()];
        candidates[0].product_id = WIRED;
        candidates[1].vendor_id = 0x046d;
        candidates[2].bus = BusKind::Bluetooth;
        candidates[3].interface_number = 1;
        for wrong in candidates {
            assert!(choose_input(&control, &[wrong], group).is_none());
        }
        let mut wrong = a.clone();
        wrong.usage_page = 1;
        assert!(choose_input(&control, &[wrong], group).is_none());
        let mut wired = control.clone();
        wired.product_id = WIRED;
        assert!(choose_input(&wired, &all, group).is_none());
    }
}
