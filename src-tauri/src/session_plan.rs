//! Desired session settings are distinct from raw device readback. In particular,
//! native F13 triggers are never substituted for a host macro's saved definition.
use super::*;
use crate::macro_runtime::Runtime;

#[derive(Clone)]pub(super) struct Session {descriptor:DeviceDescriptor,plan:Plan,needs_software:bool}
#[derive(Clone,Serialize)]pub struct SessionView {pub plan:Plan,pub software_state:&'static str,pub temporary_dpi:bool}
fn same_device(a:&DeviceDescriptor,b:&DeviceDescriptor)->bool{
 (a.vendor_id,a.product_id,a.id.as_str(),a.serial.as_deref())==(b.vendor_id,b.product_id,b.id.as_str(),b.serial.as_deref())
}
fn put(settings:&mut Vec<Setting>,value:Setting){
 let is_same=|s:&Setting|matches!((s,&value),(Setting::Dpi(_),Setting::Dpi(_))|(Setting::ReportRate(_),Setting::ReportRate(_)));
 if let Some(index)=settings.iter().position(is_same){settings[index]=value;}else{settings.push(value);}
}
impl Service {
 pub(super) fn remember_session(&self,out:&View,plan:&Plan,needs_software:bool)->Result<(),String>{
  let session=Session{descriptor:out.descriptor.clone(),plan:plan.clone(),needs_software};
  self.3.lock().map_err(|_|"会话方案锁异常")?.insert(out.descriptor.id.clone(),session);Ok(())
 }
 pub(super) fn attach_session(&self,out:&mut View,runtime:&Runtime)->Result<(),String>{
  let cached=self.3.lock().map_err(|_|"会话方案锁异常")?.get(&out.descriptor.id).cloned();
  let Some(session)=cached.filter(|s|same_device(&s.descriptor,&out.descriptor))else{out.session=None;return Ok(())};
  let cleanup=self.1.lock().map_err(|_|"宏恢复记录锁异常")?.get(&out.descriptor.id).cloned();
  let mut plan=session.plan;
  let mut temporary_dpi=false;let mut dpi=out.state.dpi.clone();
  if let Some(cleanup)=cleanup.as_ref(){
   let original=cleanup.shift.lock().map_err(|_|"狙击 DPI 锁异常")?.clone();
   if let(Some(original),Some(journal))=(original,cleanup.journal.as_ref()){
    if journal.record()?.shift.is_some_and(|shift|out.state.dpi.as_ref()==Some(&shift.expected)){
     temporary_dpi=true;dpi=Some(original);
    }
   }
  }
  if let Some(dpi)=dpi {put(&mut plan.settings,Setting::Dpi(dpi));}
  if let Some(rate)=out.state.report_rate {put(&mut plan.settings,Setting::ReportRate(rate));}
  let software_state=if !session.needs_software{"hardware"}else if plan.source.as_ref().is_some_and(|s|runtime.is_armed(s))&&cleanup.as_ref().is_some_and(|c|!c.cancelled.load(Ordering::Acquire)){"running"}else{"stopped"};
  out.session=Some(SessionView{plan,software_state,temporary_dpi});Ok(())
 }
}

