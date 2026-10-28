//! Complete framed transport faults, no physical HID or OS input.
use super::*;
use gearwise_core::tuning::MouseTuning;
struct Control {
    sets: usize,
    failures: Vec<(usize, bool)>,
    mutation: Option<(u16, Vec<u8>)>,
    on_prepare: bool,
    bank_swap: bool,
    bank_changed: bool,
}
struct Fault {
    inner: Box<dyn HidTransport>,
    control: Arc<Mutex<Control>>,
}
impl HidTransport for Fault {
    fn write(&mut self, p: &[u8]) -> Result<usize> {
        if p[1] != 7 {
            return self.inner.write(p);
        }
        let (fail, mutation) = {
            let mut c = self.control.lock().unwrap();
            c.sets += 1;
            let n = c.sets;
            if n == 1 && c.bank_swap {
                c.bank_changed = true;
            }
            let fail = c
                .failures
                .iter()
                .find(|(at, _)| *at == n)
                .map(|(_, after)| *after);
            let mutation = if n == 1 && !c.on_prepare {
                c.mutation.clone()
            } else {
                None
            };
            (fail, mutation)
        };
        if fail != Some(false) {
            self.inner.write(p)?;
        }
        if let Some((addr, bytes)) = mutation {
            self.inner.write(&frame(7, addr, bytes.len(), &bytes)?)?;
        }
        if fail.is_some() {
            return Err(Error::invalid("synthetic performance transfer failure"));
        }
        Ok(17)
    }
    fn read_timeout(&mut self, p: &mut [u8], ms: i32) -> Result<usize> {
        let n = self.inner.read_timeout(p, ms)?;
        if n == 17 && p[1] == 14 && self.control.lock().unwrap().bank_changed {
            p[..17].copy_from_slice(&frame(14, 0, 1, &[1])?);
        } else if n == 17 && p[1] == 8 && u16::from_be_bytes([p[3], p[4]]) == 181 {
            let mutation = {
                let mut c = self.control.lock().unwrap();
                if c.on_prepare {
                    c.on_prepare = false;
                    c.mutation.take()
                } else {
                    None
                }
            };
            if let Some((addr, bytes)) = mutation {
                self.inner.write(&frame(7, addr, bytes.len(), &bytes)?)?;
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
fn specs() -> impl Iterator<Item = &'static Model> {
    models()
        .iter()
        .filter(|m| tuning::capabilities(m).is_some())
}
fn patch(m: &Model) -> MouseTuning {
    let c = tuning::capabilities(m).unwrap();
    MouseTuning {
        debounce_ms: c.debounce_ms.last().copied(),
        motion_sync: Some(false),
        angle_snapping: c.angle_snapping.then_some(true),
        ripple_control: Some(true),
        led_idle_seconds: Some(120),
        sensor_sleep_enabled: Some(false),
        sensor_sleep_seconds: Some(300),
        lift_off_microns: c.lift_off_microns.last().copied(),
        ..Default::default()
    }
}
fn instrument(
    mut d: Atk,
    failures: Vec<(usize, bool)>,
    mutation: Option<(u16, Vec<u8>)>,
    on_prepare: bool,
    bank_swap: bool,
) -> (Atk, Arc<Mutex<Control>>) {
    let c = Arc::new(Mutex::new(Control {
        sets: 0,
        failures,
        mutation,
        on_prepare,
        bank_swap,
        bank_changed: false,
    }));
    d.io = Box::new(Fault {
        inner: d.io,
        control: c.clone(),
    });
    (d, c)
}
#[test]
fn all_performance_write_stages_before_after_effect_restore_exact_preimages() {
    for m in specs() {
        let rows = if tuning::capabilities(m).unwrap().lift_off_microns.is_empty() {
            2
        } else {
            3
        };
        for nth in 1..=rows {
            for after in [false, true] {
                let (mut d, _) = memory_device(m.product).unwrap();
                let before = d.read_blob(0, 768 + 12 * 384).unwrap();
                let (mut d, _) = instrument(d, vec![(nth, after)], None, false, false);
                let error = d.apply(&Setting::Tuning(patch(m))).unwrap_err().to_string();
                assert!(
                    error.contains("restoration: confirmed"),
                    "{} {nth} {after}: {error}",
                    m.name
                );
                assert_eq!(d.read_blob(0, before.len()).unwrap(), before);
            }
        }
    }
}
#[test]
fn external_selected_neighbor_and_unattempted_rows_are_preserved_without_restore() {
    for m in specs() {
        for (addr, bytes) in [
            (169, vec![211, 130]),
            (169, vec![211]),
            (175, vec![211, 130]),
            (185, vec![211, 130]),
        ] {
            let (mut d, _) = memory_device(m.product).unwrap();
            let before = d.read_blob(0, 768 + 12 * 384).unwrap();
            let (mut d, c) = instrument(d, vec![], Some((addr, bytes.clone())), false, false);
            let error = d.apply(&Setting::Tuning(patch(m))).unwrap_err().to_string();
            assert!(
                error.contains("state uncertain"),
                "{} {addr}: {error}",
                m.name
            );
            assert_eq!(c.lock().unwrap().sets, 1);
            assert_eq!(d.read(addr, bytes.len()).unwrap(), bytes);
            assert_eq!(d.read_blob(0, 10).unwrap(), before[..10]);
            assert_eq!(d.read_blob(187, 581).unwrap(), before[187..768]);
        }
    }
}
#[test]
fn prepared_rows_changed_before_first_set_reject_with_zero_set() {
    for m in specs() {
        let (d, _) = memory_device(m.product).unwrap();
        let (mut d, c) = instrument(d, vec![], Some((169, vec![211, 130])), true, false);
        assert!(d.apply(&Setting::Tuning(patch(m))).is_err());
        assert_eq!(c.lock().unwrap().sets, 0);
        assert_eq!(d.read(169, 2).unwrap(), [211, 130]);
    }
}
#[test]
fn bank_change_after_first_set_never_restores_or_continues_to_another_bank() {
    for m in specs() {
        let (d, _) = memory_device(m.product).unwrap();
        let (mut d, c) = instrument(d, vec![], None, false, true);
        let error = d.apply(&Setting::Tuning(patch(m))).unwrap_err().to_string();
        assert!(error.contains("state uncertain"), "{}: {error}", m.name);
        assert_eq!(c.lock().unwrap().sets, 1);
    }
}
#[test]
fn a_failed_performance_restore_stops_and_never_claims_confirmation() {
    for m in specs() {
        let (d, _) = memory_device(m.product).unwrap();
        let (mut d, c) = instrument(d, vec![(2, true), (3, false)], None, false, false);
        let error = d.apply(&Setting::Tuning(patch(m))).unwrap_err().to_string();
        assert!(error.contains("state uncertain"), "{}: {error}", m.name);
        assert!(!error.contains("restoration: confirmed"));
        assert_eq!(c.lock().unwrap().sets, 3);
    }
}
