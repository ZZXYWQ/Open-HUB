//! Original wired Rival 3, memory transport only; no hardware or injected input.
use super::*;
use gearwise_core::{Driver,model::{DpiState,DpiStage,Modifiers},transport::{MockRecord,MockBackend,HidInterfaceInfo,HidTransport}};
use gearwise_steelseries::rival3::{self,Profile};
fn plan(pid:u16,settings:Vec<Setting>)->Plan{Plan{vendor:0x1038,product:pid,settings,macros:vec![],source:None}}
#[test]fn rival3_exact_templates_distinguish_two_firmwares_from_other_models(){
 let views=templates().unwrap().into_iter().filter(|v|v.descriptor.vendor_id==0x1038&&rival3::supported(v.descriptor.product_id)).collect::<Vec<_>>();assert_eq!(views.len(),2);
 for v in &views{assert_eq!(v.capabilities.buttons.len(),8);assert_eq!(v.capabilities.lighting.len(),4);assert_eq!(v.capabilities.vendor_profile.as_deref(),Some(rival3::SCHEMA));let dpi=v.capabilities.dpi.as_ref().unwrap();assert_eq!((dpi.min,dpi.max),(200,8500));assert_eq!(v.readback,"write_only_cached");assert!(v.descriptor.simulated&&!v.hardware_tested);let p=template_plan(v).unwrap();validate(&p).unwrap();assert_eq!(p.settings.iter().filter(|s|matches!(s,Setting::VendorProfile{..})).count(),1);assert!(!p.settings.iter().any(|s|matches!(s,Setting::Lighting{..})));}
 if let Some(path)=std::env::var_os("OPEN_HUB_RIVAL3_VIEWS"){std::fs::write(path,serde_json::to_vec_pretty(&views).unwrap()).unwrap();}
 assert!(!supported(0x1038,0x1830));
}
#[test]fn rival3_native_sensor_rate_and_eight_slot_table_match_reference_bytes(){
 for pid in [0x1824,0x184c]{let(mut d,log)=gearwise_steelseries::simulated_model(pid).unwrap();d.apply(&Setting::Dpi(DpiState{stages:[200,400,800,1600].into_iter().map(DpiStage::uniform).collect(),active:0})).unwrap();d.apply(&Setting::ReportRate(250)).unwrap();d.apply(&Setting::Button{id:4,action:ButtonAction::Key{usage:6,mods:Modifiers(0)}}).unwrap();d.apply(&Setting::Button{id:5,action:ButtonAction::Media(0xea)}).unwrap();let ops=log.lock().unwrap();assert_eq!(ops[0],MockRecord::Write(vec![0,11,0,4,1,4,8,0x12,0x24]));assert_eq!(ops[1],MockRecord::Write(vec![0,4,0,3]));let MockRecord::Write(table)=&ops[3]else{panic!()};assert_eq!(table,&vec![0,7,0,1,0,2,0,3,0,0x33,6,0x34,0xea,0x30,0,0x31,0,0x32,0]);}
}
#[test]fn rival3_all_patterns_execute_exact_palette_without_host_color_scaling(){
 for pid in[0x1824,0x184c]{for mode in 0..=6{let(mut d,log)=gearwise_steelseries::simulated_model(pid).unwrap();let mut p=Profile{mode,..Default::default()};p.zones[0].brightness=37;let s=rival3::setting(&p);validate(&plan(pid,vec![s.clone()])).unwrap();d.apply(&s).unwrap();let ops=log.lock().unwrap();assert_eq!(ops.len(),5);assert_eq!(ops[0],MockRecord::Write(vec![0,5,0,1,255,0,0,37]));assert_eq!(ops[4],MockRecord::Write(vec![0,6,0,mode]));drop(ops);let state=d.read_state().unwrap();assert_eq!(state.vendor_profile,Some(serde_json::to_value(&p).unwrap()));assert_eq!(state.lighting.len(),if mode==4{4}else{0});}}
}
struct ShortAt{calls:Arc<Mutex<usize>>,fail:usize}
impl HidTransport for ShortAt{fn write(&mut self,data:&[u8])->gearwise_core::Result<usize>{let mut n=self.calls.lock().unwrap();*n+=1;Ok(data.len()-usize::from(*n==self.fail))}fn send_feature_report(&mut self,_:&[u8])->gearwise_core::Result<()>{panic!("output only")}fn read_timeout(&mut self,_:&mut[u8],_:i32)->gearwise_core::Result<usize>{Ok(0)}fn get_feature_report(&mut self,_:&mut[u8])->gearwise_core::Result<usize>{Ok(0)}}
#[test]fn rival3_every_partial_lighting_failure_invalidates_cache_without_retry_and_can_explicitly_recover(){
 for pid in[0x1824,0x184c]{for fail in 1..=5{let calls=Arc::new(Mutex::new(0));let c=calls.clone();let info=HidInterfaceInfo{path:format!("simulation:rival3:{pid}:{fail}"),vendor_id:0x1038,product_id:pid,interface_number:3,usage_page:0xffc0,usage:1,..Default::default()};let backend=MockBackend::new(vec![info.clone()],move |_|Ok(Box::new(ShortAt{calls:c.clone(),fail})));let mut d=gearwise_steelseries::SteelSeriesDriver::with_gap(std::time::Duration::ZERO).open(&info,&backend).unwrap().remove(0);let s=rival3::setting(&Profile::default());assert!(d.apply(&s).unwrap_err().to_string().contains("uncertain"));assert_eq!(*calls.lock().unwrap(),fail);let state=d.read_state().unwrap();assert!(state.vendor_profile.is_none()&&state.lighting.is_empty());d.apply(&s).unwrap();assert_eq!(*calls.lock().unwrap(),fail+5);assert!(d.read_state().unwrap().vendor_profile.is_some());}}
}
#[test]fn rival3_native_modifiers_and_foreign_lighting_schema_are_rejected_before_io(){
 for pid in[0x1824,0x184c]{assert!(validate(&plan(pid,vec![Setting::ReportRate(500),Setting::Button{id:4,action:ButtonAction::Key{usage:6,mods:Modifiers(1)}}])).is_err());let s=rival3::setting(&Profile::default());for foreign in[0x1720,0x1724,0x1832]{assert!(validate(&plan(foreign,vec![s.clone()])).is_err());}}
}

