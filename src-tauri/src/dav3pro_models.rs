//! Partial exact model integration over synthetic transport. No OS input or HID.
use super::*;
use gearwise_core::{
    model::{ButtonAction, DpiStage, DpiState, LightingEffect, Modifiers, ZoneState},
    profile::{Macro, MacroMode, MacroStep},
    tuning::MouseTuning,
    Setting,
};
fn plan(pid: u16) -> Plan {
    Plan {
        vendor: 0x1532,
        product: pid,
        settings: vec![
            Setting::Dpi(DpiState {
                stages: vec![DpiStage::uniform(800)],
                active: 0,
            }),
            Setting::ReportRate(125),
            Setting::Tuning(MouseTuning {
                device_sleep_seconds: Some(60),
                low_power_percent: Some(50),
                ..Default::default()
            }),
        ],
        macros: vec![],
        source: None,
    }
}
#[test]
fn dav3pro_two_transport_views_report_actual_known_fields_without_rgb_or_fake_buttons() {
    let mut out = vec![];
    let mut staged = vec![];
    for pid in [0x00b6, 0x00b7] {
        let (mut d, _) = gearwise_razer::simulated_dav3pro(pid).unwrap();
        let mut complete = view(d.as_mut()).unwrap();
        assert_eq!(complete.state.dpi.as_ref().unwrap().stages.len(),5);
        assert_eq!(complete.state.dpi.as_ref().unwrap().active,2);
        complete.descriptor.simulated=true;staged.push(complete);
        let (mut d, _) = gearwise_razer::simulated_dav3pro_current(pid).unwrap();
        let mut v = view(d.as_mut()).unwrap();
        let c = v.capabilities.dpi.as_ref().unwrap();
        assert_eq!((c.min, c.max, c.max_stages), (100, 30000, 1));
        assert_eq!(v.state.dpi.as_ref().unwrap().stages[0].x, 1600);
        assert!(v.capabilities.lighting.is_empty());
        assert_eq!(v.capabilities.buttons.len(),16);
        assert_eq!(v.capabilities.buttons.iter().filter(|b|b.remappable).count(),16);
        assert_eq!(v.capabilities.report_rates, [125, 500, 1000]);
        let c = v.capabilities.tuning.as_ref().unwrap();
        assert_eq!(c.device_sleep_seconds.len(), 15);
        assert_eq!(c.low_power_percent.len(), 20);
        let state = v.state.tuning.as_ref().unwrap();
        assert_eq!(
            (state.device_sleep_seconds, state.low_power_percent),
            (Some(300), Some(30))
        );
        assert!(!v.hardware_tested);
        v.descriptor.simulated = true;
        out.push(v);
    }
    if let Some(path) = std::env::var_os("OPEN_HUB_DAV3PRO_STAGES_FIXTURE") {
        std::fs::write(path,serde_json::to_vec_pretty(&staged).unwrap()).unwrap();
    }
    if let Some(path) = std::env::var_os("OPEN_HUB_DAV3PRO_FIXTURE") {
        std::fs::write(path, serde_json::to_vec_pretty(&out).unwrap()).unwrap();
    }
}
#[test]
fn dav3pro_full_known_dpi_rate_and_power_plan_executes_production_codec_and_exact_persistent_bytes()
{
    for pid in [0xb6, 0xb7] {
        let p = plan(pid);
        let (mut d, _) = gearwise_razer::simulated_dav3pro(pid).unwrap();
        let executable = template_plan(&view(d.as_mut()).unwrap()).unwrap();
        assert!(executable.settings.iter().any(|s|matches!(s,Setting::Tuning(t) if t.device_sleep_seconds==Some(60)&&t.low_power_percent==Some(5))));
        assert_eq!(simulate(&executable).unwrap()["hardware_tested"],false);
        validate(&p).unwrap();
        let result = simulate(&p).unwrap();
        assert_eq!(result["hardware_tested"], false);
        let packets = result["packets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                v.as_str()
                    .unwrap()
                    .split_whitespace()
                    .map(|b| u8::from_str_radix(b, 16).unwrap())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        for (class, command, args) in [
            (4, 5, vec![1, 3, 0x20, 3, 0x20, 0, 0]),
            (0, 5, vec![8]),
            (7, 3, vec![0, 60]),
            (7, 1, vec![128, 0]),
        ] {
            assert_eq!(
                packets
                    .iter()
                    .filter(|p| p[7] == class && p[8] == command && p[9..9 + args.len()] == args)
                    .count(),
                1
            );
        }
        assert_eq!(result["software_macros"].as_array().unwrap().len(), 0);
    }
}
#[test]
fn dav3pro_unverified_button_macro_rgb_wrong_power_or_excessive_stage_count_are_not_silently_applied(
) {
    for pid in [0xb6, 0xb7] {
        for bad in [
            Setting::Dpi(DpiState {
                stages: vec![DpiStage::uniform(30001)],
                active: 0,
            }),
            Setting::ReportRate(2000),
            Setting::Button {
                id: 6,
                action: ButtonAction::Key {
                    usage: 4,
                    mods: Modifiers(0),
                },
            },
            Setting::Lighting {
                zone: 0,
                state: ZoneState {
                    effect: LightingEffect::Off,
                    brightness: 100,
                },
            },
            Setting::Tuning(MouseTuning {
                sensor_sleep_seconds: Some(60),
                ..Default::default()
            }),
            Setting::Tuning(MouseTuning {
                low_power_percent: Some(4),
                ..Default::default()
            }),
        ] {
            let mut p = plan(pid);
            p.settings = vec![bad];
            assert!(validate(&p).is_err());
        }
        let mut p = plan(pid);
        p.macros.push(Macro {
            id: "m".into(),
            name: "not enabled".into(),
            mode: MacroMode::Once,
            steps: vec![MacroStep::KeyDown(4), MacroStep::KeyUp(4)],
        });
        assert!(validate(&p).is_err());
        let mut p = plan(pid);
        p.source = Some(crate::host_macros::Source {
            vendor: p.vendor,
            product: pid,
            path: "unverified shared receiver".into(),
        });
        assert!(validate(&p).is_err());
        let mut p = plan(pid);
        p.settings[0] = Setting::Dpi(DpiState {
            stages: vec![DpiStage::uniform(800); 6],
            active: 0,
        });
        assert!(simulate(&p).is_err());
    }
}
#[test]
#[ignore = "requires current native DeathAdder power UI profiles"]
fn browser_dav3pro_known_plans_roundtrip_store_and_native_power_readback() {
    let key_path=std::path::PathBuf::from(std::env::var_os("OPEN_HUB_DAV3PRO_UI_PROFILES").unwrap()).with_file_name("dav3pro-key-picker-usages.json");
    let offered:Vec<u8>=serde_json::from_slice(&std::fs::read(key_path).unwrap()).unwrap();
    assert_eq!(offered.len(),115);assert_eq!(offered.iter().copied().collect::<std::collections::BTreeSet<_>>().len(),115);
    assert!(offered.iter().all(|usage|(4..=0xa4).contains(usage)));
    assert!(offered.contains(&0x39)&&offered.contains(&0x47));
    for pid in [0xb6,0xb7]{
        let(mut device,_)=gearwise_razer::simulated_dav3pro(pid).unwrap();
        for usage in &offered{
            for id in [2,258]{
                let wanted=ButtonAction::Key{usage:*usage,mods:Modifiers(0x81)};
                let p=Plan{vendor:0x1532,product:pid,settings:vec![Setting::Button{id,action:wanted.clone()}],macros:vec![],source:None};
                validate(&p).unwrap();for setting in compile(&p).unwrap().settings{device.apply(&setting).unwrap();}
                assert_eq!(device.read_state().unwrap().buttons.get(&id),Some(&wanted));
            }
        }
    }
    let mut profiles: Vec<crate::profiles::Profile> = serde_json::from_slice(
        &std::fs::read(std::env::var_os("OPEN_HUB_DAV3PRO_UI_PROFILES").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(profiles.len(), 4);
    for p in &mut profiles {
        p.id = crate::profiles::new_id();
    }
    let mut store = crate::profiles::ProfileStore::default();
    crate::profile_transfer::merge(&mut store, profiles.clone()).unwrap();
    let file = std::env::temp_dir().join(format!("dav3pro-ui-{}.json", crate::profiles::new_id()));
    store.save(&file).unwrap();
    let loaded = crate::profiles::ProfileStore::load(&file);
    std::fs::remove_file(file).unwrap();
    for source in profiles {
        let p = loaded.get(&source.id).unwrap();
        let plan = p.multi.as_ref().unwrap();
        assert!(matches!(plan.product, 0xb6 | 0xb7));
        assert_eq!(
            plan.settings
                .iter()
                .filter(|s| matches!(s, Setting::Lighting { .. }))
                .count(),
            0
        );
        let tuning = plan
            .settings
            .iter()
            .find_map(|s| {
                if let Setting::Tuning(t) = s {
                    Some(t)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(
            (tuning.device_sleep_seconds, tuning.low_power_percent),
            (Some(900), Some(5))
        );
        let desired=plan.settings.iter().find_map(|s|if let Setting::Dpi(d)=s{Some(d)}else{None}).unwrap();
        let (mut d, _) = if desired.stages.len()==1 {gearwise_razer::simulated_dav3pro_current(plan.product).unwrap()}else{gearwise_razer::simulated_dav3pro(plan.product).unwrap()};
        view(d.as_mut()).unwrap();
        for s in &compile(plan).unwrap().settings {
            d.apply(s).unwrap();
        }
        let state = d.read_state().unwrap();
        assert_eq!(
            state.tuning.as_ref().unwrap().device_sleep_seconds,
            Some(900)
        );
        assert_eq!(state.tuning.as_ref().unwrap().low_power_percent, Some(5));
        assert_eq!(state.dpi.as_ref().unwrap().stages[0].x, 3200);
        assert_eq!(state.dpi.as_ref(),Some(desired));
        if desired.stages.len()>1{assert_eq!(desired.stages.len(),5);assert_eq!(desired.active,4);assert_eq!(desired.stages[4].x,30000);}
        assert_eq!(state.report_rate, Some(500));
        let actions=plan.settings.iter().filter_map(|s|if let Setting::Button{id,action}=s{Some((*id,action))}else{None}).collect::<Vec<_>>();assert_eq!(actions.len(),16);
        for(id,wanted)in actions{assert_eq!(state.buttons.get(&id),Some(wanted));}
        assert_eq!(simulate(plan).unwrap()["hardware_tested"], false);
    }
}

#[test]
fn native_hypershift_is_scoped_to_exact_deathadder_and_never_compiles_to_a_host_trigger(){
    for v in templates().unwrap(){
        let mut p=template_plan(&v).unwrap();
        p.settings.retain(|s|!matches!(s,Setting::Button{id:4|260,..}));
        p.settings.push(Setting::Button{id:4,action:ButtonAction::Hypershift});
        let exact=(p.vendor,p.product)==(0x1532,0xb6)||(p.vendor,p.product)==(0x1532,0xb7);
        if exact{
            validate(&p).unwrap();let compiled=compile(&p).unwrap();assert!(compiled.bindings.is_empty());assert!(!compiled.software_input);
            assert!(compiled.settings.iter().any(|s|matches!(s,Setting::Button{id:4,action:ButtonAction::Hypershift})));
            assert_eq!(simulate(&p).unwrap()["hardware_tested"],false);
            for id in [9,10,96,512,65535]{let mut bad=p.clone();bad.settings.retain(|s|!matches!(s,Setting::Button{..}));bad.settings.push(Setting::Button{id,action:ButtonAction::Hypershift});assert!(validate(&bad).is_err());}
        }else{assert!(validate(&p).is_err(),"Hypershift leaked to {}",v.descriptor.name);}
    }
}

#[test]
fn dav3pro_click_transfer_is_compiled_before_primary_remap_and_rejects_clickless_complete_table(){
    for pid in [0xb6,0xb7]{
        let mut p=plan(pid);p.settings=vec![Setting::Button{id:1,action:ButtonAction::Key{usage:6,mods:Modifiers(0x81)}},Setting::Button{id:5,action:ButtonAction::Mouse(gearwise_core::model::MouseButton::Left)}];
        validate(&p).unwrap();let compiled=compile(&p).unwrap();assert!(matches!(compiled.settings.first(),Some(Setting::Button{id:5,..})));
        let(mut d,_)=gearwise_razer::simulated_dav3pro(pid).unwrap();for s in &compiled.settings{d.apply(s).unwrap();}
        let state=d.read_state().unwrap();assert_eq!(state.buttons.get(&1),Some(&ButtonAction::Key{usage:6,mods:Modifiers(0x81)}));assert_eq!(state.buttons.get(&5),Some(&ButtonAction::Mouse(gearwise_core::model::MouseButton::Left)));
        p.settings=gearwise_razer::dav3pro_buttons::BUTTONS.into_iter().map(|(id,_)|Setting::Button{id,action:ButtonAction::Disabled}).collect();
        assert!(compile(&p).err().unwrap().contains("左键"));
        p.settings=vec![Setting::Button{id:4,action:ButtonAction::Hypershift},Setting::Button{id:260,action:ButtonAction::Disabled}];assert!(compile(&p).err().unwrap().contains("第二层"));
    }
}


#[test]
fn scroll_outputs_and_reverse_cycle_follow_exact_native_model_allowlists_without_host_input(){
    use gearwise_core::model::ScrollDirection;
    for v in templates().unwrap(){
        for action in [ButtonAction::Scroll(ScrollDirection::Up),ButtonAction::Scroll(ScrollDirection::Down),ButtonAction::Dpi(DpiFunction::CycleDown),ButtonAction::Dpi(DpiFunction::ShiftXY{x:800,y:1600})]{
            let p=Plan{vendor:v.descriptor.vendor_id,product:v.descriptor.product_id,settings:vec![Setting::Button{id:2,action:action.clone()}],macros:vec![],source:None};
            let exact=p.vendor==0x1532&&matches!(p.product,0xb6|0xb7);
            if exact{
                validate(&p).unwrap();let compiled=compile(&p).unwrap();assert!(compiled.bindings.is_empty()&&compiled.input.keys.is_empty()&&compiled.dpi_actions.is_empty());assert!(!compiled.software_input);
                let(mut d,_)=gearwise_razer::simulated_dav3pro(p.product).unwrap();for s in &compiled.settings{d.apply(s).unwrap();}assert_eq!(d.read_state().unwrap().buttons.get(&2),Some(&action));
                assert_eq!(simulate(&p).unwrap()["hardware_tested"],false);
            }else if matches!(action,ButtonAction::Scroll(_))&&crate::atk::vertical_scroll_supported(p.vendor,p.product){
                // COMPX class11 now has its own pinned OEM evidence; this does not
                // enable reverse DPI or independent clutches on those models.
                validate(&p).unwrap();let compiled=compile(&p).unwrap();assert!(compiled.bindings.is_empty()&&!compiled.software_input);
                let(mut d,_)=crate::atk::simulated(p.product).unwrap();for setting in &compiled.settings{d.apply(setting).unwrap();}assert_eq!(d.read_state().unwrap().buttons.get(&2),Some(&action));
            }else if matches!(action,ButtonAction::Scroll(_))&&p.vendor==0x1038&&gearwise_steelseries::rival3::supported(p.product){
                validate(&p).unwrap();let compiled=compile(&p).unwrap();assert!(compiled.bindings.is_empty()&&!compiled.software_input);
                let(mut d,_)=gearwise_steelseries::simulated_model(p.product).unwrap();for s in &compiled.settings{d.apply(s).unwrap();}assert_eq!(d.read_state().unwrap().buttons.get(&2),Some(&action));
            }else{assert!(validate(&p).is_err(),"new native action leaked to {}: {action:?}",v.descriptor.name);}
        }
    }
}
