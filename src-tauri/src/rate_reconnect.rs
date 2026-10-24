//! Durable, explicit continuation across M65 poll-rate USB re-enumeration.
//! Loading or scanning a pending plan never executes it.
use super::*;
use crate::{host_macros::Source,macro_runtime::Runtime};
use std::{path::{Path,PathBuf},io::{Read,Write}};

#[derive(Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingRate {version:u8,pub target:DeviceDescriptor,pub plan:Plan,pub requested:u16,pub sent:bool,pub error:Option<String>,#[serde(default="initial_attempt")]pub attempt:u32}
fn initial_attempt()->u32{1}
#[derive(Serialize)]pub struct PendingItem{pub key:String,pub revision:String,pub pending:PendingRate}
#[derive(Serialize)]pub struct PendingList{pub records:Vec<PendingItem>,pub errors:Vec<String>}
#[derive(Serialize)]pub struct Candidate{pub descriptor:DeviceDescriptor,pub rate:Option<u16>,pub revision:String,pub blocked:Option<String>}
#[derive(Serialize)]pub struct PendingCandidates{pub record:PendingItem,pub devices:Vec<Candidate>,pub needs_source:bool,pub warnings:Vec<String>}
struct Entry{path:PathBuf,pending:PendingRate}
pub(super) fn root()->Result<PathBuf,String>{Ok(PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA unavailable")?).join("Open HUB"))}
fn directory(root:&Path)->PathBuf{root.join("rate-reconnect")}
fn value(p:&PendingRate)->Result<serde_json::Value,String>{serde_json::to_value(p).map_err(|e|e.to_string())}
fn revision(p:&PendingRate)->Result<String,String>{serde_json::to_string(&value(p)?).map_err(|e|e.to_string())}
fn same_revision(p:&PendingRate,expected:&str)->Result<(),String>{
    if expected.len()>1024*1024{return Err("待续接方案过大".into());}
    if serde_json::from_str::<serde_json::Value>(expected).map_err(|_|"待续接方案版本无效")?!=value(p)?{return Err("待续接方案已变化，请重新查看".into());}Ok(())
}
impl PendingRate {
 fn validate(&self)->Result<(),String>{
    if self.version!=1||self.attempt==0||(self.target.vendor_id,self.target.product_id)!=(0x1b1c,0x1b2e)||(self.plan.vendor,self.plan.product)!=(0x1b1c,0x1b2e)
      ||self.target.id.is_empty()||self.target.id.len()>4096||self.target.serial.as_ref().is_some_and(|s|s.len()>4096)||self.plan.source.is_some()
      ||self.error.as_ref().is_some_and(|e|e.len()>8192){return Err("待续接记录的型号、来源或大小无效".into());}
    validate(&self.plan)?;
    if self.plan.settings.iter().filter_map(|s|if let Setting::ReportRate(r)=s{Some(*r)}else{None}).collect::<Vec<_>>()!=[self.requested]{return Err("待续接回报率与方案不一致".into());}Ok(())
 }
}
impl Entry {
 fn key(&self)->Result<String,String>{self.path.file_name().and_then(|s|s.to_str()).map(str::to_owned).ok_or("待续接文件名无效".into())}
 fn item(&self)->Result<PendingItem,String>{Ok(PendingItem{key:self.key()?,revision:revision(&self.pending)?,pending:self.pending.clone()})}
 fn persist(&self)->Result<(),String>{
    self.pending.validate()?;let bytes=serde_json::to_vec_pretty(&self.pending).map_err(|e|e.to_string())?;
    if bytes.len()>1024*1024{return Err("待续接方案超过大小限制".into());}
    std::fs::create_dir_all(self.path.parent().ok_or("待续接目录无效")?).map_err(|e|e.to_string())?;
    let tmp=self.path.with_extension("json.tmp");let mut f=std::fs::File::create(&tmp).map_err(|e|e.to_string())?;
    f.write_all(&bytes).and_then(|_|f.sync_all()).map_err(|e|e.to_string())?;drop(f);std::fs::rename(tmp,&self.path).map_err(|e|e.to_string())
 }
 fn finish(&self)->Result<(),String>{std::fs::remove_file(&self.path).map_err(|e|format!("无法清理待续接记录：{e}"))}
}
fn load(root:&Path)->(Vec<Entry>,Vec<String>){
 let entries=match std::fs::read_dir(directory(root)){Ok(v)=>v,Err(e)if e.kind()==std::io::ErrorKind::NotFound=>return(vec![],vec![]),Err(e)=>return(vec![],vec![e.to_string()])};
 let mut records=Vec::new();let mut errors=Vec::new();
 for entry in entries.take(128){let read=(||->Result<Option<Entry>,String>{let path=entry.map_err(|e|e.to_string())?.path();if path.extension().and_then(|s|s.to_str())!=Some("json"){return Ok(None);}
    let meta=std::fs::symlink_metadata(&path).map_err(|e|e.to_string())?;if !meta.is_file()||meta.len()>1024*1024{return Err("待续接记录大小或类型无效".into());}
    let mut bytes=Vec::new();std::fs::File::open(&path).map_err(|e|e.to_string())?.take(1024*1024+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
    if bytes.len()>1024*1024{return Err("待续接记录读取期间过大".into());}
    let pending:PendingRate=serde_json::from_slice(&bytes).map_err(|e|format!("待续接记录损坏，已保留文件：{e}"))?;pending.validate()?;Ok(Some(Entry{path,pending}))})();
    match read{Ok(Some(e))=>records.push(e),Ok(None)=>{},Err(e)=>errors.push(e)}
 }(records,errors)
}
fn find(root:&Path,key:&str,expected:&str)->Result<Entry,String>{
 if key.len()>256{return Err("待续接文件名无效".into());}
 let entry=load(root).0.into_iter().find(|e|e.path.file_name().and_then(|v|v.to_str())==Some(key)).ok_or("待续接方案不存在或已完成，请刷新")?;
 same_revision(&entry.pending,expected)?;Ok(entry)
}
fn target(p:&PendingRate,d:&DeviceDescriptor)->Result<(),String>{
 if(d.vendor_id,d.product_id)!=(p.target.vendor_id,p.target.product_id){return Err("续接目标具体型号不匹配".into());}
 if p.target.serial.as_deref().filter(|s|!s.trim().is_empty()).is_some_and(|s|d.serial.as_deref()!=Some(s)){return Err("续接目标序列号不匹配，未写入其他鼠标".into());}Ok(())
}
fn identity(manager:&mut DeviceManager,id:&str)->Result<(DeviceDescriptor,Option<u16>,String),String>{
 if manager.devices().filter(|d|d.descriptor().id==raw_id(id)).count()!=1{return Err("鼠标已断开或设备标识不唯一，请刷新".into());}
 let d=manager.device_mut(raw_id(id)).ok_or("鼠标已断开")?;let descriptor=d.descriptor().clone();let rate=d.read_state().map_err(|e|e.to_string())?.report_rate;
 let token=serde_json::to_string(&serde_json::json!({"id":descriptor.id,"serial":descriptor.serial,"vendor":descriptor.vendor_id,"product":descriptor.product_id,"rate":rate})).map_err(|e|e.to_string())?;Ok((descriptor,rate,token))
}
fn selection(root:&Path,manager:&mut DeviceManager,key:&str,record_revision:&str,id:&str,candidate_revision:&str)->Result<Entry,String>{
 let entry=find(root,key,record_revision)?;
 let descriptor=manager.device_mut(raw_id(id)).ok_or("鼠标已断开")?.descriptor().clone();target(&entry.pending,&descriptor)?;
 let(_,_,actual)=identity(manager,id)?;
 if candidate_revision.len()>16384||serde_json::from_str::<serde_json::Value>(candidate_revision).map_err(|_|"重连设备确认无效")?!=serde_json::from_str::<serde_json::Value>(&actual).map_err(|e|e.to_string())?{return Err("重连设备或回报率已变化，请重新查看并确认".into());}Ok(entry)
}

#[cfg(test)]mod tests {
 use super::*;
 use gearwise_core::{Device,Driver,HidBackend,HidTransport,HidInterfaceInfo,Error,model::{Vendor,DpiState,DpiStage},profile::{Macro,MacroMode,MacroStep}};
 #[derive(Clone,Default)]struct Rec(Arc<Mutex<Vec<String>>>);
 impl gearwise_input::InputSink for Rec {
  fn key_down(&mut self,k:u8)->gearwise_core::Result<()>{self.0.lock().unwrap().push(format!("d{k}"));Ok(())}
  fn key_up(&mut self,k:u8)->gearwise_core::Result<()>{self.0.lock().unwrap().push(format!("u{k}"));Ok(())}
  fn mouse_down(&mut self,_:gearwise_core::model::MouseButton)->gearwise_core::Result<()>{panic!("unexpected mouse output")}
  fn mouse_up(&mut self,_:gearwise_core::model::MouseButton)->gearwise_core::Result<()>{panic!("unexpected mouse output")}
  fn scroll(&mut self,_:i32,_:i32)->gearwise_core::Result<()>{panic!("unexpected scroll")}
  fn media(&mut self,_:u16)->gearwise_core::Result<()>{panic!("unexpected media")}
 }
 struct BusState {online:bool,path:String,serial:Option<String>,rate:u16,epoch:u64,fail_open:bool,fail_rate:bool,fail_setting:bool,rate_writes:usize,setting_writes:usize,logs:Vec<MockLog>}
 #[derive(Clone)]struct Bus(Arc<Mutex<BusState>>);
 impl HidBackend for Bus {
  fn enumerate(&self)->gearwise_core::Result<Vec<HidInterfaceInfo>>{let b=self.0.lock().unwrap();Ok(if b.online{vec![HidInterfaceInfo{path:b.path.clone(),vendor_id:0x1b1c,product_id:0x1b2e,serial:b.serial.clone(),..Default::default()}]}else{vec![]})}
  fn open(&self,_:&str)->gearwise_core::Result<Box<dyn HidTransport>>{Err(Error::Unsupported("test driver creates the real NXP simulated transport"))}
 }
 struct RebootDevice{inner:Box<dyn Device>,descriptor:DeviceDescriptor,bus:Bus,epoch:u64}
 impl RebootDevice {fn alive(&self)->gearwise_core::Result<()>{let b=self.bus.0.lock().unwrap();if b.online&&b.epoch==self.epoch{Ok(())}else{Err(Error::Disconnected)}}}
 impl Device for RebootDevice {
  fn descriptor(&self)->&DeviceDescriptor{&self.descriptor}
  fn capabilities(&self)->&Capabilities{self.inner.capabilities()}
  fn read_state(&mut self)->gearwise_core::Result<DeviceState>{self.alive()?;self.inner.read_state()}
  fn apply(&mut self,s:&Setting)->gearwise_core::Result<()>{
   self.alive()?;
   if let Setting::ReportRate(rate)=s{
    self.inner.apply(s)?;let mut b=self.bus.0.lock().unwrap();b.rate_writes+=1;b.online=false;b.epoch+=1;
    if b.fail_rate{return Err(Error::Disconnected);}b.rate=*rate;return Ok(());
   }
   {let mut b=self.bus.0.lock().unwrap();if b.fail_setting{return Err(Error::Disconnected);}b.setting_writes+=1;}
   self.inner.apply(s)
  }
  fn stop_software_actions(&mut self)->gearwise_core::Result<()>{self.alive()?;self.inner.stop_software_actions()}
  fn restore_hardware_input(&mut self)->gearwise_core::Result<()>{self.alive()?;self.inner.restore_hardware_input()}
 }
 struct RebootDriver(Bus);
 impl Driver for RebootDriver {
  fn name(&self)->&'static str{"Scripted hotplug, real Corsair NXP driver"}
  fn vendor(&self)->Vendor{Vendor::Corsair}
  fn probe(&self,i:&HidInterfaceInfo)->bool{(i.vendor_id,i.product_id)==(0x1b1c,0x1b2e)}
  fn open(&self,i:&HidInterfaceInfo,_:&dyn HidBackend)->gearwise_core::Result<Vec<Box<dyn Device>>>{
   let(rate,epoch)={let b=self.0.0.lock().unwrap();if !b.online||b.fail_open{return Err(Error::Disconnected);}(b.rate,b.epoch)};
   let(inner,log)=gearwise_corsair::simulated_m65_pro_at_rate(rate)?;let mut descriptor=inner.descriptor().clone();descriptor.id=format!("simulation:control:{}",i.path);descriptor.serial=i.serial.clone();self.0.0.lock().unwrap().logs.push(log);
   Ok(vec![Box::new(RebootDevice{inner,descriptor,bus:self.0.clone(),epoch})])
  }
 }
 fn setup(serial:Option<&str>)->(Service,Bus,PathBuf,Runtime,Rec){
  let bus=Bus(Arc::new(Mutex::new(BusState{online:true,path:"old-path".into(),serial:serial.map(str::to_owned),rate:1000,epoch:0,fail_open:false,fail_rate:false,fail_setting:false,rate_writes:0,setting_writes:0,logs:vec![]})));
  let mut manager=DeviceManager::new(Box::new(bus.clone()),vec![Box::new(RebootDriver(bus.clone()))]);assert_eq!(manager.scan().added.len(),1);
  let service=Service::default();*service.0.lock().unwrap()=Some(manager);let root=std::env::temp_dir().join(format!("rate-reconnect-{}",crate::profiles::new_id()));let rec=Rec::default();let runtime=Runtime::simulation(Arc::new(Mutex::new(rec.clone())));(service,bus,root,runtime,rec)
 }
 fn plan()->Plan{Plan{vendor:0x1b1c,product:0x1b2e,settings:vec![Setting::Dpi(DpiState{stages:vec![DpiStage::uniform(2400)],active:0}),Setting::ReportRate(500)],macros:vec![],source:None}}
 fn id(service:&Service)->String{service.peers()[0].id.clone()}
 fn begin_plan(service:&Service,root:&Path,runtime:&Runtime,plan:&Plan)->PendingItem{
  let error=service.apply_at(&id(service),plan,runtime,root,None).err().expect("rate change must defer continuation");assert!(error.contains("其余 DPI、按键和宏尚未应用"));let list=service.reconnect_list_at(root).unwrap();assert!(list.errors.is_empty());assert_eq!(list.records.len(),1);list.records.into_iter().next().unwrap()
 }
 fn reconnect(bus:&Bus){let mut b=bus.0.lock().unwrap();b.online=true;b.path="new-path".into();}
 fn candidate(service:&Service,root:&Path,item:&PendingItem)->Candidate{let mut found=service.reconnect_candidates_at(root,&item.key,&item.revision).unwrap();assert_eq!(found.devices.len(),1);found.devices.remove(0)}
 fn clean(root:&Path,runtime:&Runtime){runtime.shutdown();if root.exists(){std::fs::remove_dir_all(root).unwrap();}}
 #[test]fn changed_rate_reopens_before_applying_the_remaining_real_driver_plan(){
  let(s,b,root,r,rec)=setup(Some("unit-a"));let item=begin_plan(&s,&root,&r,&plan());assert!(item.pending.sent);assert!(s.peers().is_empty());assert_eq!(b.0.lock().unwrap().rate_writes,1);assert_eq!(b.0.lock().unwrap().setting_writes,0);
  assert!(s.reconnect_candidates_at(&root,&item.key,&item.revision).unwrap().devices.is_empty());reconnect(&b);let c=candidate(&s,&root,&item);assert_eq!(c.rate,Some(500));assert!(c.blocked.is_none());
  let out=s.reconnect_resume_at(&root,&item.key,&item.revision,&c.descriptor.id,&c.revision,None,true,&r).unwrap();assert_eq!(out.state.dpi.unwrap().stages[0].x,2400);assert_eq!(out.state.report_rate,Some(500));assert_eq!(b.0.lock().unwrap().rate_writes,1);assert_eq!(b.0.lock().unwrap().setting_writes,1);assert!(s.reconnect_list_at(&root).unwrap().records.is_empty());assert!(rec.0.lock().unwrap().is_empty());clean(&root,&r);
 }
 #[test]fn uncertain_send_is_not_repeated_by_scan_or_restart_and_requires_explicit_retry(){
  let(s,b,root,r,_)=setup(None);b.0.lock().unwrap().fail_rate=true;let item=begin_plan(&s,&root,&r,&plan());assert!(!item.pending.sent);assert!(item.pending.error.is_some());reconnect(&b);let c=candidate(&s,&root,&item);assert_eq!(c.rate,Some(1000));
  assert!(s.reconnect_resume_at(&root,&item.key,&item.revision,&c.descriptor.id,&c.revision,None,true,&r).err().expect("must reject wrong rate").contains("尚未达到目标"));
  assert!(s.reconnect_retry_at(&root,&item.key,&item.revision,&c.descriptor.id,&c.revision,false).is_err());assert_eq!(b.0.lock().unwrap().rate_writes,1);
  // Identical transport errors still invalidate the previous review.
  s.reconnect_retry_at(&root,&item.key,&item.revision,&c.descriptor.id,&c.revision,true).unwrap();let failed=s.reconnect_list_at(&root).unwrap().records.remove(0);assert_eq!(failed.pending.attempt,2);assert!(!failed.pending.sent);assert_eq!(b.0.lock().unwrap().rate_writes,2);assert!(s.reconnect_candidates_at(&root,&item.key,&item.revision).is_err());
  reconnect(&b);let c=candidate(&s,&root,&failed);b.0.lock().unwrap().fail_rate=false;s.reconnect_retry_at(&root,&failed.key,&failed.revision,&c.descriptor.id,&c.revision,true).unwrap();assert_eq!(b.0.lock().unwrap().rate_writes,3);assert_eq!(b.0.lock().unwrap().setting_writes,0);
  let updated=s.reconnect_list_at(&root).unwrap().records.remove(0);assert_eq!(updated.pending.attempt,3);reconnect(&b);let c=candidate(&s,&root,&updated);s.reconnect_resume_at(&root,&updated.key,&updated.revision,&c.descriptor.id,&c.revision,None,true,&r).unwrap();assert_eq!(b.0.lock().unwrap().rate_writes,3);clean(&root,&r);
 }
 #[test]fn a_different_serial_and_a_target_swap_after_review_get_no_settings(){
  let(s,b,root,r,_)=setup(Some("unit-a"));let item=begin_plan(&s,&root,&r,&plan());reconnect(&b);let c=candidate(&s,&root,&item);
  b.0.lock().unwrap().serial=Some("unit-b".into());assert!(s.reconnect_resume_at(&root,&item.key,&item.revision,&c.descriptor.id,&c.revision,None,true,&r).err().expect("must reject replaced target").contains("序列号"));assert_eq!(b.0.lock().unwrap().setting_writes,0);let c=candidate(&s,&root,&item);assert!(c.blocked.is_some());assert_eq!(b.0.lock().unwrap().rate_writes,1);assert_eq!(s.reconnect_list_at(&root).unwrap().records.len(),1);clean(&root,&r);
 }
 #[test]fn open_failure_can_be_retried_and_plan_write_failure_keeps_the_original_pending_plan(){
  let(s,b,root,r,_)=setup(Some("unit-a"));let item=begin_plan(&s,&root,&r,&plan());reconnect(&b);b.0.lock().unwrap().fail_open=true;assert!(s.reconnect_candidates_at(&root,&item.key,&item.revision).unwrap().devices.is_empty());b.0.lock().unwrap().fail_open=false;let c=candidate(&s,&root,&item);
  b.0.lock().unwrap().fail_setting=true;assert!(s.reconnect_resume_at(&root,&item.key,&item.revision,&c.descriptor.id,&c.revision,None,true,&r).is_err());assert_eq!(s.reconnect_list_at(&root).unwrap().records[0].pending.plan,plan());b.0.lock().unwrap().fail_setting=false;let c=candidate(&s,&root,&item);s.reconnect_resume_at(&root,&item.key,&item.revision,&c.descriptor.id,&c.revision,None,true,&r).unwrap();assert_eq!(b.0.lock().unwrap().rate_writes,1);clean(&root,&r);
 }
 #[test]fn unchanged_rate_never_disconnects_and_failed_write_ahead_prevents_the_rate_write(){
  let(s,b,root,r,_)=setup(Some("unit-a"));let mut p=plan();p.settings[1]=Setting::ReportRate(1000);s.apply_at(&id(&s),&p,&r,&root,None).unwrap();assert_eq!(b.0.lock().unwrap().rate_writes,0);assert!(b.0.lock().unwrap().online);assert!(s.reconnect_list_at(&root).unwrap().records.is_empty());
  let blocked=root.join("blocked-root");std::fs::write(&blocked,b"file instead of directory").unwrap();assert!(s.apply_at(&id(&s),&plan(),&r,&blocked,None).is_err());assert_eq!(b.0.lock().unwrap().rate_writes,0);clean(&root,&r);
 }
 #[test]fn full_macro_plan_survives_restart_and_only_the_new_source_can_execute_it(){
  let(s,b,root,r,rec)=setup(None);let mut p=plan();p.settings.push(Setting::Button{id:4,action:ButtonAction::Macro("after-reboot".into())});p.macros=vec![Macro{id:"after-reboot".into(),name:"Original macro".into(),mode:MacroMode::Once,steps:vec![MacroStep::KeyDown(4),MacroStep::KeyUp(4)]}];p.source=Some(Source{vendor:0x1b1c,product:0x1b2e,path:"simulation:old-input".into()});let item=begin_plan(&s,&root,&r,&p);
  // Recreate the service manager, retaining only the durable plan and the fake bus.
  drop(s);r.shutdown();let r=Runtime::simulation(Arc::new(Mutex::new(rec.clone())));let restarted=Service::default();*restarted.0.lock().unwrap()=Some(DeviceManager::new(Box::new(b.clone()),vec![Box::new(RebootDriver(b.clone()))]));reconnect(&b);let recovered=restarted.reconnect_list_at(&root).unwrap().records.remove(0);assert_eq!(item.revision,recovered.revision);assert_eq!(recovered.pending.plan.macros,p.macros);assert!(recovered.pending.plan.source.is_none());let c=candidate(&restarted,&root,&recovered);
  assert!(restarted.reconnect_resume_at(&root,&recovered.key,&recovered.revision,&c.descriptor.id,&c.revision,None,true,&r).is_err());
  let source=Source{vendor:0x1b1c,product:0x1b2e,path:"simulation:new-input".into()};restarted.reconnect_resume_at(&root,&recovered.key,&recovered.revision,&c.descriptor.id,&c.revision,Some(source.clone()),true,&r).unwrap();
  r.simulate_event(crate::raw_input::Event::Buttons{source:p.source.unwrap(),mask:8});std::thread::sleep(std::time::Duration::from_millis(5));assert!(rec.0.lock().unwrap().is_empty());r.simulate_event(crate::raw_input::Event::Buttons{source:source.clone(),mask:8});
  let end=std::time::Instant::now()+std::time::Duration::from_secs(1);while rec.0.lock().unwrap().len()<2&&std::time::Instant::now()<end{std::thread::sleep(std::time::Duration::from_millis(1));}assert_eq!(*rec.0.lock().unwrap(),vec!["d4","u4"]);r.simulate_event(crate::raw_input::Event::Buttons{source,mask:0});restarted.stop_software(&r).unwrap();assert_eq!(b.0.lock().unwrap().rate_writes,1);clean(&root,&r);
 }
 #[test]fn stale_confirmation_traversal_and_discard_do_not_send_hardware_commands(){
  let(s,b,root,r,_)=setup(None);let item=begin_plan(&s,&root,&r,&plan());reconnect(&b);let c=candidate(&s,&root,&item);
  assert!(s.reconnect_resume_at(&root,&item.key,&item.revision,&c.descriptor.id,&c.revision,None,false,&r).is_err());assert!(s.reconnect_candidates_at(&root,"../file.json",&item.revision).is_err());assert!(s.reconnect_discard_at(&root,&item.key,"stale").is_err());
  s.reconnect_discard_at(&root,&item.key,&item.revision).unwrap();assert!(s.reconnect_list_at(&root).unwrap().records.is_empty());assert_eq!(b.0.lock().unwrap().setting_writes,0);assert_eq!(b.0.lock().unwrap().rate_writes,1);clean(&root,&r);
 }
 #[test]fn a_poll_write_does_not_trigger_cached_mode_resets_on_the_old_handle(){
  let(mut device,log)=gearwise_corsair::simulated_m65_pro();device.apply(&Setting::Button{id:4,action:ButtonAction::Disabled}).unwrap();log.lock().unwrap().clear();device.apply(&Setting::ReportRate(500)).unwrap();drop(device);
  let writes=log.lock().unwrap().iter().filter_map(|r|if let gearwise_core::transport::MockRecord::Write(b)=r{Some(b.clone())}else{None}).collect::<Vec<_>>();assert_eq!(writes.len(),1);assert_eq!(&writes[0][..6],&[0,7,10,0,0,2]);
 }
 #[test]fn a_rate_only_plan_finishes_without_resending_or_requiring_an_input_source(){
  let(s,b,root,r,_)=setup(None);let mut p=plan();p.settings=vec![Setting::ReportRate(500)];let item=begin_plan(&s,&root,&r,&p);reconnect(&b);let c=candidate(&s,&root,&item);
  let out=s.reconnect_resume_at(&root,&item.key,&item.revision,&c.descriptor.id,&c.revision,None,true,&r).unwrap();assert_eq!(out.state.report_rate,Some(500));assert_eq!(b.0.lock().unwrap().rate_writes,1);assert_eq!(b.0.lock().unwrap().setting_writes,0);assert!(s.reconnect_list_at(&root).unwrap().records.is_empty());clean(&root,&r);
 }
 #[test]fn a_pending_m65_plan_cannot_cancel_an_active_route_or_read_write_a_foreign_target(){
  let(s,b,root,r,rec)=setup(Some("unit-a"));let item=begin_plan(&s,&root,&r,&plan());reconnect(&b);let c=candidate(&s,&root,&item);
  let(mut naga,naga_log)=crate::naga::simulated();let naga_id=naga.descriptor().id.clone();let _=naga.read_state().unwrap();s.with(|m|{m.add_virtual(vec![naga]);Ok(())}).unwrap();naga_log.lock().unwrap().clear();
  assert!(s.reconnect_resume_at(&root,&item.key,&item.revision,&naga_id,"unknown",None,true,&r).is_err());assert!(naga_log.lock().unwrap().is_empty());
  let other=Plan{vendor:0x1532,product:0x00b4,settings:vec![Setting::Dpi(DpiState{stages:vec![DpiStage::uniform(1800)],active:0})],macros:vec![],source:None};assert_eq!(s.apply_at(&naga_id,&other,&r,&root,None).unwrap().state.dpi.unwrap().stages[0].x,1800);assert_eq!(b.0.lock().unwrap().setting_writes,0);
  let source=Source{vendor:0x1b1c,product:0x1b2e,path:"simulation:active-other-plan".into()};r.arm(source.clone(),vec![],crate::macro_runtime::InputMap{keys:Default::default(),buttons:vec![],dpi_buttons:vec![]},crate::macro_runtime::Hooks::default()).unwrap();
  s.1.lock().unwrap().insert(c.descriptor.id.clone(),Cleanup{source,settings:vec![],expected:Default::default(),software_input:false,cancelled:Arc::new(AtomicBool::new(false)),shift:Arc::new(Mutex::new(None)),journal:None,serial:Some("unit-a".into())});
  assert!(s.reconnect_resume_at(&root,&item.key,&item.revision,&c.descriptor.id,&c.revision,None,true,&r).is_err());assert_eq!(r.status().source_count,1);assert!(rec.0.lock().unwrap().is_empty());assert_eq!(b.0.lock().unwrap().setting_writes,0);s.1.lock().unwrap().clear();clean(&root,&r);
 }
 #[test]fn corrupt_oversized_and_cross_model_pending_files_are_preserved_and_never_executed(){
  let(s,b,root,r,_)=setup(None);let item=begin_plan(&s,&root,&r,&plan());let path=directory(&root).join(&item.key);let original=std::fs::read(&path).unwrap();
  for bytes in [b"{ broken JSON".to_vec(),vec![b' ';1024*1024+1],{let mut v:serde_json::Value=serde_json::from_slice(&original).unwrap();v["target"]["vendor_id"]=serde_json::json!(0x046d);serde_json::to_vec(&v).unwrap()}]{
   std::fs::write(&path,&bytes).unwrap();let list=s.reconnect_list_at(&root).unwrap();assert!(list.records.is_empty());assert!(!list.errors.is_empty());assert_eq!(std::fs::read(&path).unwrap(),bytes);assert_eq!(b.0.lock().unwrap().rate_writes,1);assert_eq!(b.0.lock().unwrap().setting_writes,0);
  }std::fs::write(path,original).unwrap();assert_eq!(s.reconnect_list_at(&root).unwrap().records.len(),1);clean(&root,&r);
 }
}
/// Called while the manager lock is held, before any plan writes.
pub(super) fn guard_apply(root:&Path,manager:&mut DeviceManager,id:&str,resume:Option<(&str,&str,&str)>)->Result<(),String>{
 if let Some((key,rev,candidate))=resume{let entry=selection(root,manager,key,rev,id,candidate)?;let(_,rate,_)=identity(manager,id)?;if rate!=Some(entry.pending.requested){return Err("重连后的回报率尚未达到目标，未执行其余设置".into());}return Ok(());}
 let d=manager.device_mut(raw_id(id)).ok_or("鼠标已断开")?.descriptor();
 if load(root).0.iter().any(|e|{let t=&e.pending.target;(d.vendor_id,d.product_id)==(t.vendor_id,t.product_id)&&(t.serial.as_deref().filter(|s|!s.trim().is_empty()).is_none()||t.serial==d.serial||t.id==d.id)}){
    return Err("此鼠标有回报率更改后的待续接方案，请在配置中心继续或放弃该方案，未重复发送回报率".into());
 }Ok(())
}
pub(super) fn begin(root:&Path,device:&mut dyn gearwise_core::Device,plan:&Plan)->Result<String,String>{
 let requested=plan.settings.iter().find_map(|s|if let Setting::ReportRate(r)=s{Some(*r)}else{None}).ok_or("方案缺少回报率")?;
 let mut saved=plan.clone();saved.source=None;
 let mut entry=Entry{path:directory(root).join(format!("rate-{}.json",crate::profiles::new_id())),pending:PendingRate{version:1,target:device.descriptor().clone(),plan:saved,requested,sent:false,error:None,attempt:1}};
 entry.persist()?; // Durable before the possibly-disconnecting write.
 let result=device.apply(&Setting::ReportRate(requested));entry.pending.sent=result.is_ok();entry.pending.error=result.err().map(|e|e.to_string());
 let saved=entry.persist().err().map(|e|format!("；发送后状态保存失败，发送前记录仍保留：{e}")).unwrap_or_default();
 Ok(format!("{}；其余 DPI、按键和宏尚未应用。请在配置中心的‘待续接设备方案’查看重连鼠标并继续。记录：{}{}",if entry.pending.sent{"回报率命令已发送，鼠标可能断开后重新连接"}else{"回报率发送结果不确定，未自动重试"},entry.key()?,saved))
}
pub(super) fn finish(root:&Path,key:&str)->Result<(),String>{let entry=load(root).0.into_iter().find(|e|e.path.file_name().and_then(|v|v.to_str())==Some(key)).ok_or("待续接方案已不存在")?;entry.finish()}
impl Service {
 pub fn reconnect_list(&self)->Result<PendingList,String>{self.reconnect_list_at(&root()?)}
 fn reconnect_list_at(&self,root:&Path)->Result<PendingList,String>{self.with(|_|{let(entries,mut errors)=load(root);let mut records=Vec::new();for e in entries{match e.item(){Ok(v)=>records.push(v),Err(e)=>errors.push(e)}}records.sort_by(|a,b|a.key.cmp(&b.key));Ok(PendingList{records,errors})})}
 pub fn reconnect_candidates(&self,key:&str,revision:&str)->Result<PendingCandidates,String>{self.reconnect_candidates_at(&root()?,key,revision)}
 fn reconnect_candidates_at(&self,root:&Path,key:&str,revision:&str)->Result<PendingCandidates,String>{self.with(|manager|{
    let entry=find(root,key,revision)?;let report=manager.retry_scan();let mut warnings=report.issues.into_iter().map(|i|i.message).collect::<Vec<_>>();let mut devices=Vec::new();
    let ids=manager.devices().filter(|d|(d.descriptor().vendor_id,d.descriptor().product_id)==(entry.pending.target.vendor_id,entry.pending.target.product_id)).map(|d|d.descriptor().id.clone()).collect::<Vec<_>>();
    for id in ids{match identity(manager,&id){Ok((descriptor,rate,revision))=>{let blocked=target(&entry.pending,&descriptor).err();devices.push(Candidate{descriptor,rate,revision,blocked});},Err(e)=>warnings.push(e)}}
    let compiled=compile(&entry.pending.plan)?;Ok(PendingCandidates{record:entry.item()?,devices,needs_source:compiled.software_input||!compiled.bindings.is_empty(),warnings})
 })}
 fn check_inactive(&self,id:&str)->Result<(),String>{if self.1.lock().map_err(|_|"宏恢复记录锁异常")?.contains_key(raw_id(id)){return Err("此鼠标仍有运行或待恢复的软件动作，请先停止并完成恢复".into());}Ok(())}
 pub fn reconnect_resume(&self,key:&str,revision:&str,id:&str,candidate:&str,source:Option<Source>,confirmed:bool,runtime:&Runtime)->Result<View,String>{self.reconnect_resume_at(&root()?,key,revision,id,candidate,source,confirmed,runtime)}
 fn reconnect_resume_at(&self,root:&Path,key:&str,revision:&str,id:&str,candidate:&str,source:Option<Source>,confirmed:bool,runtime:&Runtime)->Result<View,String>{
    if !confirmed{return Err("请先核对重连目标和待续接方案并确认".into());}
    let mut plan=self.with(|manager|{
        let entry=selection(root,manager,key,revision,id,candidate)?;self.check_inactive(id)?;
        // Reopen the selected idle device, so poll rate comes from a new IDENT
        // query, not from a potentially stale cached control handle.
        manager.drop_device(raw_id(id));manager.retry_scan();selection(root,manager,key,revision,id,candidate)?;
        let(_,rate,_)=identity(manager,id)?;if rate!=Some(entry.pending.requested){return Err("新连接报告的回报率尚未达到目标，未续接；可明确重试回报率".into());}
        Ok(entry.pending.plan)
    })?;
    plan.settings.retain(|s|!matches!(s,Setting::ReportRate(_)));plan.source=source;
    if plan.settings.is_empty(){return self.with(|m|{selection(root,m,key,revision,id,candidate)?;let out=view(m.device_mut(raw_id(id)).ok_or("鼠标已断开")?.as_mut())?;finish(root,key)?;Ok(out)});}
    self.apply_at(id,&plan,runtime,root,Some((key,revision,candidate)))
 }
 pub fn reconnect_retry(&self,key:&str,revision:&str,id:&str,candidate:&str,confirmed:bool)->Result<String,String>{self.reconnect_retry_at(&root()?,key,revision,id,candidate,confirmed)}
 fn reconnect_retry_at(&self,root:&Path,key:&str,revision:&str,id:&str,candidate:&str,confirmed:bool)->Result<String,String>{
    if !confirmed{return Err("重试发送回报率前需要明确确认目标鼠标".into());}
    self.with(|manager|{let mut entry=selection(root,manager,key,revision,id,candidate)?;self.check_inactive(id)?;manager.drop_device(raw_id(id));manager.retry_scan();selection(root,manager,key,revision,id,candidate)?;
        let(_,rate,_)=identity(manager,id)?;if rate==Some(entry.pending.requested){return Ok("新连接已报告目标回报率，无需重复发送；请继续应用其余方案".into());}
        entry.pending.attempt=entry.pending.attempt.checked_add(1).ok_or("发送次数记录已满，请先另存并放弃此待续接方案")?;
        entry.pending.sent=false;entry.pending.error=Some("用户已明确确认重试，发送结果待核对".into());entry.persist()?;
        let outcome=manager.device_mut(raw_id(id)).ok_or("鼠标已断开")?.apply(&Setting::ReportRate(entry.pending.requested));manager.drop_device(raw_id(id));
        entry.pending.sent=outcome.is_ok();entry.pending.error=outcome.err().map(|e|e.to_string());entry.persist()?;
        Ok(if entry.pending.sent{"已按确认重新发送一次回报率；请等待重连并重新查看"}else{"发送结果仍不确定，记录已保留；未自动重复发送"}.into())
    })
 }
 pub fn reconnect_discard(&self,key:&str,revision:&str)->Result<(),String>{self.reconnect_discard_at(&root()?,key,revision)}
 fn reconnect_discard_at(&self,root:&Path,key:&str,revision:&str)->Result<(),String>{self.with(|_|find(root,key,revision)?.finish())}
}
