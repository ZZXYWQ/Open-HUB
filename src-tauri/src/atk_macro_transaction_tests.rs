//! Faults act at complete real COMPX reports; no HID or OS input.
use super::*;
use gearwise_core::profile::{Macro, MacroMode, MacroStep};
struct FaultState {
    sets: usize,
    failures: Vec<(usize, bool)>,
    mutate: Option<(usize, u16, Vec<u8>)>,
}
struct FaultTransport {
    inner: Box<dyn HidTransport>,
    state: Arc<Mutex<FaultState>>,
}
impl HidTransport for FaultTransport {
    fn write(&mut self, p: &[u8]) -> Result<usize> {
        if p[1] != 7 {
            return self.inner.write(p);
        }
        let (fail, mutation) = {
            let mut s = self.state.lock().unwrap();
            s.sets += 1;
            let n = s.sets;
            let fail = s
                .failures
                .iter()
                .find(|(nth, _)| *nth == n)
                .map(|(_, after)| *after);
            let mutation = s.mutate.as_ref().filter(|(nth, _, _)| *nth == n).cloned();
            (fail, mutation)
        };
        if fail != Some(false) {
            self.inner.write(p)?;
        }
        if let Some((_, addr, data)) = mutation {
            self.inner.write(&frame(7, addr, data.len(), &data)?)?;
        }
        if fail.is_some() {
            return Err(Error::invalid("synthetic uncertain COMPX macro SET"));
        }
        Ok(17)
    }
    fn read_timeout(&mut self, p: &mut [u8], ms: i32) -> Result<usize> {
        self.inner.read_timeout(p, ms)
    }
    fn send_feature_report(&mut self, p: &[u8]) -> Result<()> {
        self.inner.send_feature_report(p)
    }
    fn get_feature_report(&mut self, p: &mut [u8]) -> Result<usize> {
        self.inner.get_feature_report(p)
    }
}
fn definition() -> Macro {
    Macro {
        id: "txn".into(),
        name: "native transaction".into(),
        mode: MacroMode::Repeat(3),
        steps: vec![
            MacroStep::KeyDown(4),
            MacroStep::Delay(31),
            MacroStep::KeyUp(4),
        ],
    }
}
fn action() -> Setting {
    Setting::Button {
        id: 4,
        action: ButtonAction::Macro("txn".into()),
    }
}
fn specs() -> impl Iterator<Item = &'static Model> {
    models()
        .iter()
        .filter(|m| horizontal_scroll_supported(m.vendor, m.product))
}
fn instrument(
    mut d: Atk,
    failures: Vec<(usize, bool)>,
    mutate: Option<(usize, u16, Vec<u8>)>,
) -> (Atk, Arc<Mutex<FaultState>>) {
    let state = Arc::new(Mutex::new(FaultState {
        sets: 0,
        failures,
        mutate,
    }));
    d.io = Box::new(FaultTransport {
        inner: d.io,
        state: state.clone(),
    });
    (d, state)
}
#[test]
fn complete_macro_slot_tail_other_slots_and_all_other_bindings_survive_success() {
    for spec in specs() {
        let (mut d, _) = memory_device(spec.product).unwrap();
        d.write_verified(768 + 380, &[91, 92, 93, 94]).unwrap();
        let before = d.read_blob(0, 768 + 12 * 384).unwrap();
        d.set_macro_definitions(&[definition()]).unwrap();
        d.apply(&action()).unwrap();
        let after = d.read_blob(0, before.len()).unwrap();
        let payload = native_macro::encode(&definition()).unwrap();
        assert_eq!(&after[768..768 + payload.len()], payload.as_slice());
        assert_eq!(&after[108..112], &[6, 0, 3, 76]);
        for at in 0..before.len() {
            if !(108..112).contains(&at) && !(768..768 + payload.len()).contains(&at) {
                assert_eq!(after[at], before[at], "untouched byte {at}");
            }
        }
    }
}
#[test]
fn every_macro_and_binding_set_before_or_after_effect_recovers_exact_original_state() {
    let (mut baseline, _) = memory_device(0x1115).unwrap();
    baseline.set_macro_definitions(&[definition()]).unwrap();
    let (mut baseline, counter) = instrument(baseline, vec![], None);
    baseline.apply(&action()).unwrap();
    let writes = counter.lock().unwrap().sets;
    assert_eq!(writes, 6);
    for spec in specs() {
        for nth in 1..=writes {
            for after_effect in [false, true] {
                let (mut d, _) = memory_device(spec.product).unwrap();
                let before = d.read_blob(0, 768 + 12 * 384).unwrap();
                d.set_macro_definitions(&[definition()]).unwrap();
                let (mut d, _) = instrument(d, vec![(nth, after_effect)], None);
                let error = d.apply(&action()).unwrap_err().to_string();
                assert!(
                    error.contains("restored and verified"),
                    "{} {nth} {after_effect}: {error}",
                    spec.name
                );
                assert_eq!(d.read_blob(0, before.len()).unwrap(), before);
            }
        }
    }
}
#[test]
fn external_macro_row_tail_and_current_bank_binding_changes_block_all_recovery_writes() {
    for spec in specs() {
        for (addr, bytes) in [
            (768, vec![0xa5]),
            (768 + 380, vec![0xa5]),
            (96 + 4, vec![6, 0, 1, 78]),
            (108, vec![1, 2, 0, 82]),
        ] {
            let (mut d, _) = memory_device(spec.product).unwrap();
            d.set_macro_definitions(&[definition()]).unwrap();
            let (mut d, count) = instrument(d, vec![], Some((1, addr, bytes.clone())));
            let error = d.apply(&action()).unwrap_err().to_string();
            assert!(
                error.contains("state uncertain"),
                "{} {addr}: {error}",
                spec.name
            );
            assert_eq!(count.lock().unwrap().sets, 1);
            assert_eq!(d.read(addr, bytes.len()).unwrap(), bytes);
        }
    }
}
#[test]
fn failed_restore_is_uncertain_and_never_reported_verified_or_retried() {
    for spec in specs() {
        let (mut d, _) = memory_device(spec.product).unwrap();
        d.set_macro_definitions(&[definition()]).unwrap();
        let (mut d, count) = instrument(d, vec![(2, true), (3, false)], None);
        let error = d.apply(&action()).unwrap_err().to_string();
        assert!(error.contains("state uncertain"), "{error}");
        assert!(!error.contains("restored and verified"));
        assert_eq!(count.lock().unwrap().sets, 3);
    }
}
#[test]
fn transaction_rechecks_current_bank_reference_and_free_slot_before_any_write() {
    for spec in specs() {
        let (mut d, _) = memory_device(spec.product).unwrap();
        let payload = native_macro::encode(&definition()).unwrap();
        let old = d.read(108, 4).unwrap();
        d.write_verified(100, &[6, 0, 1, 78]).unwrap();
        let (mut d, count) = instrument(d, vec![], None);
        assert!(d
            .commit_native_macro(4, 768, &payload, &old, &[6, 0, 3, 76])
            .is_err());
        assert_eq!(count.lock().unwrap().sets, 0);
        let (mut d, _) = memory_device(spec.product).unwrap();
        let old = d.read(108, 4).unwrap();
        d.write_verified(768 + 31, &[2]).unwrap();
        let (mut d, count) = instrument(d, vec![], None);
        assert!(d
            .commit_native_macro(4, 768, &payload, &old, &[6, 0, 3, 76])
            .is_err());
        assert_eq!(count.lock().unwrap().sets, 0);
    }
}

