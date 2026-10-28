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
