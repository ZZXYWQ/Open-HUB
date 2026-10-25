//! Real production SteelSeries driver over memory transport. Never HID or OS input.
use super::*;
use gearwise_core::{Driver,model::{DpiStage,DpiState,LightingEffect,ZoneState,Rgb,Modifiers,MouseButton},profile::{Macro,MacroMode,MacroStep},transport::{MockRecord,MockBackend,MockReply,MockTransport,HidInterfaceInfo,HidTransport}};
fn iface(pid:u16,unit:&str)->HidInterfaceInfo{HidInterfaceInfo{path:format!("simulation:steel:{pid:04x}:{unit}"),vendor_id:0x1038,product_id:pid,interface_number:0,usage_page:0xffc0,usage:1,serial:Some(unit.into()),..Default::default()}}
fn opened(pid:u16)->(Box<dyn gearwise_core::Device>,MockLog){gearwise_steelseries::simulated_model(pid).unwrap()}
fn plan(pid:u16,settings:Vec<Setting>)->Plan{Plan{vendor:0x1038,product:pid,settings,macros:vec![],source:None}}
fn action()->Macro{Macro{id:"steel-macro".into(),name:"独立鼠标组合".into(),mode:MacroMode::Once,steps:vec![MacroStep::KeyDown(6),MacroStep::Delay(7),MacroStep::KeyUp(6)]}}
#[test]fn steelseries_exact_three_model_dtos_have_correct_buttons_lights_and_sensor_steps(){
 let all=templates().unwrap();let views=all.into_iter().filter(|v|v.descriptor.vendor_id==0x1038).collect::<Vec<_>>();assert_eq!(views.len(),3);
 for v in &views{assert_eq!(v.capabilities.buttons.len(),if v.descriptor.product_id==0x1722{8}else{6});assert!(!v.capabilities.buttons[0].remappable);assert_eq!(v.capabilities.lighting.len(),2);assert_eq!(v.capabilities.dpi.as_ref().unwrap().max,if v.descriptor.product_id==0x1710{6500}else{12000});assert_eq!(v.readback,"write_only_cached");assert!(v.descriptor.simulated&&!v.hardware_tested);}
 if let Some(path)=std::env::var_os("OPEN_HUB_STEELSERIES_FIXTURE"){std::fs::write(path,serde_json::to_vec_pretty(&views).unwrap()).unwrap();}
 assert!(!supported(0x1038,0x1702));assert!(!supported(0x1038,0x1724));
}
#[test]fn steelseries_native_dpi_rates_and_default_keyboard_media_tables_match_independent_vectors(){
 for pid in crate::steelseries::PRODUCTS{let(mut d,log)=opened(pid);d.apply(&Setting::Dpi(DpiState{stages:vec![DpiStage::uniform(1000),DpiStage::uniform(2000)],active:0})).unwrap();d.apply(&Setting::ReportRate(250)).unwrap();d.apply(&Setting::Button{id:4,action:ButtonAction::Key{usage:6,mods:Modifiers(0x11)}}).unwrap();d.apply(&Setting::Button{id:5,action:ButtonAction::Media(0xea)}).unwrap();d.save_onboard().unwrap();let ops=log.lock().unwrap();
 let first=if pid==0x1710{vec![0,3,1,20]}else{vec![0,0x53,0,1,9,0,0x42]};assert_eq!(ops[0],MockRecord::Write(first));assert_eq!(ops[2],MockRecord::Write(if pid==0x1710{vec![0,4,0,3]}else{vec![0,0x54,0,3]}));
 let MockRecord::Write(table)=&ops[4]else{panic!("button table must be output")};assert_eq!(&table[..3],&[0,0x31,0]);assert_eq!(&table[18..23],&[0x51,0xe0,0xe4,6,0]);assert_eq!(&table[23..28],&[0x61,0xea,0,0,0]);assert_eq!(table.len(),3+if pid==0x1722{8}else{6}*5);if pid==0x1722{assert_eq!(&table[28..33],&[0x51,0x4e,0,0,0]);assert_eq!(&table[33..38],&[0x51,0x4b,0,0,0]);assert_eq!(&table[38..43],&[0x30,0,0,0,0]);}
 assert_eq!(ops.last().unwrap(),&MockRecord::Write(if pid==0x1710{vec![0,9,0]}else{vec![0,0x59,0]}));
 }
}
#[test]fn steelseries_all_known_buttons_compile_native_actions_and_independent_software_macros(){
 for pid in crate::steelseries::PRODUCTS{let count=if pid==0x1722{8}else{6};for id in 2..=count{for a in [ButtonAction::Default,ButtonAction::Disabled,ButtonAction::Mouse(MouseButton::Back),ButtonAction::Key{usage:6,mods:Modifiers(0x11)},ButtonAction::Media(0xea),ButtonAction::Dpi(DpiFunction::Cycle),ButtonAction::Macro("steel-macro".into())]{let is_macro=matches!(a,ButtonAction::Macro(_));let mut p=plan(pid,vec![Setting::Button{id,action:a}]);if is_macro{p.macros.push(action());}validate(&p).unwrap();let r=simulate(&p).unwrap();assert!(!r["packets"].as_array().unwrap().is_empty());assert_eq!(r["software_macros"].as_array().unwrap().len(),usize::from(is_macro));if is_macro{assert_eq!(r["software_macros"][0]["events"],serde_json::json!(["key_down:6","key_up:6"]));}}}}
}
#[test]fn steelseries_preflight_rejects_non_native_dpi_active_stage_keys_media_and_lighting_before_apply(){
 for pid in crate::steelseries::PRODUCTS{let invalid=[Setting::Dpi(DpiState{stages:vec![DpiStage::uniform(1001)],active:0}),Setting::Dpi(DpiState{stages:vec![DpiStage::uniform(800),DpiStage::uniform(1600)],active:1}),Setting::Button{id:1,action:ButtonAction::Disabled},Setting::Button{id:9,action:ButtonAction::Mouse(MouseButton::Back)},Setting::Button{id:4,action:ButtonAction::Key{usage:6,mods:Modifiers(0x0f)}},Setting::Button{id:4,action:ButtonAction::Media(0xb7)},Setting::Lighting{zone:2,state:ZoneState{effect:LightingEffect::Off,brightness:100}}];for bad in invalid{assert!(validate(&plan(pid,vec![Setting::ReportRate(500),bad])).is_err());}}
 assert!(validate(&plan(0x1710,vec![Setting::Lighting{zone:0,state:ZoneState{effect:LightingEffect::Breathing{color:Rgb{r:1,g:2,b:3},period_ms:4000},brightness:50}}])).is_err());
}
#[test]fn steelseries_two_zones_have_exact_native_colors_brightness_and_model_specific_rhythms(){
 for pid in crate::steelseries::PRODUCTS{for zone in 0..2{let(mut d,log)=opened(pid);let setting=Setting::Lighting{zone,state:ZoneState{effect:LightingEffect::Static{color:Rgb{r:100,g:150,b:200}},brightness:50}};crate::steelseries::validate_setting(pid,&setting).unwrap();d.apply(&setting).unwrap();let ops=log.lock().unwrap();if pid==0x1710{assert_eq!(ops[0],MockRecord::Write(vec![0,8,zone+1,50,75,100]));assert_eq!(ops[1],MockRecord::Write(vec![0,7,zone+1,1]));}else{let MockRecord::SetFeature(bytes)=&ops[0]else{panic!("colour stops must be feature")};assert_eq!(&bytes[..4],&[0,0x5b,0,zone]);assert_eq!(&bytes[4..6],&1000u16.to_le_bytes());assert_eq!(&bytes[29..32],&[50,75,100]);}}
 if pid!=0x1710{let(mut d,log)=opened(pid);let s=Setting::Lighting{zone:1,state:ZoneState{effect:LightingEffect::ColorCycle{period_ms:1000},brightness:100}};crate::steelseries::validate_setting(pid,&s).unwrap();d.apply(&s).unwrap();let ops=log.lock().unwrap();let MockRecord::SetFeature(bytes)=&ops[0]else{panic!()};assert_eq!(&bytes[4..6],&1000u16.to_le_bytes());}
 }
}
struct ShortOutput{writes:Arc<Mutex<usize>>}
impl HidTransport for ShortOutput{fn write(&mut self,data:&[u8])->gearwise_core::Result<usize>{*self.writes.lock().unwrap()+=1;Ok(data.len()-1)}fn send_feature_report(&mut self,_:&[u8])->gearwise_core::Result<()>{Ok(())}fn read_timeout(&mut self,_:&mut[u8],_:i32)->gearwise_core::Result<usize>{Ok(0)}fn get_feature_report(&mut self,_:&mut[u8])->gearwise_core::Result<usize>{Ok(0)}}
#[test]fn steelseries_output_short_write_is_sent_once_and_does_not_commit_cached_state(){
 for pid in crate::steelseries::PRODUCTS{let writes=Arc::new(Mutex::new(0));let c=writes.clone();let info=iface(pid,"short");let backend=MockBackend::new(vec![info.clone()],move |_|Ok(Box::new(ShortOutput{writes:c.clone()})));let mut d=gearwise_steelseries::SteelSeriesDriver::with_gap(std::time::Duration::ZERO).open(&info,&backend).unwrap().remove(0);let before=d.read_state().unwrap();assert!(d.apply(&Setting::ReportRate(500)).unwrap_err().to_string().contains("short write"));assert_eq!(*writes.lock().unwrap(),1);assert_eq!(d.read_state().unwrap().report_rate,before.report_rate);}
}
#[test]
#[ignore="requires current exact SteelSeries browser profiles"]
fn steelseries_browser_profiles_roundtrip_import_and_execute_exact_native_model(){
 let path=std::env::var_os("OPEN_HUB_STEELSERIES_UI_PROFILES").unwrap();let profiles:Vec<crate::profiles::Profile>=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();assert_eq!(profiles.len(),3);
 let file=std::env::temp_dir().join(format!("steelseries-ui-{}.json",crate::profiles::new_id()));std::fs::write(&file,serde_json::to_vec(&serde_json::json!({"format":"open-hub-profiles/1","profiles":profiles})).unwrap()).unwrap();let preview=crate::profile_transfer::preview("openhub",Some(file.display().to_string())).unwrap();let _=std::fs::remove_file(file);
 for p in preview.profiles{let plan=p.multi.as_ref().unwrap();assert!(crate::steelseries::supported(plan.vendor,plan.product));let count=if plan.product==0x1722{8}else{6};assert_eq!(plan.settings.iter().filter(|s|matches!(s,Setting::Button{..})).count(),count-1);assert_eq!(plan.settings.iter().filter(|s|matches!(s,Setting::Lighting{..})).count(),2);assert_eq!(plan.macros.len(),1);let r=simulate(plan).unwrap();assert_eq!(r["software_macros"].as_array().unwrap().len(),1);assert_eq!(r["hardware_tested"],false);}
}
