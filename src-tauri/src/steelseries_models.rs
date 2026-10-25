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
 let imported=roundtrip_browser_profiles("OPEN_HUB_STEELSERIES_UI_PROFILES");assert_eq!(imported.len(),3);

 for p in imported{let plan=p.multi.as_ref().unwrap();assert!(crate::steelseries::supported(plan.vendor,plan.product));let count=if plan.product==0x1722{8}else{6};assert_eq!(plan.settings.iter().filter(|s|matches!(s,Setting::Button{..})).count(),count-1);assert_eq!(plan.settings.iter().filter(|s|matches!(s,Setting::Lighting{..})).count(),2);assert_eq!(plan.macros.len(),1);let r=simulate(plan).unwrap();assert_eq!(r["software_macros"].as_array().unwrap().len(),1);assert_eq!(r["hardware_tested"],false);}
}

fn gradient(colors:&[(u8,[u8;3])],period_ms:u16)->gearwise_core::model::LightingEffect{
 let mut stops=[gearwise_core::model::ColorStop::EMPTY;14];for(i,(position,c))in colors.iter().enumerate(){stops[i]=gearwise_core::model::ColorStop{position:(*position).into(),color:Rgb{r:c[0],g:c[1],b:c[2]}};}LightingEffect::Gradient{stops,count:colors.len()as u8,period_ms}
}
#[test]fn steelseries_gradient_full_feature_buffers_match_inert_independent_reference_vectors(){
 let vectors:serde_json::Value=serde_json::from_str(include_str!("../../docs/development/steelseries-independent-gradient-vectors.json")).unwrap();
 for pid in [0x1720,0x1722]{for(zone,colors,period)in [(0,vec![(0,[255,0,0]),(33,[0,255,0]),(66,[0,0,255])],1000),(1,vec![(0,[17,34,51]),(25,[68,85,102]),(50,[119,136,153]),(75,[170,187,204])],5000)]{let(mut d,log)=opened(pid);let setting=Setting::Lighting{zone,state:ZoneState{effect:gradient(&colors,period),brightness:100}};crate::steelseries::validate_setting(pid,&setting).unwrap();d.apply(&setting).unwrap();let hex=vectors["cases"][usize::from(zone)]["hidapi_feature_hex"].as_str().unwrap();let expected=(0..hex.len()).step_by(2).map(|i|u8::from_str_radix(&hex[i..i+2],16).unwrap()).collect::<Vec<_>>();assert_eq!(log.lock().unwrap().as_slice(),&[MockRecord::SetFeature(expected)]);assert_eq!(d.read_state().unwrap().lighting[&zone].effect,if let Setting::Lighting{state,..}=setting{state.effect}else{unreachable!()});}}
}
#[test]fn steelseries_gradient_orders_bounds_closure_and_model_isolation_are_validated_before_any_send(){
 let base=gradient(&[(0,[100,150,200]),(50,[1,2,3]),(100,[10,20,30])],1777);
 for case in 0..6{let mut effect=base;if let LightingEffect::Gradient{ref mut stops,ref mut count,ref mut period_ms}=effect{match case{0=>*count=0,1=>*count=15,2=>*period_ms=0,3=>stops[0].position=1u8.into(),4=>stops[1].position=101u8.into(),_=>{stops[1].position=75u8.into();stops[2].position=50u8.into();}}}let(mut d,log)=opened(0x1722);assert!(d.apply(&Setting::Lighting{zone:0,state:ZoneState{effect,brightness:100}}).is_err());assert!(log.lock().unwrap().is_empty());}
 let s=Setting::Lighting{zone:0,state:ZoneState{effect:base,brightness:50}};for(v,p)in[(0x1532,0x0084),(0x1532,0x00aa),(0x1b1c,0x1b2e),(0x373b,0x1115),(0x1038,0x1710)]{let plan=Plan{vendor:v,product:p,settings:vec![s.clone()],macros:vec![],source:None};assert!(validate(&plan).is_err());}
 let(mut d,log)=opened(0x1722);d.apply(&s).unwrap();let ops=log.lock().unwrap();let MockRecord::SetFeature(bytes)=&ops[0]else{panic!()};assert_eq!(&bytes[29..32],&[50,75,100]);assert_eq!(bytes[28],3);
}
#[test]fn steelseries_fourteen_point_gradient_preserves_exact_active_count_and_profile_roundtrip(){
 let colors=(0..14u8).map(|i|(i*7,[i,255-i,i*5])).collect::<Vec<_>>();let s=Setting::Lighting{zone:1,state:ZoneState{effect:gradient(&colors,65535),brightness:100}};let(mut d,log)=opened(0x1722);d.apply(&s).unwrap();let ops=log.lock().unwrap();let MockRecord::SetFeature(bytes)=&ops[0]else{panic!()};assert_eq!(bytes.len(),88);assert_eq!(bytes[28],14);assert_eq!(&bytes[4..6],&[255,255]);
 let encoded=serde_json::to_vec(&s).unwrap();let decoded:Setting=serde_json::from_slice(&encoded).unwrap();assert_eq!(s,decoded);let mut malformed=serde_json::to_value(&s).unwrap();malformed["Lighting"]["state"]["effect"]["Gradient"]["stops"].as_array_mut().unwrap().pop();assert!(serde_json::from_value::<Setting>(malformed).is_err());
}