#[cfg(test)]mod tests {
 use super::*;
 use gearwise_core::{Device,transport::MockBackend,model::{DpiStage,DpiState},profile::{Macro,MacroMode,MacroStep}};
 #[derive(Clone,Default)]struct Rec(Arc<Mutex<Vec<String>>>);
 impl gearwise_input::InputSink for Rec {
  fn key_down(&mut self,k:u8)->gearwise_core::Result<()>{self.0.lock().unwrap().push(format!("d{k}"));Ok(())}
  fn key_up(&mut self,k:u8)->gearwise_core::Result<()>{self.0.lock().unwrap().push(format!("u{k}"));Ok(())}
  fn mouse_down(&mut self,_:gearwise_core::model::MouseButton)->gearwise_core::Result<()>{panic!("unexpected mouse output")}
  fn mouse_up(&mut self,_:gearwise_core::model::MouseButton)->gearwise_core::Result<()>{panic!("unexpected mouse output")}
  fn scroll(&mut self,_:i32,_:i32)->gearwise_core::Result<()>{panic!("unexpected scroll")}
  fn media(&mut self,_:u16)->gearwise_core::Result<()>{panic!("unexpected media")}
 }
 fn scenario(v:u16,p:u16)->(Service,String,std::path::PathBuf,Runtime,Rec,Plan,MockLog){
  let(device,log)=simulated(v,p).unwrap();let id=device.descriptor().id.clone();let mut manager=DeviceManager::new(Box::new(MockBackend::new(vec![],|_|Err(gearwise_core::Error::Unsupported("no physical HID")))),vec![]);manager.add_virtual(vec![device]);let s=Service::default();*s.0.lock().unwrap()=Some(manager);
  let root=std::env::temp_dir().join(format!("session-plan-{}",crate::profiles::new_id()));let rec=Rec::default();let rt=Runtime::simulation(Arc::new(Mutex::new(rec.clone())));
  let plan=Plan{vendor:v,product:p,settings:vec![Setting::Dpi(DpiState{stages:vec![DpiStage::uniform(1600)],active:0}),Setting::ReportRate(1000),Setting::Button{id:if v==0x1532{64}else{4},action:ButtonAction::Macro("original".into())}],macros:vec![Macro{id:"original".into(),name:"Original named macro".into(),mode:MacroMode::Once,steps:vec![MacroStep::KeyDown(4),MacroStep::KeyUp(4)]}],source:Some(crate::host_macros::Source{vendor:v,product:p,path:format!("simulation:session-{v}")})};(s,id,root,rt,rec,plan,log)
 }
 fn press(rt:&Runtime,p:&Plan,down:bool){let source=p.source.clone().unwrap();if p.vendor==0x1b1c{rt.simulate_event(crate::raw_input::Event::Buttons{source,mask:if down{8}else{0}})}else{rt.simulate_event(crate::raw_input::Event::Key{source,vkey:0x7c,down})}}
 fn wait(rec:&Rec,n:usize){let end=std::time::Instant::now()+std::time::Duration::from_secs(1);while rec.0.lock().unwrap().len()<n&&std::time::Instant::now()<end{std::thread::sleep(std::time::Duration::from_millis(1));}assert_eq!(rec.0.lock().unwrap().len(),n);}
 fn clean(s:&Service,rt:&Runtime,root:&std::path::Path){s.stop_software(rt).unwrap();rt.shutdown();std::fs::remove_dir_all(root).unwrap();}
 #[test]fn native_read_full_plan_edit_save_and_reapply_keeps_macros_for_all_three_models(){
  let mut fixtures=Vec::new();
  for(v,p)in[(0x1532,0x00b4),(0x1b1c,0x1b2e),(0x1038,0x1720)]{
   let(s,id,root,rt,rec,plan,_)=scenario(v,p);s.apply_at(&id,&plan,&rt,&root,None).unwrap();
   let view=s.read(&id,&rt).unwrap();let session=view.session.as_ref().unwrap();assert_eq!(session.software_state,"running");assert_eq!(session.plan.macros,plan.macros);assert_eq!(session.plan.source,plan.source);assert_eq!(session.plan.settings,plan.settings);
   if v!=0x1b1c{assert!(matches!(view.state.buttons[&if v==0x1532{64}else{4}],ButtonAction::Key{usage:0x68,..}));}
   fixtures.push(serde_json::to_value(&view).unwrap());
   let encoded=serde_json::to_vec(&session.plan).unwrap();let mut edited:Plan=serde_json::from_slice(&encoded).unwrap();put(&mut edited.settings,Setting::Dpi(DpiState{stages:vec![DpiStage::uniform(2400)],active:0}));
   s.apply_at(&id,&edited,&rt,&root,None).unwrap();press(&rt,&edited,true);wait(&rec,2);press(&rt,&edited,false);assert_eq!(*rec.0.lock().unwrap(),vec!["d4","u4"]);assert_eq!(s.read(&id,&rt).unwrap().state.dpi.unwrap().stages[0].x,2400);
   s.stop_software(&rt).unwrap();let stopped=s.read(&id,&rt).unwrap().session.unwrap();assert_eq!(stopped.software_state,"stopped");assert_eq!(stopped.plan.macros,plan.macros);rt.shutdown();std::fs::remove_dir_all(root).unwrap();
  }
  if let Some(path)=std::env::var_os("OPEN_HUB_SESSION_FIXTURE"){std::fs::write(path,serde_json::to_vec_pretty(&fixtures).unwrap()).unwrap();}
 }
 #[test]fn temporary_sniper_dpi_is_not_saved_as_the_base_dpi(){
  let(s,id,root,rt,_,mut plan,_)=scenario(0x1b1c,0x1b2e);plan.settings.push(Setting::Button{id:8,action:ButtonAction::Dpi(DpiFunction::Shift(400))});s.apply_at(&id,&plan,&rt,&root,None).unwrap();let source=plan.source.clone().unwrap();rt.simulate_event(crate::raw_input::Event::Buttons{source:source.clone(),mask:128});
  let view=s.read(&id,&rt).unwrap();assert_eq!(view.state.dpi.unwrap().stages[0].x,400);let session=view.session.unwrap();assert!(session.temporary_dpi);assert_eq!(session.plan.settings.iter().find_map(|s|if let Setting::Dpi(d)=s{Some(d.stages[0].x)}else{None}),Some(1600));
  rt.simulate_event(crate::raw_input::Event::Buttons{source,mask:0});assert!(!s.read(&id,&rt).unwrap().session.unwrap().temporary_dpi);clean(&s,&rt,&root);
 }
 #[test]fn removed_source_is_stopped_and_full_desired_plan_remains_editable(){
  let(s,id,root,rt,rec,plan,_)=scenario(0x1532,0x00b4);s.apply_at(&id,&plan,&rt,&root,None).unwrap();rt.simulate_event(crate::raw_input::Event::Removed{path:plan.source.as_ref().unwrap().path.clone()});
  let view=s.read(&id,&rt).unwrap();assert_eq!(view.session.as_ref().unwrap().software_state,"stopped");assert_eq!(view.session.unwrap().plan.macros,plan.macros);assert!(s.1.lock().unwrap()[&id].cancelled.load(Ordering::Acquire));press(&rt,&plan,true);assert!(rec.0.lock().unwrap().is_empty());press(&rt,&plan,false);clean(&s,&rt,&root);
 }
 #[test]fn session_identity_does_not_leak_to_another_serial_or_model(){
  let(s,id,root,rt,_,plan,_)=scenario(0x1532,0x00b4);s.apply_at(&id,&plan,&rt,&root,None).unwrap();let mut view=s.read(&id,&rt).unwrap();view.descriptor.serial=Some("another-unit".into());s.attach_session(&mut view,&rt).unwrap();assert!(view.session.is_none());view.descriptor.vendor_id=0x046d;s.attach_session(&mut view,&rt).unwrap();assert!(view.session.is_none());clean(&s,&rt,&root);
 }
 #[test]fn a_new_service_does_not_claim_an_old_unsaved_plan_is_running(){
  let(s,id,root,rt,_,plan,_)=scenario(0x1038,0x1720);s.apply_at(&id,&plan,&rt,&root,None).unwrap();let fresh=Service::default();let mut view=s.read(&id,&rt).unwrap();fresh.attach_session(&mut view,&rt).unwrap();assert!(view.session.is_none());clean(&s,&rt,&root);
 }
 #[test]fn stopping_one_mouse_keeps_the_other_brands_route_and_plan_running(){
  let(s,id,root,rt,rec,plan,_)=scenario(0x1532,0x00b4);s.apply_at(&id,&plan,&rt,&root,None).unwrap();let(device,_)=simulated(0x1038,0x1720).unwrap();let other_id=device.descriptor().id.clone();s.with(|m|{m.add_virtual(vec![device]);Ok(())}).unwrap();let mut other=plan.clone();other.vendor=0x1038;other.product=0x1720;other.source=Some(crate::host_macros::Source{vendor:0x1038,product:0x1720,path:"simulation:other-running".into()});other.settings[2]=Setting::Button{id:4,action:ButtonAction::Macro("original".into())};s.apply_at(&other_id,&other,&rt,&root,None).unwrap();
  s.stop_device_software(&id,&rt).unwrap();assert_eq!(s.read(&id,&rt).unwrap().session.unwrap().software_state,"stopped");assert_eq!(s.read(&other_id,&rt).unwrap().session.unwrap().software_state,"running");assert_eq!(rt.status().source_count,1);press(&rt,&other,true);wait(&rec,2);press(&rt,&other,false);clean(&s,&rt,&root);
 }
}