#[test]fn rival3_all_nonprimary_inputs_compile_independent_software_macros(){
 use gearwise_core::profile::{Macro,MacroMode,MacroStep};
 for pid in[0x1824,0x184c]{for id in 2..=8{let mut p=plan(pid,vec![Setting::Button{id,action:ButtonAction::Macro("rival3-macro".into())}]);p.macros.push(Macro{id:"rival3-macro".into(),name:"Per mouse".into(),mode:MacroMode::Once,steps:vec![MacroStep::KeyDown(6),MacroStep::Delay(7),MacroStep::KeyUp(6)]});validate(&p).unwrap();let result=simulate(&p).unwrap();assert_eq!(result["software_macros"].as_array().unwrap().len(),1);assert_eq!(result["hardware_tested"],false);}}
}
#[test]
#[ignore="requires current Rival3 browser profiles"]
fn rival3_browser_saved_profiles_import_and_replay_native_codec(){
 let profiles=super::steelseries_models::roundtrip_browser_profiles("OPEN_HUB_RIVAL3_UI_PROFILES");assert_eq!(profiles.len(),2);
 for profile in profiles{let p=profile.multi.as_ref().unwrap();validate(p).unwrap();assert!(gearwise_steelseries::rival3::supported(p.product));assert_eq!(p.settings.iter().filter(|s|matches!(s,Setting::Button{..})).count(),7);let r=simulate(p).unwrap();assert_eq!(r["software_macros"].as_array().unwrap().len(),1);assert_eq!(r["hardware_tested"],false);}
}