#[test]
#[ignore="requires current native gradient browser profiles"]
fn steelseries_browser_native_gradients_roundtrip_and_compile_into_exact_selected_zone(){
 let profiles=roundtrip_browser_profiles("OPEN_HUB_STEELSERIES_GRADIENT_UI_PROFILES");assert_eq!(profiles.len(),2);for p in profiles{crate::profile_transfer::validate(&p).unwrap();let encoded=serde_json::to_vec(&p).unwrap();let restored:crate::profiles::Profile=serde_json::from_slice(&encoded).unwrap();let plan=restored.multi.unwrap();assert!(matches!(plan.product,0x1720|0x1722));let setting=plan.settings.iter().find(|s|matches!(s,Setting::Lighting{zone:0,..})).unwrap();let Setting::Lighting{state,..}=setting else{unreachable!()};let LightingEffect::Gradient{stops,count,period_ms}=state.effect else{panic!("missing native gradient")};assert_eq!((count,period_ms),(14,4321));assert_eq!(stops[0].color,Rgb{r:18,g:52,b:86});let(mut d,log)=opened(plan.product);d.apply(setting).unwrap();let ops=log.lock().unwrap();let MockRecord::SetFeature(bytes)=&ops[0]else{panic!()};assert_eq!(&bytes[1..4],&[0x5b,0,0]);assert_eq!(bytes[28],14);assert_eq!(&bytes[4..6],&4321u16.to_le_bytes());assert_eq!(&bytes[29..32],&[18,52,86]);assert_eq!(simulate(&plan).unwrap()["hardware_tested"],false);}
}

// Uses the same id assignment, production profile store and import merger as
// a real save/export/import, including a separate reloaded store. No AppData.
fn roundtrip_browser_profiles(env:&str)->Vec<crate::profiles::Profile>{
 let path=std::env::var_os(env).unwrap();let profiles:Vec<crate::profiles::Profile>=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();let mut store=crate::profiles::ProfileStore::default();for mut p in profiles{crate::profile_transfer::validate(&p).unwrap();if p.id.is_empty(){p.id=crate::profiles::new_id();}store.upsert(p);}
 let dir=std::env::temp_dir().join(format!("open-hub-steel-store-{}",crate::profiles::new_id()));std::fs::create_dir(&dir).unwrap();let state=dir.join("profiles.json");store.save(&state).unwrap();let loaded=crate::profiles::ProfileStore::load(&state);let profiles=loaded.profiles.into_iter().filter(|p|p.id!=crate::profiles::DESKTOP_ID).collect::<Vec<_>>();let pack=dir.join("export.json");std::fs::write(&pack,serde_json::to_vec(&serde_json::json!({"format":"open-hub-profiles/1","profiles":profiles})).unwrap()).unwrap();let preview=crate::profile_transfer::preview("openhub",Some(pack.display().to_string())).unwrap();let mut dest=crate::profiles::ProfileStore::default();crate::profile_transfer::merge(&mut dest,preview.profiles).unwrap();dest.save(&state).unwrap();let imported=crate::profiles::ProfileStore::load(&state).profiles.into_iter().filter(|p|p.id!=crate::profiles::DESKTOP_ID).collect();std::fs::remove_file(pack).unwrap();std::fs::remove_file(state).unwrap();std::fs::remove_dir(dir).unwrap();imported
}

#[test]fn native_half_percent_json_stops_keep_exact_quantized_deltas_and_integer_vectors_remain_unchanged(){
 use gearwise_core::model::{ColorStop,ColorPosition};
 let mut stops=[ColorStop::EMPTY;14];for(i,(halves,color))in[(0,Rgb{r:255,g:0,b:0}),(33,Rgb::BLACK),(66,Rgb{r:0,g:255,b:0}),(99,Rgb::BLACK),(132,Rgb{r:0,g:0,b:255}),(166,Rgb::BLACK)].into_iter().enumerate(){stops[i]=ColorStop{position:ColorPosition::from_half_percent(halves),color};}
 let setting=Setting::Lighting{zone:0,state:ZoneState{effect:LightingEffect::Gradient{stops,count:6,period_ms:10000},brightness:100}};let json=serde_json::to_value(&setting).unwrap();assert_eq!(json["Lighting"]["state"]["effect"]["Gradient"]["stops"][1]["position"],serde_json::json!(16.5));let decoded:Setting=serde_json::from_value(json.clone()).unwrap();assert_eq!(decoded,setting);
 for pid in[0x1720,0x1722]{let(mut device,log)=opened(pid);device.apply(&decoded).unwrap();let ops=log.lock().unwrap();let MockRecord::SetFeature(bytes)=&ops[0]else{panic!()};assert_eq!(bytes[28],7);assert_eq!((0..7).map(|i|bytes[35+i*4]).collect::<Vec<_>>(),[0,42,42,42,42,43,44]);assert_eq!(&bytes[36..39],&[0,0,0]);}
 for bad in[16.25,-0.5,100.5]{let mut malformed=json.clone();malformed["Lighting"]["state"]["effect"]["Gradient"]["stops"][1]["position"]=serde_json::json!(bad);assert!(serde_json::from_value::<Setting>(malformed).is_err());}
}
