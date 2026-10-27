use super::*;
use gearwise_core::model::ScrollDirection;
use gearwise_core::profile::{Macro, MacroMode, MacroStep};
fn views() -> Vec<View> {
    crate::atk::models()
        .iter()
        .filter(|s| crate::atk::horizontal_scroll_supported(s.vendor, s.product))
        .map(|s| {
            let (mut d, _) = crate::atk::simulated(s.product).unwrap();
            let m = Macro {
                id: "scroll".into(),
                name: "原生滚轮宏".into(),
                mode: MacroMode::Once,
                steps: vec![
                    MacroStep::Scroll(ScrollDirection::Up),
                    MacroStep::Delay(37),
                    MacroStep::Scroll(ScrollDirection::Down),
                ],
            };
            d.set_macro_definitions(&[m]).unwrap();
            d.apply(&Setting::Button {
                id: 4,
                action: ButtonAction::Macro("scroll".into()),
            })
            .unwrap();
            view(d.as_mut()).unwrap()
        })
        .collect()
}
#[test]
#[ignore = "exports production scroll macro DTOs for built UI"]
fn native_compx_scroll_fixture_export() {
    let values = views();
    assert_eq!(values.len(), 9);
    std::fs::write(
        std::env::var_os("OPEN_HUB_COMPX_SCROLL_FIXTURE").unwrap(),
        serde_json::to_vec_pretty(&values).unwrap(),
    )
    .unwrap();
}
#[test]
#[ignore = "requires actual UI saved scroll profiles"]
fn browser_scroll_profiles_store_import_compile_and_native_replay() {
    let mut profiles: Vec<crate::profiles::Profile> = serde_json::from_slice(
        &std::fs::read(std::env::var_os("OPEN_HUB_COMPX_SCROLL_UI_PROFILES").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(profiles.len(), 9);
    for p in &mut profiles {if p.id.is_empty(){p.id=crate::profiles::new_id();}}
    let mut store = crate::profiles::ProfileStore::default();
    crate::profile_transfer::merge(&mut store, profiles.clone()).unwrap();
    for p in profiles {
        let plan = p.multi.unwrap();
        let expected = vec![
            MacroStep::Scroll(ScrollDirection::Down),
            MacroStep::Delay(37),
            MacroStep::Scroll(ScrollDirection::Down),
            MacroStep::Scroll(ScrollDirection::Up),
        ];
        assert_eq!(plan.macros[0].steps, expected);
        assert_eq!(plan.macros[0].mode, MacroMode::Repeat(5));
        let c = compile(&plan).unwrap();
        assert!(!c.software_input);
        let (mut d, _) = crate::atk::simulated(plan.product).unwrap();
        d.set_macro_definitions(&plan.macros).unwrap();
        for s in &c.settings {
            d.apply(s).unwrap();
        }
        let state = d.read_state().unwrap();
        let ButtonAction::Macro(id) = &state.buttons[&4] else {
            panic!("binding lost")
        };
        let m = d
            .macro_definitions()
            .into_iter()
            .find(|m| &m.id == id)
            .unwrap();
        assert_eq!(m.steps, expected);
        assert_eq!(m.mode, MacroMode::Repeat(5));
    }
}