#[test]
fn vertical_scroll_macro_modes_execute_and_reopen_across_all_existing_compx_connections(){
 use gearwise_core::model::ScrollDirection;
 for spec in specs(){for mode in[MacroMode::Once,MacroMode::Repeat(3),MacroMode::WhileHeld,MacroMode::Toggle]{let(mut d,_)=memory_device(spec.product).unwrap();let mut m=definition();m.mode=mode;m.steps=vec![MacroStep::Scroll(ScrollDirection::Up),MacroStep::Delay(37),MacroStep::Scroll(ScrollDirection::Down)];d.set_macro_definitions(&[m.clone()]).unwrap();d.apply(&action()).unwrap();let state=d.read_state().unwrap();let ButtonAction::Macro(id)=&state.buttons[&4]else{panic!("scroll binding not decoded")};let actual=d.macro_definitions().into_iter().find(|m|&m.id==id).unwrap();assert_eq!(actual.mode,mode);assert_eq!(actual.steps,m.steps);let info=HidInterfaceInfo{vendor_id:spec.vendor,product_id:spec.product,usage_page:0xff02,usage:2,interface_number:1,..Default::default()};let mut reopened=Atk::open(d.io,&info).unwrap();let state=reopened.read_state().unwrap();assert!(matches!(state.buttons[&4],ButtonAction::Macro(_)));assert!(reopened.macro_definitions().iter().any(|x|x.steps==m.steps&&x.mode==mode));}}
}
