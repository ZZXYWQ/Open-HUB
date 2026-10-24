//! Exact-model regression using the real Razer driver with stateful fake firmware.
use super::*;
use gearwise_core::{model::{DpiStage,DpiState,LightingEffect,ZoneState,Rgb,MouseButton,Modifiers},transport::{MockBackend,MockRecord}};
const PIDS:[u16;6]=[0x007a,0x007b,0x00a5,0x00a6,0x00c0,0x00c1];
#[test]fn foreign_model_plan_never_cancels_current_mouse_cleanup_or_writes(){
 let(device,log)=crate::naga::simulated();let id=device.descriptor().id.clone();let mut manager=DeviceManager::new(Box::new(MockBackend::new(vec![],|_|Err(gearwise_core::Error::Unsupported("no hardware")))),vec![]);manager.add_virtual(vec![device]);let service=Service::default();*service.0.lock().unwrap()=Some(manager);
 let cancelled=Arc::new(AtomicBool::new(false));service.1.lock().unwrap().insert(id.clone(),Cleanup{source:crate::host_macros::Source{vendor:0x1532,product:0x00b4,path:"simulation:retained-route".into()},settings:vec![],expected:Default::default(),software_input:false,cancelled:cancelled.clone(),shift:Arc::new(Mutex::new(None)),journal:None,serial:None});
 log.lock().unwrap().clear();let root=std::env::temp_dir().join(format!("foreign-plan-{}",crate::profiles::new_id()));let runtime=crate::macro_runtime::Runtime::default();let error=match service.apply_at(&id,&plan(0x00c1),&runtime,&root,None){Ok(_)=>panic!("foreign model accepted"),Err(e)=>e};assert!(error.contains("不一致"));assert!(!cancelled.load(Ordering::Acquire));assert!(service.1.lock().unwrap().contains_key(&id));assert!(log.lock().unwrap().is_empty());assert!(!root.exists());runtime.shutdown();
}
fn plan(pid:u16)->Plan{Plan{vendor:0x1532,product:pid,settings:vec![Setting::Dpi(DpiState{stages:vec![DpiStage::uniform(800),DpiStage{x:1601,y:2401}],active:1}),Setting::ReportRate(if pid==0x00c1{8000}else{500})],macros:vec![],source:None}}
#[test]fn all_six_exact_viper_ids_reach_real_driver_dpi_stages_and_polling(){
 for pid in PIDS {
  let p=plan(pid);validate(&p).unwrap();let(mut device,log)=simulated(0x1532,pid).unwrap();log.lock().unwrap().clear();
  for setting in &p.settings{device.apply(setting).unwrap();}
  let state=device.read_state().unwrap();assert_eq!(state.dpi,Some(if let Setting::Dpi(d)=&p.settings[0]{d.clone()}else{unreachable!()}));assert_eq!(state.report_rate,Some(if pid==0x00c1{8000}else{500}));
  let log=log.lock().unwrap();assert!(log.iter().any(|r|matches!(r,MockRecord::SetFeature(bytes) if bytes[7..9]==[4,6]&&bytes[6]==38)));
  if pid==0x00c1{let selectors=log.iter().filter_map(|r|if let MockRecord::SetFeature(b)=r{(b[7..9]==[0,0x40]).then_some(b[9])}else{None}).collect::<Vec<_>>();assert_eq!(selectors,vec![0,1]);}
 }
}
#[test]fn ultimate_lighting_modes_brightness_and_color_use_native_packets(){
 for pid in [0x007a,0x007b]{
  let(mut device,log)=simulated(0x1532,pid).unwrap();
  let effects=[LightingEffect::Off,LightingEffect::Static{color:Rgb::new(20,40,60)},LightingEffect::Breathing{color:Rgb::new(70,80,90),period_ms:4000},LightingEffect::ColorCycle{period_ms:10000},LightingEffect::Reactive{color:Rgb::new(100,110,120),duration_ms:1000}];
  for (index,effect) in effects.into_iter().enumerate(){
   log.lock().unwrap().clear();let state=ZoneState{effect,brightness:60};let p=Plan{settings:vec![Setting::Lighting{zone:4,state}],..plan(pid)};validate(&p).unwrap();device.apply(&p.settings[0]).unwrap();assert_eq!(device.read_state().unwrap().lighting[&4],state);
   let code=[0,1,2,3,5][index];let packets=log.lock().unwrap();assert!(packets.iter().any(|r|matches!(r,MockRecord::SetFeature(b) if b[7..9]==[0x0f,2]&&b[9..12]==[1,4,code])));assert!(packets.iter().any(|r|matches!(r,MockRecord::SetFeature(b) if b[7..9]==[0x0f,4]&&b[9..12]==[1,4,153])));
  }
 }
}
#[test]fn viper_v3_native_buttons_use_dedicated_transaction_and_get_readback(){
 for pid in [0x00c0,0x00c1]{let(mut device,log)=simulated(0x1532,pid).unwrap();for action in [ButtonAction::Mouse(MouseButton::Right),ButtonAction::Disabled,ButtonAction::Default]{
  log.lock().unwrap().clear();let p=Plan{settings:vec![Setting::Button{id:4,action:action.clone()}],..plan(pid)};validate(&p).unwrap();device.apply(&p.settings[0]).unwrap();let actual=device.read_state().unwrap().buttons[&4].clone();assert_eq!(actual,if action==ButtonAction::Default{ButtonAction::Mouse(MouseButton::Back)}else{action});
  assert!(log.lock().unwrap().iter().any(|r|matches!(r,MockRecord::SetFeature(b) if b[7..9]==[2,0x0c]&&b[2]==0x10&&b[9..12]==[1,4,0])));
 }}
}
#[test]fn unverified_actions_foreign_models_and_non_rgb_models_never_write(){
 for pid in PIDS{let(mut device,log)=simulated(0x1532,pid).unwrap();log.lock().unwrap().clear();
  for setting in [Setting::Button{id:1,action:ButtonAction::Disabled},Setting::Button{id:4,action:ButtonAction::Key{usage:0x68,mods:Modifiers(0)}}]{let p=Plan{settings:vec![setting.clone()],..plan(pid)};assert!(validate(&p).is_err());assert!(device.apply(&setting).is_err());}
  if !matches!(pid,0x007a|0x007b){let s=Setting::Lighting{zone:4,state:ZoneState{effect:LightingEffect::Static{color:Rgb::new(1,2,3)},brightness:100}};assert!(validate(&Plan{settings:vec![s.clone()],..plan(pid)}).is_err());assert!(device.apply(&s).is_err());}
  assert!(log.lock().unwrap().is_empty());
 }assert!(!supported(0x046d,0x00c1));assert!(!supported(0x1532,0xffff));
}
#[test]fn native_viper_sessions_roundtrip_complete_scoped_plans_and_emit_ui_fixtures(){
 let mut fixtures=Vec::new();
 for pid in PIDS{
  let(device,_)=simulated(0x1532,pid).unwrap();let id=device.descriptor().id.clone();let mut manager=DeviceManager::new(Box::new(MockBackend::new(vec![],|_|Err(gearwise_core::Error::Unsupported("no hardware")))),vec![]);manager.add_virtual(vec![device]);let service=Service::default();*service.0.lock().unwrap()=Some(manager);let runtime=crate::macro_runtime::Runtime::default();let root=std::env::temp_dir().join(format!("viper-session-{}",crate::profiles::new_id()));
  let mut p=plan(pid);if matches!(pid,0x007a|0x007b){p.settings.push(Setting::Lighting{zone:4,state:ZoneState{effect:LightingEffect::Static{color:Rgb::new(110,231,208)},brightness:80}});}if matches!(pid,0x00c0|0x00c1){p.settings.push(Setting::Button{id:4,action:ButtonAction::Mouse(MouseButton::Forward)});}
  service.apply_at(&id,&p,&runtime,&root,None).unwrap();let view=service.read(&id,&runtime).unwrap();assert_eq!(view.session.as_ref().unwrap().plan,p);assert_eq!(view.session.as_ref().unwrap().software_state,"hardware");assert!(!view.hardware_tested);fixtures.push(serde_json::to_value(view).unwrap());
  let path=root.join("saved-plan.json");std::fs::write(&path,serde_json::to_vec(&p).unwrap()).unwrap();let restored:Plan=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();assert_eq!(restored,p);assert!(!supported(0x3837,pid));runtime.shutdown();std::fs::remove_dir_all(root).unwrap();
 }
 if let Some(path)=std::env::var_os("OPEN_HUB_VIPER_FIXTURE"){std::fs::write(path,serde_json::to_vec_pretty(&fixtures).unwrap()).unwrap();}
}
