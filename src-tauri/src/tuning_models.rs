//! Full bridge/configuration fixtures for performance controls. No hardware I/O.
use super::*;
use gearwise_core::tuning::MouseTuning;
#[test]
fn tuning_native_dtos_export_without_inventing_other_vendor_capabilities() {
    let all = templates().unwrap();
    let views = all
        .into_iter()
        .filter(|v| v.capabilities.tuning.is_some())
        .collect::<Vec<_>>();
    assert_eq!(views.len(), 11);
    let views = views.into_iter().filter(|v| matches!(v.descriptor.vendor_id, 0x373b | 0x3554)).collect::<Vec<_>>();
    assert_eq!(views.len(), 9);
    assert!(views
        .iter()
        .all(|v| !v.hardware_tested && v.descriptor.simulated && v.state.tuning.is_some()));
    if let Some(path) = std::env::var_os("OPEN_HUB_TUNING_FIXTURE") {
        std::fs::write(path, serde_json::to_vec_pretty(&views).unwrap()).unwrap();
    }
    for (v, p) in [
        (0x1532, 0x00ab),
        (0x1b1c, 0x1b2e),
        (0x1038, 0x1720),
        (0x3554, 0xf507),
    ] {
        let p = Plan {
            vendor: v,
            product: p,
            settings: vec![Setting::Tuning(MouseTuning {
                motion_sync: Some(true),
                ..Default::default()
            })],
            source: None,
            macros: vec![],
        };
        assert!(validate(&p).is_err());
    }
}
#[test]
fn tuning_sessions_keep_complete_settings_and_exact_scope() {
    let (device, _) = crate::atk::simulated(0x1115).unwrap();
    let id = device.descriptor().id.clone();
    let state = {
        let mut d = device;
        let state = d.read_state().unwrap();
        (d, state)
    };
    let (device, state) = state;
    let mut m = DeviceManager::new(
        Box::new(gearwise_core::transport::MockBackend::new(vec![], |_| {
            Err(gearwise_core::Error::Unsupported("no hardware"))
        })),
        vec![],
    );
    m.add_virtual(vec![device]);
    let service = Service::default();
    *service.0.lock().unwrap() = Some(m);
    let root = std::env::temp_dir().join(format!("tuning-session-{}", crate::profiles::new_id()));
    let runtime = crate::macro_runtime::Runtime::default();
    let patch = MouseTuning {
        debounce_ms: Some(4),
        motion_sync: Some(false),
        lift_off_microns: Some(2000),
        ..Default::default()
    };
    let p = Plan {
        vendor: 0x373b,
        product: 0x1115,
        settings: vec![
            Setting::Dpi(state.dpi.unwrap()),
            Setting::Tuning(patch.clone()),
        ],
        macros: vec![],
        source: None,
    };
    service.apply_at(&id, &p, &runtime, &root, None).unwrap();
    let v = service.read(&id, &runtime).unwrap();
    let actual = v.state.tuning.unwrap();
    assert_eq!(actual.debounce_ms, Some(4));
    assert_eq!(actual.lift_off_microns, Some(2000));
    let saved = v.session.unwrap().plan;
    assert!(saved.settings.iter().any(|s|matches!(s,Setting::Tuning(t)if t==&patch)));
    assert!(saved
        .settings
        .iter()
        .any(|s| matches!(s,Setting::Tuning(t)if t.debounce_ms==Some(4))));
    assert_eq!(
        saved,
        serde_json::from_slice::<Plan>(&serde_json::to_vec(&saved).unwrap()).unwrap()
    );
    runtime.shutdown();
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn duplicate_performance_patches_and_false_models_are_rejected() {
    let t = Setting::Tuning(MouseTuning {
        motion_sync: Some(true),
        ..Default::default()
    });
    let mut p = Plan {
        vendor: 0x373b,
        product: 0x1115,
        settings: vec![t.clone(), t],
        macros: vec![],
        source: None,
    };
    assert!(validate(&p).is_err());
    p.settings.pop();
    validate(&p).unwrap();
    p.product = 0x9999;
    assert!(validate(&p).is_err());
}
#[test]
#[ignore = "requires current performance UI output"]
fn tuning_browser_saved_profiles_import_and_execute() {
    let path = std::env::var_os("OPEN_HUB_TUNING_UI_PROFILES").unwrap();
    let profiles: Vec<crate::profiles::Profile> =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(profiles.len(), 9);
    let file = std::env::temp_dir().join(format!("tuning-pack-{}.json", crate::profiles::new_id()));
    std::fs::write(
        &file,
        serde_json::to_vec(
            &serde_json::json!({"format":"open-hub-profiles/1","profiles":profiles}),
        )
        .unwrap(),
    )
    .unwrap();
    let pack =
        crate::profile_transfer::preview("openhub", Some(file.to_string_lossy().into())).unwrap();
    for p in pack.profiles {
        let plan = p.multi.as_ref().unwrap();
        let (mut d, _) = simulated(plan.vendor, plan.product).unwrap();
        for s in &plan.settings {
            d.apply(s).unwrap();
        }
        let actual = d.read_state().unwrap().tuning.unwrap();
        assert_eq!(actual.motion_sync, Some(false));
        assert_eq!(actual.ripple_control, Some(true));
        assert_eq!(actual.led_idle_seconds, Some(120));
        assert_eq!(actual.sensor_sleep_seconds, Some(300));
        assert_eq!(actual.sensor_sleep_enabled, Some(false));
        assert!(!p.targets(0x046d, &[plan.product]));
    }
    std::fs::remove_file(file).unwrap();
}

#[test]fn m65_native_lift_and_snap_reach_existing_driver_and_cache_with_exact_packets(){let(mut d,log)=simulated(0x1b1c,0x1b2e).unwrap();assert_eq!(d.capabilities().lift_off.len(),5);assert!(d.capabilities().angle_snapping);log.lock().unwrap().clear();let p=Plan{vendor:0x1b1c,product:0x1b2e,settings:vec![Setting::LiftOff(4),Setting::AngleSnapping(false)],source:None,macros:vec![]};validate(&p).unwrap();for s in &p.settings{d.apply(s).unwrap();}let state=d.read_state().unwrap();assert_eq!(state.lift_off,Some(4));assert_eq!(state.angle_snapping,Some(false));let packets=log.lock().unwrap();for wanted in [&[7,19,3,0,5][..],&[7,19,4,0,0,5][..]]{assert!(packets.iter().any(|r|matches!(r,gearwise_core::transport::MockRecord::Write(p)if p.windows(wanted.len()).any(|v|v==wanted))));}let v=templates().unwrap().into_iter().find(|v|v.descriptor.product_id==0x1b2e&&v.descriptor.vendor_id==0x1b1c).unwrap();assert_eq!(v.readback,"stored_and_session_cached");if let Some(path)=std::env::var_os("OPEN_HUB_M65_PERFORMANCE_FIXTURE"){std::fs::write(path,serde_json::to_vec_pretty(&v).unwrap()).unwrap();}}
#[test]fn m65_invalid_lift_and_foreign_models_fail_before_execution(){for(v,p)in[(0x1b1c,0x1b2e),(0x1532,0x00ab),(0x3554,0xf507)]{let plan=Plan{vendor:v,product:p,settings:vec![Setting::LiftOff(5)],source:None,macros:vec![]};assert!(validate(&plan).is_err());}let p=Plan{vendor:0x1532,product:0x00ab,settings:vec![Setting::AngleSnapping(true)],source:None,macros:vec![]};assert!(validate(&p).is_err());}
#[test]#[ignore="requires current M65 performance UI output"]fn m65_performance_browser_plan_imports_and_runs_native_writes(){let path=std::env::var_os("OPEN_HUB_M65_PERFORMANCE_UI_PROFILE").unwrap();let p:crate::profiles::Profile=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();crate::profile_transfer::validate(&p).unwrap();let plan=p.multi.unwrap();assert!(plan.settings.contains(&Setting::LiftOff(4)));assert!(plan.settings.contains(&Setting::AngleSnapping(false)));let result=simulate(&plan).unwrap();assert_eq!(result["hardware_tested"],false);}
