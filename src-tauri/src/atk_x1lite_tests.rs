use super::*;
#[test]
fn x1lite_exact_model_collection_and_independent_identity_vectors() {
    let m = model(0x373b, 0x113e).unwrap();
    assert_eq!(m.mids, [89]);
    assert_eq!(
        (m.min_dpi, m.max_dpi, m.buttons, m.usage_page),
        (100, 30000, 6, 0xff04)
    );
    let mut i = HidInterfaceInfo {
        vendor_id: 0x373b,
        product_id: 0x113e,
        usage_page: 0xff04,
        usage: 2,
        ..Default::default()
    };
    assert!(AtkDriver.probe(&i));
    for page in [0xff02, 0xff05, 1] {
        i.usage_page = page;
        assert!(!AtkDriver.probe(&i));
    }
    i.usage_page = 0xff04;
    i.vendor_id = 0x3554;
    assert!(!AtkDriver.probe(&i));
    assert_eq!(
        frame(16, 0, 0, &[]).unwrap(),
        [8, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 61]
    );
    assert_eq!(
        frame(16, 0, 10, &[2, 89, 0, 0, 0, 0, 1, 2, 3, 4]).unwrap(),
        [8, 16, 0, 0, 0, 10, 2, 89, 0, 0, 0, 0, 1, 2, 3, 4, 206]
    );
}
#[test]
fn x1lite_known_controls_dpi_and_light_palette_roundtrip_and_reopen() {
    use gearwise_core::model::{DpiIndicatorMode, DpiIndicatorState, IndicatorLevel};
    let (mut d, log) = memory_device(0x113e).unwrap();
    let dpi = DpiState {
        stages: vec![DpiStage { x: 100, y: 150 }, DpiStage { x: 30000, y: 25000 }],
        active: 1,
    };
    d.apply(&Setting::Dpi(dpi.clone())).unwrap();
    d.apply(&Setting::ReportRate(500)).unwrap();
    for id in 2..=6 {
        d.apply(&Setting::Button {
            id,
            action: ButtonAction::Scroll(gearwise_core::model::ScrollDirection::Down),
        })
        .unwrap();
    }
    let light = DpiIndicatorState {
        enabled: true,
        mode: DpiIndicatorMode::Breathing,
        brightness: IndicatorLevel::High,
        speed: IndicatorLevel::Low,
    };
    d.apply(&Setting::DpiIndicator(light.clone())).unwrap();
    for stage in 0..8 {
        d.apply(&Setting::DpiColor {
            stage,
            color: Rgb {
                r: 17 + stage,
                g: 34,
                b: 51,
            },
        })
        .unwrap();
    }
    let info = HidInterfaceInfo {
        vendor_id: 0x373b,
        product_id: 0x113e,
        usage_page: 0xff04,
        usage: 2,
        ..Default::default()
    };
    let mut d = Atk::open(d.io, &info).unwrap();
    let state = d.read_state().unwrap();
    assert_eq!(state.dpi, Some(dpi));
    assert_eq!(state.report_rate, Some(500));
    assert_eq!(state.dpi_indicator, Some(light));
    assert_eq!(state.dpi_colors.len(), 8);
    for id in 2..=6 {
        assert_eq!(
            state.buttons[&id],
            ButtonAction::Scroll(gearwise_core::model::ScrollDirection::Down)
        );
    }
    log.lock().unwrap().clear();
    for invalid in [
        Setting::Button {
            id: 1,
            action: ButtonAction::Disabled,
        },
        Setting::Button {
            id: 7,
            action: ButtonAction::Disabled,
        },
        Setting::ReportRate(8000),
        Setting::DpiColor {
            stage: 8,
            color: Rgb { r: 1, g: 2, b: 3 },
        },
    ] {
        assert!(d.apply(&invalid).is_err());
    }
    assert!(log.lock().unwrap().is_empty());
}

