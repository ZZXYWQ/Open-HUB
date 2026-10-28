use super::*;
fn views() -> Vec<View> {
    crate::atk::models()
        .iter()
        .filter(|m| crate::atk::profile_cycle_supported(m.vendor, m.product))
        .map(|m| {
            let (mut d, _) = crate::atk::simulated(m.product).unwrap();
            d.apply(&Setting::Button {
                id: 4,
                action: ButtonAction::ProfileSelect(2),
            })
            .unwrap();
            view(d.as_mut()).unwrap()
        })
        .collect()
}
#[test]
fn direct_profile_plans_compile_natively_and_reject_wrong_index_model_or_key() {
    for v in views() {
        let p = Plan {
            vendor: v.descriptor.vendor_id,
            product: v.descriptor.product_id,
            settings: (0..4)
                .map(|i| Setting::Button {
                    id: u16::from(i) + 2,
                    action: ButtonAction::ProfileSelect(i),
                })
                .collect(),
            macros: vec![],
            source: None,
        };
        let c = compile(&p).unwrap();
        assert!(c.bindings.is_empty() && !c.software_input);
        let (mut d, _) = crate::atk::simulated(p.product).unwrap();
        for s in &c.settings {
            d.apply(s).unwrap();
        }
        for i in 0..4 {
            assert_eq!(
                d.read_state().unwrap().buttons[&(u16::from(i) + 2)],
                ButtonAction::ProfileSelect(i)
            );
        }
    }
    for (v, p, id, index) in [
        (0x3554, 0xf507, 4, 0),
        (0x3554, 0xf508, 4, 3),
        (0x373b, 0x1017, 1, 0),
        (0x373b, 0x1017, 7, 0),
        (0x373b, 0x1017, 4, 4),
    ] {
        assert!(validate(&Plan {
            vendor: v,
            product: p,
            settings: vec![Setting::Button {
                id,
                action: ButtonAction::ProfileSelect(index)
            }],
            macros: vec![],
            source: None
        })
        .is_err());
    }
}
#[test]
fn export_compx_profile_select_ui_fixture() {
    let v = views();
    assert_eq!(v.len(), crate::atk::native_test_count());
    if let Some(path) = std::env::var_os("OPEN_HUB_COMPX_PROFILE_SELECT_FIXTURE") {
        std::fs::write(path, serde_json::to_vec_pretty(&v).unwrap()).unwrap();
    }
}
#[test]
#[ignore = "requires built UI profiles via OPEN_HUB_COMPX_PROFILE_SELECT_PROFILES"]
fn browser_compx_profile_select_profiles_import_and_replay() {
    let mut profiles: Vec<crate::profiles::Profile> = serde_json::from_slice(
        &std::fs::read(std::env::var("OPEN_HUB_COMPX_PROFILE_SELECT_PROFILES").unwrap()).unwrap(),
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
        for s in &c.settings {
            d.apply(s).unwrap();
        }
        let state = d.read_state().unwrap();
        for i in 0..4 {
            assert_eq!(
                state.buttons[&(u16::from(i) + 2)],
                ButtonAction::ProfileSelect(i)
            );
        }
    }
}

#[test]
fn new_direct_profile_variant_does_not_enable_other_model_families(){
    for v in templates().unwrap(){
        let vendor=v.descriptor.vendor_id;let product=v.descriptor.product_id;
        if crate::atk::profile_cycle_supported(vendor,product){continue;}
        let plan=Plan{vendor,product,settings:vec![Setting::Button{id:2,action:ButtonAction::ProfileSelect(0)}],macros:vec![],source:None};
        assert!(validate(&plan).is_err(),"direct profile leaked to {}",v.descriptor.name);
    }
}
