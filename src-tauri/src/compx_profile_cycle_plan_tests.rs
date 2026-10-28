use super::*;
fn views() -> Vec<View> {
    crate::atk::models()
        .iter()
        .filter(|m| crate::atk::profile_cycle_supported(m.vendor, m.product))
        .map(|m| {
            let (mut d, _) = crate::atk::simulated(m.product).unwrap();
            d.apply(&Setting::Button {
                id: 4,
                action: ButtonAction::ProfileCycle,
            })
            .unwrap();
            view(d.as_mut()).unwrap()
        })
        .collect()
}
#[test]
fn profile_cycle_templates_compile_natively_and_unproved_models_or_keys_reject() {
    for v in views() {
        let p = Plan {
            vendor: v.descriptor.vendor_id,
            product: v.descriptor.product_id,
            settings: vec![Setting::Button {
                id: 4,
                action: ButtonAction::ProfileCycle,
            }],
            macros: vec![],
            source: None,
        };
        let c = compile(&p).unwrap();
        assert!(c.bindings.is_empty() && !c.software_input);
        let (mut d, _) = crate::atk::simulated(p.product).unwrap();
        for s in &c.settings {
            d.apply(s).unwrap();
        }
        assert_eq!(
            d.read_state().unwrap().buttons[&4],
            ButtonAction::ProfileCycle
        );
    }
    for (v, p, id) in [
        (0x3554, 0xf507, 4),
        (0x3554, 0xf508, 4),
        (0x373b, 0x1017, 1),
        (0x373b, 0x1017, 7),
    ] {
        assert!(validate(&Plan {
            vendor: v,
            product: p,
            settings: vec![Setting::Button {
                id,
                action: ButtonAction::ProfileCycle
            }],
            macros: vec![],
            source: None
        })
        .is_err());
    }
}
#[test]
fn export_compx_profile_cycle_ui_fixture() {
    let v = views();
    assert_eq!(v.len(), crate::atk::native_test_count());
    if let Some(path) = std::env::var_os("OPEN_HUB_COMPX_PROFILE_CYCLE_FIXTURE") {
        std::fs::write(path, serde_json::to_vec_pretty(&v).unwrap()).unwrap();
    }
}
#[test]
#[ignore = "requires built UI requests through OPEN_HUB_COMPX_PROFILE_CYCLE_PROFILES"]
fn browser_compx_profile_cycle_profiles_import_and_replay() {
    let mut profiles: Vec<crate::profiles::Profile> = serde_json::from_slice(
        &std::fs::read(std::env::var("OPEN_HUB_COMPX_PROFILE_CYCLE_PROFILES").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(profiles.len(), 9);
    for p in &mut profiles {
        if p.id.is_empty() {
            p.id = crate::profiles::new_id();
        }
    }
    let mut store = crate::profiles::ProfileStore::default();
    crate::profile_transfer::merge(&mut store, profiles.clone()).unwrap();
    for p in profiles {
        let plan = p.multi.unwrap();
        let c = compile(&plan).unwrap();
        assert!(c.bindings.is_empty() && !c.software_input);
        let (mut d, _) = crate::atk::simulated(plan.product).unwrap();
        d.set_macro_definitions(&plan.macros).unwrap();
        for s in &c.settings {
            d.apply(s).unwrap();
        }
        let state = d.read_state().unwrap();
        assert_eq!(state.buttons[&2], ButtonAction::ProfileCycle);
        assert_eq!(state.buttons[&4], ButtonAction::ProfileCycle);
    }
}