#[test]
fn x1lite_performance_has_exact_native_rows_and_preserves_every_unselected_byte() {
    use gearwise_core::tuning::MouseTuning;
    let (mut d, log) = memory_device(0x113e).unwrap();
    let (mut peer, _) = memory_device(0x113e).unwrap();
    let peer_before = peer.read_state().unwrap();
    let before = d.read_blob(0, 768 + 12 * 384).unwrap();
    let patch = MouseTuning {
        debounce_ms: Some(20),
        motion_sync: Some(false),
        angle_snapping: Some(true),
        ripple_control: Some(false),
        led_idle_seconds: Some(1800),
        sensor_sleep_enabled: Some(false),
        sensor_sleep_seconds: Some(300),
        lift_off_microns: Some(2000),
        ..Default::default()
    };
    log.lock().unwrap().clear();
    d.apply(&Setting::Tuning(patch.clone())).unwrap();
    let writes = log
        .lock()
        .unwrap()
        .iter()
        .filter_map(|r| {
            if let gearwise_core::transport::MockRecord::Write(p) = r {
                if p[1] == 7 {
                    Some(p.clone())
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        writes,
        vec![
            vec![8, 7, 0, 0, 10, 2, 2, 83, 0, 0, 0, 0, 0, 0, 0, 0, 229],
            vec![8, 7, 0, 0, 169, 10, 20, 65, 0, 85, 180, 161, 1, 84, 0, 85, 234],
            vec![8, 7, 0, 0, 181, 6, 0, 85, 30, 55, 2, 83, 0, 0, 0, 0, 140]
        ]
    );

    assert_eq!(d.read(10, 2).unwrap(), [2, 83]);
    assert_eq!(
        d.read(169, 10).unwrap(),
        [20, 65, 0, 85, 180, 161, 1, 84, 0, 85]
    );
    assert_eq!(d.read(181, 6).unwrap(), [0, 85, 30, 55, 2, 83]);
    let actual = d.read_state().unwrap().tuning.unwrap();
    assert_eq!(actual, patch);
    let after = d.read_blob(0, before.len()).unwrap();
    for at in 0..before.len() {
        if !(10..12).contains(&at) && !(169..179).contains(&at) && !(181..185).contains(&at) {
            assert_eq!(after[at], before[at], "untouched {at}");
        }
    }
    assert_eq!(peer.read_state().unwrap(), peer_before);
    let info = HidInterfaceInfo {
        vendor_id: 0x373b,
        product_id: 0x113e,
        usage_page: 0xff04,
        usage: 2,
        ..Default::default()
    };
    let mut reopened = Atk::open(d.io, &info).unwrap();
    assert_eq!(reopened.read_state().unwrap().tuning.unwrap(), patch);
}
#[test]
fn x1lite_performance_rejects_unsupported_values_without_any_transfer() {
    use gearwise_core::tuning::MouseTuning;
    let (mut d, log) = memory_device(0x113e).unwrap();
    let caps = d.capabilities().tuning.as_ref().unwrap();
    assert_eq!(caps.debounce_ms, [0, 1, 2, 4, 8, 15, 20]);
    assert_eq!(caps.lift_off_microns, [1000, 2000]);
    for patch in [
        MouseTuning {
            debounce_ms: Some(3),
            ..Default::default()
        },
        MouseTuning {
            led_idle_seconds: Some(90),
            ..Default::default()
        },
        MouseTuning {
            sensor_sleep_seconds: Some(90),
            ..Default::default()
        },
        MouseTuning {
            lift_off_microns: Some(700),
            ..Default::default()
        },
    ] {
        log.lock().unwrap().clear();
        assert!(d.apply(&Setting::Tuning(patch)).is_err());
        assert!(log.lock().unwrap().is_empty());
    }
}

struct TuningFault {
    inner: Box<dyn HidTransport>,
    sets: usize,
    nth: usize,
    after: bool,
}
impl HidTransport for TuningFault {
    fn write(&mut self, p: &[u8]) -> Result<usize> {
        if p[1] == 7 {
            self.sets += 1;
            if self.sets == self.nth {
                if self.after {
                    self.inner.write(p)?;
                }
                return Err(Error::invalid("synthetic X1 Lite performance failure"));
            }
        }
        self.inner.write(p)
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
#[test]
fn x1lite_three_performance_stages_before_and_after_effect_restore_original_eeprom() {
    use gearwise_core::tuning::MouseTuning;
    for nth in 1..=3 {
        for after in [false, true] {
            let (mut d, _) = memory_device(0x113e).unwrap();
            let before = d.read_blob(0, 768 + 12 * 384).unwrap();
            let patch = MouseTuning {
                lift_off_microns: Some(2000),
                debounce_ms: Some(20),
                motion_sync: Some(false),
                sensor_sleep_seconds: Some(300),
                ..Default::default()
            };
            d.io = Box::new(TuningFault {
                inner: d.io,
                sets: 0,
                nth,
                after,
            });
            let error = d.apply(&Setting::Tuning(patch)).unwrap_err().to_string();
            assert!(
                error.contains("performance original rows restoration: confirmed"),
                "{nth} {after}: {error}"
            );
            assert_eq!(d.read_blob(0, before.len()).unwrap(), before);
        }
    }
}
