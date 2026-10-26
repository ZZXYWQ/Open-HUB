//! Production calibration setting/profile flow over synthetic firmware only.
use super::*;
use gearwise_core::SensorCalibration;

fn calibration_plan(pid: u16, value: SensorCalibration) -> Plan {
    Plan { vendor: 0x1532, product: pid,
        settings: vec![Setting::SensorCalibration(value)], macros: vec![], source: None }
}

#[test]
fn calibration_plan_is_exact_model_scoped_unique_and_capability_bounded() {
    let value=SensorCalibration::Asymmetric { lift: 26, landing: 25 };
    let supported:Vec<u16>=[0xb6,0xb7].into_iter().filter(|pid|gearwise_razer::calibration_caps(*pid).is_some()).collect();
    assert!(!supported.is_empty());
    for pid in supported {
        let plan=calibration_plan(pid,value);
        validate(&plan).unwrap();
        let compiled=compile(&plan).unwrap();
        assert_eq!(compiled.settings,plan.settings);
        assert!(!compiled.software_input && compiled.bindings.is_empty());
        let(mut device,_)=gearwise_razer::simulated_dav3pro(pid).unwrap();
        let view=view(device.as_mut()).unwrap();
        validate_capabilities(&plan,&view.capabilities).unwrap();
        let mut absent=view.capabilities.clone();absent.calibration=None;
        assert!(validate_capabilities(&plan,&absent).is_err());
        let mut duplicate=plan.clone();duplicate.settings.extend(plan.settings.clone());
        assert!(validate(&duplicate).is_err());
        for invalid in [SensorCalibration::Standard { distance: 0 },
            SensorCalibration::Standard { distance: 4 },
            SensorCalibration::Asymmetric { lift: 1, landing: 0 },
            SensorCalibration::Asymmetric { lift: 26, landing: 26 },
            SensorCalibration::Asymmetric { lift: 13, landing: 13 },
            SensorCalibration::Asymmetric { lift: 27, landing: 1 }] {
            assert!(validate(&calibration_plan(pid,invalid)).is_err());
        }
    }
    for view in templates().unwrap() {
        let mut plan=calibration_plan(view.descriptor.product_id,value);
        plan.vendor=view.descriptor.vendor_id;
        if plan.vendor!=0x1532||gearwise_razer::calibration_caps(plan.product).is_none(){
            assert!(validate(&plan).is_err(),"{:04x}:{:04x}",plan.vendor,plan.product);
        }
    }
    for product in [0x00b5,0x00b8,0x00c2,0xffff]{assert!(validate(&calibration_plan(product,value)).is_err());}
}

