use super::*;
use gearwise_core::model::MouseButton;
fn native_views() -> Vec<View> {
    crate::atk::models()
        .iter()
        .filter(|m| crate::atk::horizontal_scroll_supported(m.vendor, m.product))
        .map(|m| {
            let (mut d, _) = crate::atk::simulated(m.product).unwrap();
            d.apply(&Setting::Button {
                id: 4,
                action: ButtonAction::Mouse(MouseButton::WheelLeft),
            })
            .unwrap();
            view(d.as_mut()).unwrap()
        })
        .collect()
}
#[test]
fn exact_horizontal_profiles_compile_without_host_input_and_roundtrip_known_snapshot() {
    for v in native_views() {
        let mut p = template_plan(&v).unwrap();
        p.settings
            .retain(|s| !matches!(s, Setting::Button { id: 2 | 4, .. }));
        p.settings.extend([
            Setting::Button {
                id: 2,
                action: ButtonAction::Mouse(MouseButton::WheelRight),
            },
            Setting::Button {
                id: 4,
                action: ButtonAction::Mouse(MouseButton::WheelLeft),
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
            ButtonAction::Mouse(MouseButton::WheelRight)
        );
        assert_eq!(
            state.buttons[&4],
            ButtonAction::Mouse(MouseButton::WheelLeft)
        );
    }
}
#[test]
fn pulsar_profiles_and_unknown_button_cannot_bypass_horizontal_model_gate() {
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
                action: ButtonAction::Mouse(MouseButton::WheelLeft),
            }],
            macros: vec![],
            source: None,
        };
        assert!(validate(&p).is_err());
    }
}
#[test]
fn export_compx_horizontal_ui_fixture() {
    let views = native_views();
    assert_eq!(views.len(), 9);
    if let Some(path) = std::env::var_os("OPEN_HUB_COMPX_HORIZONTAL_FIXTURE") {
        std::fs::write(path, serde_json::to_vec_pretty(&views).unwrap()).unwrap();
    }
    if let Some(path)=std::env::var_os("OPEN_HUB_COMPX_HORIZONTAL_NEGATIVE_FIXTURE"){
        let negative=[(0x3554,0xf507),(0x3554,0xf508),(0x03f0,0x048e)].into_iter().map(|(v,p)|{let(mut d,_)=super::simulated(v,p).unwrap();view(d.as_mut()).unwrap()}).collect::<Vec<_>>();
        std::fs::write(path,serde_json::to_vec_pretty(&negative).unwrap()).unwrap();
    }
}
#[test]
#[ignore = "requires actual built UI profiles through OPEN_HUB_COMPX_HORIZONTAL_PROFILES"]
fn browser_compx_horizontal_profiles_store_import_and_replay() {
    let profiles: Vec<crate::profiles::Profile> = serde_json::from_slice(
        &std::fs::read(std::env::var("OPEN_HUB_COMPX_HORIZONTAL_PROFILES").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(profiles.len(), 9);
    let mut store = crate::profiles::ProfileStore::default();
    let mut expected = profiles.clone();
    for p in &mut expected {
        if p.id.is_empty() {
            p.id = crate::profiles::new_id();
        }
    }
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
            ButtonAction::Mouse(MouseButton::WheelRight)
        );
        assert_eq!(
            state.buttons[&4],
            ButtonAction::Mouse(MouseButton::WheelLeft)
        );
    }
}
