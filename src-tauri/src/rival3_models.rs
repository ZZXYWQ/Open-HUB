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