#[test]fn rival3_original_full_brightness_steady_rewrites_use_oem_batch_after_success_only(){
 for pid in[0x1824,0x184c]{let(mut d,log)=gearwise_steelseries::simulated_model(pid).unwrap();let mut p=Profile::default();d.apply(&rival3::setting(&p)).unwrap();assert_eq!(log.lock().unwrap().len(),5);log.lock().unwrap().clear();p.zones[0].color=gearwise_core::model::Rgb{r:1,g:2,b:3};p.zones[3].color=gearwise_core::model::Rgb{r:4,g:5,b:6};d.apply(&rival3::setting(&p)).unwrap();let ops=log.lock().unwrap();if pid==0x1824{let mut color=vec![0,10,0,15,1,2,3,0,255,0,0,0,255,4,5,6];color.resize(33,0);let mut mode=vec![0,6,0,4];mode.resize(33,0);assert_eq!(ops.as_slice(),&[MockRecord::Write(color),MockRecord::Write(mode)]);}else{assert_eq!(ops.len(),5);assert!(ops.iter().all(|r|!matches!(r,MockRecord::Write(v)if v[1]==10)));}drop(ops);assert_eq!(d.read_state().unwrap().vendor_profile,Some(serde_json::to_value(p).unwrap()));}
}
#[test]fn rival3_dimmed_disabled_or_pattern_palettes_cannot_use_brightnessless_batch(){
 for prior in 0..3{let(mut d,log)=gearwise_steelseries::simulated_model(0x1824).unwrap();let mut p=Profile::default();match prior{0=>p.zones[0].brightness=37,1=>p.enabled=false,_=>p.mode=6};d.apply(&rival3::setting(&p)).unwrap();log.lock().unwrap().clear();d.apply(&rival3::setting(&Profile::default())).unwrap();assert_eq!(log.lock().unwrap().len(),5);assert_eq!(log.lock().unwrap()[0],MockRecord::Write(vec![0,5,0,1,255,0,0,100]));}
 for target in 0..3{let(mut d,log)=gearwise_steelseries::simulated_model(0x1824).unwrap();d.apply(&rival3::setting(&Profile::default())).unwrap();log.lock().unwrap().clear();let mut p=Profile::default();match target{0=>p.zones[2].brightness=99,1=>p.enabled=false,_=>p.mode=2};d.apply(&rival3::setting(&p)).unwrap();assert_eq!(log.lock().unwrap().len(),5);}
}
#[test]fn rival3_both_batch_failures_clear_session_priming_and_explicit_recovery_uses_complete_writes(){
 for fail in[6,7]{let calls=Arc::new(Mutex::new(0));let c=calls.clone();let info=HidInterfaceInfo{path:format!("simulation:rival3-batch:{fail}"),vendor_id:0x1038,product_id:0x1824,interface_number:3,usage_page:0xffc0,usage:1,..Default::default()};let backend=MockBackend::new(vec![info.clone()],move |_|Ok(Box::new(ShortAt{calls:c.clone(),fail})));let mut d=gearwise_steelseries::SteelSeriesDriver::with_gap(std::time::Duration::ZERO).open(&info,&backend).unwrap().remove(0);let s=rival3::setting(&Profile::default());d.apply(&s).unwrap();assert!(d.apply(&s).unwrap_err().to_string().contains("uncertain"));assert_eq!(*calls.lock().unwrap(),fail);assert!(d.read_state().unwrap().vendor_profile.is_none());d.apply(&s).unwrap();assert_eq!(*calls.lock().unwrap(),fail+5);}
}

#[test]fn rival3_priming_belongs_to_one_open_device_and_static_zone_keeps_other_colors(){
 let(mut a,la)=gearwise_steelseries::simulated_model(0x1824).unwrap();let(mut b,lb)=gearwise_steelseries::simulated_model(0x1824).unwrap();a.apply(&rival3::setting(&Profile::default())).unwrap();la.lock().unwrap().clear();assert!(lb.lock().unwrap().is_empty());let s=Setting::Lighting{zone:1,state:gearwise_core::model::ZoneState{effect:gearwise_core::model::LightingEffect::Static{color:gearwise_core::model::Rgb{r:7,g:8,b:9}},brightness:100}};a.apply(&s).unwrap();assert_eq!(la.lock().unwrap().len(),2);assert!(lb.lock().unwrap().is_empty());let state=a.read_state().unwrap();assert_eq!(state.lighting[&0].effect,gearwise_core::model::LightingEffect::Static{color:gearwise_core::model::Rgb{r:255,g:0,b:0}});assert_eq!(state.lighting[&1].effect,gearwise_core::model::LightingEffect::Static{color:gearwise_core::model::Rgb{r:7,g:8,b:9}});b.apply(&s).unwrap();assert_eq!(lb.lock().unwrap().len(),5);let(mut reopened,lr)=gearwise_steelseries::simulated_model(0x1824).unwrap();reopened.apply(&s).unwrap();assert_eq!(lr.lock().unwrap().len(),5);
}
