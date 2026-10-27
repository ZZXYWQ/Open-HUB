//! Synthetic OEM identity replies at the real HID framing boundary.
use super::*;
use gearwise_core::transport::MockRecord;
struct ReplyState {
    raw: Vec<u8>,
    swap_on_get: Option<u16>,
    swap_on_set: bool,
}
struct UnitReply {
    inner: Box<dyn HidTransport>,
    state: Arc<Mutex<ReplyState>>,
}
impl HidTransport for UnitReply {
    fn write(&mut self, p: &[u8]) -> Result<usize> {
        let n = self.inner.write(p)?;
        if p.get(1) == Some(&7) {
            let mut s = self.state.lock().unwrap();
            if s.swap_on_set {
                s.raw[9] ^= 0x80;
            }
        }
        Ok(n)
    }
    fn read_timeout(&mut self, p: &mut [u8], ms: i32) -> Result<usize> {
        let n = self.inner.read_timeout(p, ms)?;
        if n == 17 && p[1] == 0x10 {
            let raw = self.state.lock().unwrap().raw.clone();
            p[..17].copy_from_slice(&frame(0x10, 0, raw.len(), &raw)?);
        } else if n == 17 && p[1] == 8 {
            let addr = u16::from_be_bytes([p[3], p[4]]);
            let mut s = self.state.lock().unwrap();
            if s.swap_on_get == Some(addr) {
                s.raw[9] ^= 0x80;
                s.swap_on_get = None;
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
fn full(mid: u8, token: [u8; 4]) -> Vec<u8> {
    let mut r = vec![2, mid, 0xa2, 0xa3, 0xa4, 0xa5];
    r.extend(token);
    r
}
fn model_specs() -> impl Iterator<Item = &'static Model> {
    models()
        .iter()
        .filter(|m| horizontal_scroll_supported(m.vendor, m.product))
}
fn open_with(
    spec: &Model,
    raw: Vec<u8>,
) -> Result<(Atk, Arc<Mutex<ReplyState>>, Arc<Mutex<Vec<MockRecord>>>)> {
    let (base, log) = memory_device(spec.product)?;
    let state = Arc::new(Mutex::new(ReplyState {
        raw,
        swap_on_get: None,
        swap_on_set: false,
    }));
    let info = HidInterfaceInfo {
        path: format!("simulation:identity:{:04x}", spec.product),
        vendor_id: spec.vendor,
        product_id: spec.product,
        usage_page: 0xff02,
        usage: 2,
        interface_number: 1,
        ..Default::default()
    };
    let d = Atk::open(
        Box::new(UnitReply {
            inner: base.io,
            state: state.clone(),
        }),
        &info,
    )?;
    Ok((d, state, log))
}
fn set_count(log: &Arc<Mutex<Vec<MockRecord>>>) -> usize {
    log.lock()
        .unwrap()
        .iter()
        .filter(|r| matches!(r,MockRecord::Write(p)if p[1]==7))
        .count()
}
#[test]
fn full_unit_identity_pins_all_four_bytes_for_every_existing_compx_connection() {
    assert_eq!(model_specs().count(), 9);
    for spec in model_specs() {
        for at in 6..10 {
            let (mut d, state, log) = open_with(spec, full(spec.mids[0], [1, 2, 3, 4])).unwrap();
            assert_eq!(d.unit_token, Some([1, 2, 3, 4]));
            assert_eq!(d.desc.serial, None);
            log.lock().unwrap().clear();
            state.lock().unwrap().raw[at] ^= 0x40;
            assert!(d.apply(&Setting::ReportRate(500)).is_err());
            assert_eq!(set_count(&log), 0);
            assert!(d.read_state().is_err());
        }
    }
}
#[test]
fn unit_tokens_are_opaque_not_guessed_serials_and_reserved_bytes_are_not_identity() {
    for spec in model_specs() {
        for token in [[0; 4], [255; 4], [0x80, 0, 255, 0x11]] {
            let (mut d, state, log) = open_with(spec, full(spec.mids[0], token)).unwrap();
            assert_eq!(d.unit_token, Some(token));
            assert_eq!(d.desc.serial, None);
            for at in 2..6 {
                state.lock().unwrap().raw[at] ^= 0x40;
            }
            d.apply(&Setting::ReportRate(500)).unwrap();
            assert!(set_count(&log) > 0);
            assert_eq!(d.unit_token, Some(token));
        }
    }
}
#[test]
fn truncated_full_or_late_full_identity_never_downgrades_or_repins_a_connection() {
    for spec in model_specs() {
        for len in [0, 1, 2, 3, 5, 6, 9] {
            let (mut d, state, log) = open_with(spec, full(spec.mids[0], [1, 2, 3, 4])).unwrap();
            state.lock().unwrap().raw.truncate(len);
            log.lock().unwrap().clear();
            assert!(d.apply(&Setting::ReportRate(500)).is_err());
            assert_eq!(set_count(&log), 0);
            assert_eq!(d.unit_token, Some([1, 2, 3, 4]));
        }
        let (mut d, state, log) = open_with(spec, vec![2, spec.mids[0]]).unwrap();
        state.lock().unwrap().raw = full(spec.mids[0], [1, 2, 3, 4]);
        log.lock().unwrap().clear();
        assert!(d.apply(&Setting::ReportRate(500)).is_err());
        assert_eq!(set_count(&log), 0);
        assert_eq!(d.unit_token, None);
    }
}
#[test]
fn full_identity_swap_between_row_get_and_set_sends_no_eeprom_write() {
    for spec in model_specs() {
        let (mut d, state, log) = open_with(spec, full(spec.mids[0], [1, 2, 3, 4])).unwrap();
        state.lock().unwrap().swap_on_get = Some(0);
        log.lock().unwrap().clear();
        assert!(d.apply(&Setting::ReportRate(500)).is_err());
        assert_eq!(set_count(&log), 0);
    }
}
#[test]
fn full_identity_swap_after_set_is_not_reported_success_and_never_restores_to_new_unit() {
    for spec in model_specs() {
        let (mut d, state, log) = open_with(spec, full(spec.mids[0], [1, 2, 3, 4])).unwrap();
        state.lock().unwrap().swap_on_set = true;
        log.lock().unwrap().clear();
        assert!(d.apply(&Setting::ReportRate(500)).is_err());
        assert_eq!(set_count(&log), 1);
        assert_eq!(d.unit_token, Some([1, 2, 3, 4]));
    }
}
#[test]
fn malformed_identity_shapes_refuse_open_and_two_same_model_units_remain_independent() {
    for spec in model_specs() {
        for len in [0, 1, 3, 4, 5, 6, 7, 8, 9] {
            let mut raw = full(spec.mids[0], [1, 2, 3, 4]);
            raw.truncate(len);
            assert!(open_with(spec, raw).is_err());
        }
        let (mut a, sa, la) = open_with(spec, full(spec.mids[0], [1, 2, 3, 4])).unwrap();
        let (mut b, _, lb) = open_with(spec, full(spec.mids[0], [4, 3, 2, 1])).unwrap();
        sa.lock().unwrap().raw[9] ^= 1;
        la.lock().unwrap().clear();
        lb.lock().unwrap().clear();
        assert!(a.apply(&Setting::ReportRate(500)).is_err());
        b.apply(&Setting::ReportRate(500)).unwrap();
        assert_eq!(set_count(&la), 0);
        assert!(set_count(&lb) > 0);
    }
}

#[test]
fn full_identity_keeps_native_dpi_rate_buttons_macro_and_palette_paths_working() {
    use gearwise_core::profile::{Macro, MacroMode, MacroStep};
    for spec in model_specs() {
        let (mut d, _, _) = open_with(spec, full(spec.mids[0], [1, 2, 3, 4])).unwrap();
        let dpi = DpiState {
            stages: vec![DpiStage::uniform(800), DpiStage::uniform(2400)],
            active: 1,
        };
        d.apply(&Setting::Dpi(dpi.clone())).unwrap();
        d.apply(&Setting::ReportRate(500)).unwrap();
        d.apply(&Setting::Button {
            id: 2,
            action: ButtonAction::ProfileSelect(3),
        })
        .unwrap();
        d.set_macro_definitions(&[Macro {
            id: "identity-test".into(),
            name: "identity macro".into(),
            mode: MacroMode::Once,
            steps: vec![
                MacroStep::KeyDown(4),
                MacroStep::Delay(31),
                MacroStep::KeyUp(4),
            ],
        }])
        .unwrap();
        d.apply(&Setting::Button {
            id: 4,
            action: ButtonAction::Macro("identity-test".into()),
        })
        .unwrap();
        if spec.dpi_color_stages > 0 {
            d.apply(&Setting::DpiColor {
                stage: 0,
                color: Rgb {
                    r: 17,
                    g: 63,
                    b: 211,
                },
            })
            .unwrap();
        }
        let state = d.read_state().unwrap();
        assert_eq!(state.dpi, Some(dpi));
        assert_eq!(state.report_rate, Some(500));
        assert_eq!(state.buttons[&2], ButtonAction::ProfileSelect(3));
        assert!(matches!(state.buttons[&4], ButtonAction::Macro(_)));
        assert_eq!(d.unit_token, Some([1, 2, 3, 4]));
        assert_eq!(d.desc.serial, None);
    }
}
#[test]
fn unit_swap_during_first_macro_chunk_never_sends_later_chunks_or_recovery_to_new_unit() {
    use gearwise_core::profile::{Macro, MacroMode, MacroStep};
    for spec in model_specs() {
        let (mut d, state, log) = open_with(spec, full(spec.mids[0], [1, 2, 3, 4])).unwrap();
        d.set_macro_definitions(&[Macro {
            id: "identity-test".into(),
            name: "identity macro".into(),
            mode: MacroMode::Once,
            steps: vec![MacroStep::KeyDown(4), MacroStep::KeyUp(4)],
        }])
        .unwrap();
        state.lock().unwrap().swap_on_set = true;
        log.lock().unwrap().clear();
        assert!(d
            .apply(&Setting::Button {
                id: 4,
                action: ButtonAction::Macro("identity-test".into())
            })
            .is_err());
        assert_eq!(set_count(&log), 1);
        assert!(!log.lock().unwrap().iter().any(
            |r| matches!(r,MockRecord::Write(p) if p[1]==7&&u16::from_be_bytes([p[3],p[4]])==108)
        ));
    }
}
#[test]
#[ignore = "requires existing built UI profile-select saved plans"]
fn browser_compx_saved_plans_execute_with_full_unit_identity() {
    let profiles: Vec<crate::profiles::Profile> = serde_json::from_slice(
        &std::fs::read(std::env::var_os("OPEN_HUB_COMPX_IDENTITY_UI_PROFILES").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(profiles.len(), 9);
    for p in profiles {
        let plan = p.multi.unwrap();
        let spec = model(plan.vendor, plan.product).unwrap();
        let (mut d, state, log) = open_with(spec, full(spec.mids[0], [1, 2, 3, 4])).unwrap();
        let c = crate::multibrand::compile(&plan).unwrap();
        assert!(!c.software_input);
        for setting in &c.settings {
            d.apply(setting).unwrap();
        }
        let read = d.read_state().unwrap();
        for i in 0..4 {
            assert_eq!(
                read.buttons[&(u16::from(i) + 2)],
                ButtonAction::ProfileSelect(i)
            );
        }
        state.lock().unwrap().raw[9] ^= 0x80;
        log.lock().unwrap().clear();
        for setting in &c.settings {
            assert!(d.apply(setting).is_err());
        }
        assert_eq!(set_count(&log), 0);
    }
}