#[test]
fn calibration_template_apply_read_save_reload_and_session_keep_native_state() {
    for pid in [0xb6,0xb7].into_iter().filter(|pid|gearwise_razer::calibration_caps(*pid).is_some()) {
        let(mut device,_)=gearwise_razer::simulated_dav3pro(pid).unwrap();
        let initial=view(device.as_mut()).unwrap();
        assert_eq!(initial.state.calibration,Some(SensorCalibration::Standard{distance:2}));
        assert!(template_plan(&initial).unwrap().settings.iter().any(|s|matches!(s,Setting::SensorCalibration(SensorCalibration::Standard{distance:2}))));
        let runtime=crate::macro_runtime::Runtime::default();
        let service=Service::default();
        for value in [SensorCalibration::Standard{distance:1},SensorCalibration::Standard{distance:3},
            SensorCalibration::Asymmetric{lift:2,landing:1},SensorCalibration::Asymmetric{lift:26,landing:25},
            SensorCalibration::Standard{distance:2}] {
            let plan=calibration_plan(pid,value);
            validate(&plan).unwrap();
            for setting in compile(&plan).unwrap().settings {device.apply(&setting).unwrap();}
            let mut actual=view(device.as_mut()).unwrap();
            assert_eq!(actual.state.calibration,Some(value));
            assert!(actual.state.lighting.is_empty());
            assert_eq!(actual.state.dpi,initial.state.dpi);
            assert_eq!(actual.state.buttons,initial.state.buttons);
            service.remember_session(&actual,&plan,false).unwrap();
            service.attach_session(&mut actual,&runtime).unwrap();
            let session=actual.session.unwrap().plan;
            assert_eq!(session.settings.iter().filter(|s|matches!(s,Setting::SensorCalibration(_))).cloned().collect::<Vec<_>>(),plan.settings);
            assert!(session.settings.contains(&Setting::Dpi(initial.state.dpi.clone().unwrap())));
            assert!(session.settings.contains(&Setting::ReportRate(initial.state.report_rate.unwrap())));
            assert!(session.macros.is_empty()&&session.source.is_none());
            let encoded=serde_json::to_vec(&plan).unwrap();
            assert_eq!(serde_json::from_slice::<Plan>(&encoded).unwrap(),plan);
            assert_eq!(simulate(&plan).unwrap()["hardware_tested"],false);
        }
        // An external native change is reflected in the selected session field.
        let desired=calibration_plan(pid,SensorCalibration::Standard{distance:2});
        service.remember_session(&view(device.as_mut()).unwrap(),&desired,false).unwrap();
        device.apply(&Setting::SensorCalibration(SensorCalibration::Asymmetric{lift:13,landing:12})).unwrap();
        let mut actual=view(device.as_mut()).unwrap();service.attach_session(&mut actual,&runtime).unwrap();
        assert_eq!(actual.session.unwrap().plan.settings.iter().filter(|s|matches!(s,Setting::SensorCalibration(_))).cloned().collect::<Vec<_>>(),vec![Setting::SensorCalibration(SensorCalibration::Asymmetric{lift:13,landing:12})]);
    }
}

#[test]
#[ignore="requires current calibration UI profiles"]
fn browser_calibration_profiles_roundtrip_store_then_execute_native_codec() {
    let mut profiles:Vec<crate::profiles::Profile>=serde_json::from_slice(&std::fs::read(std::env::var_os("OPEN_HUB_DAV3PRO_CALIBRATION_UI_PROFILES").unwrap()).unwrap()).unwrap();
    assert!(!profiles.is_empty());
    for profile in &mut profiles{profile.id=crate::profiles::new_id();}
    let mut store=crate::profiles::ProfileStore::default();
    crate::profile_transfer::merge(&mut store,profiles.clone()).unwrap();
    let file=std::env::temp_dir().join(format!("dav3pro-calibration-ui-{}.json",crate::profiles::new_id()));
    store.save(&file).unwrap();let loaded=crate::profiles::ProfileStore::load(&file);std::fs::remove_file(file).unwrap();
    let mut modes=std::collections::BTreeSet::new();
    for profile in profiles {
        let plan=loaded.get(&profile.id).unwrap().multi.as_ref().unwrap();
        validate(plan).unwrap();
        assert_eq!(profile.target_vendor,Some(plan.vendor));assert_eq!(profile.target_model,Some(plan.product));
        let desired=plan.settings.iter().find_map(|s|if let Setting::SensorCalibration(value)=s{Some(*value)}else{None}).unwrap();
        match desired {SensorCalibration::Standard{distance}=>{assert_eq!(distance,3);modes.insert((plan.product,0));},SensorCalibration::Asymmetric{lift,landing}=>{assert_eq!((lift,landing),(26,25));modes.insert((plan.product,1));}}
        let(mut device,_)=gearwise_razer::simulated_dav3pro(plan.product).unwrap();
        let initial=view(device.as_mut()).unwrap();
        for setting in compile(plan).unwrap().settings{device.apply(&setting).unwrap();}
        let actual=device.read_state().unwrap();assert_eq!(actual.calibration,Some(desired));
        assert_eq!(actual.dpi,initial.state.dpi);assert_eq!(actual.buttons,initial.state.buttons);
        assert!(actual.lighting.is_empty());
    }
    for pid in [0xb6,0xb7].into_iter().filter(|pid|gearwise_razer::calibration_caps(*pid).is_some()){
        assert!(modes.contains(&(pid,0))&&modes.contains(&(pid,1)));
    }
}
