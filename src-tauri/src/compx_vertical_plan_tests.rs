use super::*;
use gearwise_core::model::ScrollDirection;
fn vertical_views() -> Vec<View> {
    crate::atk::models()
        .iter()
        .filter(|m| crate::atk::vertical_scroll_supported(m.vendor, m.product))
        .map(|m| {
            let (mut d, _) = crate::atk::simulated(m.product).unwrap();
            d.apply(&Setting::Button {
                id: 4,
                action: ButtonAction::Mouse(MouseButton::WheelLeft),
            })
            .unwrap();
            d.apply(&Setting::Button {
                id: 5,
                action: ButtonAction::Scroll(ScrollDirection::Up),
            })
            .unwrap();
            view(d.as_mut()).unwrap()
        })
        .collect()
}
#[test]
fn vertical_profiles_compile_to_native_writes_without_software_input() {
    for v in vertical_views() {
        let mut p = template_plan(&v).unwrap();
        p.settings.extend([
            Setting::Button {
                id: 2,
                action: ButtonAction::Scroll(ScrollDirection::Down),
            },
            Setting::Button {
                id: 4,
                action: ButtonAction::Mouse(MouseButton::WheelLeft),
            },
            Setting::Button {
                id: 5,
                action: ButtonAction::Scroll(ScrollDirection::Up),
            },
        ]);
        let c = compile(&p).unwrap();
        assert!(!c.software_input && c.bindings.is_empty());
        let (mut d, _) = crate::atk::simulated(p.product).unwrap();
        for s in &c.settings {
            d.apply(s).unwrap();
        }
        let state = d.read_state().unwrap();
        assert_eq!(
            state.buttons[&2],
            ButtonAction::Scroll(ScrollDirection::Down)
        );
        assert_eq!(
            state.buttons[&4],
            ButtonAction::Mouse(MouseButton::WheelLeft)
        );
        assert_eq!(state.buttons[&5], ButtonAction::Scroll(ScrollDirection::Up));
    }
}
#[test]
fn vertical_unsupported_profiles_cannot_bypass_driver_gate() {
    for (vendor, product, id) in [
        (0x3554, 0xf507, 4),
        (0x3554, 0xf508, 4),
        (0x373b, 0x1017, 1),
        (0x373b, 0x1017, 7),
    ] {
        let p = Plan {
            vendor,
            product,
            settings: vec![Setting::Button {
                id,
                action: ButtonAction::Scroll(ScrollDirection::Up),
            }],
            macros: vec![],
            source: None,
        };
        assert!(validate(&p).is_err());
    }
}
#[test]
fn export_compx_vertical_ui_fixture() {
    let views = vertical_views();
    assert_eq!(views.len(), crate::atk::native_test_count());
    if let Some(path) = std::env::var_os("OPEN_HUB_COMPX_VERTICAL_FIXTURE") {
        std::fs::write(path, serde_json::to_vec_pretty(&views).unwrap()).unwrap();
    }
}
#[test]
#[ignore = "requires built UI saved profiles through OPEN_HUB_COMPX_VERTICAL_PROFILES"]
fn browser_compx_vertical_profiles_store_import_and_replay() {
    let profiles: Vec<crate::profiles::Profile> = serde_json::from_slice(
        &std::fs::read(std::env::var("OPEN_HUB_COMPX_VERTICAL_PROFILES").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(profiles.len(), 9);
    let mut expected = profiles.clone();
    for p in &mut expected {
        if p.id.is_empty() {
            p.id = crate::profiles::new_id();
        }
    }
    let mut store = crate::profiles::ProfileStore::default();
    crate::profile_transfer::merge(&mut store, expected.clone()).unwrap();
    for p in expected {
        let plan = p.multi.as_ref().unwrap();
        let c = compile(plan).unwrap();
        assert!(!c.software_input && c.bindings.is_empty());
        let (mut d, _) = crate::atk::simulated(plan.product).unwrap();
        d.set_macro_definitions(&plan.macros).unwrap();
        for s in &c.settings {
            d.apply(s).unwrap();
        }
        let state = d.read_state().unwrap();
        assert_eq!(
            state.buttons[&2],
            ButtonAction::Scroll(ScrollDirection::Down)
        );
        assert_eq!(
            state.buttons[&4],
            ButtonAction::Mouse(MouseButton::WheelLeft)
        );
        assert_eq!(state.buttons[&5], ButtonAction::Scroll(ScrollDirection::Up));
    }
}
