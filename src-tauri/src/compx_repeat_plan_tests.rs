use super::*;
use gearwise_core::profile::{Macro, MacroMode, MacroStep};
fn definition(count: u8) -> Macro {
    Macro {
        id: "counted".into(),
        name: "Counted".into(),
        mode: MacroMode::Repeat(count),
        steps: vec![
            MacroStep::KeyDown(4),
            MacroStep::Delay(50),
            MacroStep::KeyUp(4),
            MacroStep::Delay(1),
        ],
    }
}
#[test]
fn finite_repeat_macro_plans_are_native_and_invalid_counts_reject() {
    for spec in crate::atk::models()
        .iter()
        .filter(|m| m.native_macros && m.native_buttons)
    {
        for count in [2, 10, 252] {
            let m = definition(count);
            let plan = Plan {
                vendor: spec.vendor,
                product: spec.product,
                settings: vec![Setting::Button {
                    id: 4,
                    action: ButtonAction::Macro(m.id.clone()),
                }],
                macros: vec![m],
                source: None,
            };
            let c = compile(&plan).unwrap();
            assert!(!c.software_input && c.bindings.is_empty());
            let (mut d, _) = crate::atk::simulated(spec.product).unwrap();
            d.set_macro_definitions(&plan.macros).unwrap();
            for s in &c.settings {
                d.apply(s).unwrap();
            }
            let state = d.read_state().unwrap();
            let ButtonAction::Macro(id) = &state.buttons[&4] else {
                panic!("counted native macro lost")
            };
            assert_eq!(
                d.macro_definitions()
                    .iter()
                    .find(|m| &m.id == id)
                    .unwrap()
                    .mode,
                MacroMode::Repeat(count)
            );
        }
    }
    for count in [0, 1, 253, 254, 255] {
        assert!(crate::host_macros::validate(&definition(count)).is_err());
    }
}
#[test]
fn export_compx_repeat_ui_fixture() {
    let views = crate::atk::models()
        .iter()
        .filter(|m| m.native_macros && m.native_buttons)
        .map(|spec| {
            let (mut d, _) = crate::atk::simulated(spec.product).unwrap();
            let m = definition(3);
            d.set_macro_definitions(&[m.clone()]).unwrap();
            d.apply(&Setting::Button {
                id: 4,
                action: ButtonAction::Macro(m.id),
            })
            .unwrap();
            view(d.as_mut()).unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(views.len(), 9);
    if let Some(path) = std::env::var_os("OPEN_HUB_COMPX_REPEAT_FIXTURE") {
        std::fs::write(path, serde_json::to_vec_pretty(&views).unwrap()).unwrap();
    }
}
#[test]
#[ignore = "requires built UI profiles via OPEN_HUB_COMPX_REPEAT_PROFILES"]
fn browser_compx_repeat_profiles_import_and_replay() {
    let mut profiles: Vec<crate::profiles::Profile> = serde_json::from_slice(
        &std::fs::read(std::env::var("OPEN_HUB_COMPX_REPEAT_PROFILES").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(profiles.len(), 27);
    for p in &mut profiles {
        if p.id.is_empty() {
            p.id = crate::profiles::new_id();
        }
    }
    let mut store = crate::profiles::ProfileStore::default();
    crate::profile_transfer::merge(&mut store, profiles.clone()).unwrap();
    for p in profiles {
        let plan = p.multi.unwrap();
        let expected = plan.macros[0].mode;
        let c = compile(&plan).unwrap();
        assert!(!c.software_input && c.bindings.is_empty());
        let (mut d, _) = crate::atk::simulated(plan.product).unwrap();
        d.set_macro_definitions(&plan.macros).unwrap();
        for s in &c.settings {
            d.apply(s).unwrap();
        }
        let state = d.read_state().unwrap();
        let ButtonAction::Macro(id) = &state.buttons[&4] else {
            panic!("finite macro binding lost")
        };
        assert_eq!(
            d.macro_definitions()
                .iter()
                .find(|m| &m.id == id)
                .unwrap()
                .mode,
            expected
        );
    }
}
