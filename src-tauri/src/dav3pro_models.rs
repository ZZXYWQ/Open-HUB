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
        assert!(v.capabilities.lighting.is_empty() && v.capabilities.buttons.is_empty());
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
                id: 2,
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
                .filter(|s| matches!(s, Setting::Button { .. } | Setting::Lighting { .. }))
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
        for s in &plan.settings {
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
        assert_eq!(simulate(plan).unwrap()["hardware_tested"], false);
    }
}
