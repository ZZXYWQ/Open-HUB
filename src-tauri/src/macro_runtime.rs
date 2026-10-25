//! Native macro runtime: only exact, currently present Raw Input sources may
//! arm SendInput playback. The same dispatch hub is tested with a recorder.
use std::{collections::{BTreeMap,HashMap,HashSet},sync::{Arc,Mutex}};
use crate::{host_macros::{Router,Source,Binding},raw_input::{self,Event,SourceInfo}};
use gearwise_input::SharedSink;
use serde::Serialize;
#[derive(Clone,Debug)]pub struct InputMap{pub keys:BTreeMap<u16,u16>,pub buttons:Vec<u16>,pub dpi_buttons:Vec<u16>}
#[derive(Clone,Default)]pub struct Hooks{pub dpi:Option<Arc<dyn Fn(u16,bool)->Result<(),String>+Send+Sync>>,pub cancel:Option<Arc<dyn Fn()+Send+Sync>>}
struct Config{source:Source,map:InputMap,last_mask:u32,hooks:Hooks,quiescing:bool}
#[derive(Default)]struct Physical{keys:HashSet<u16>,buttons:u32}
#[derive(Clone)]struct ReleaseWatch{keys:Vec<u16>,buttons:u32}
struct Hub{router:Router,configs:Mutex<HashMap<String,Config>>,physical:Mutex<HashMap<Source,Physical>>,pending:Mutex<HashMap<Source,ReleaseWatch>>,errors:Mutex<Vec<String>>}
impl Hub{
 fn new(sink:SharedSink)->Self{Self{router:Router::new(sink),configs:Mutex::new(HashMap::new()),physical:Mutex::new(HashMap::new()),pending:Mutex::new(HashMap::new()),errors:Mutex::new(vec![])}}
 fn configure(&self,source:Source,bindings:Vec<Binding>,map:InputMap,hooks:Hooks)->Result<(),String>{
  if self.pending.lock().map_err(|_|"恢复等待状态锁异常")?.keys().any(|s|s.path==source.path){return Err("此输入来源仍在等待原始映射恢复，暂不能重新运行宏".into());}
  if map.keys.keys().any(|k|!(0x7c..=0x87).contains(k))||map.buttons.iter().any(|b|!(1..=25).contains(b)){return Err("软件宏触发映射无效".into());}
  let ids=bindings.iter().map(|b|b.button).collect::<std::collections::HashSet<_>>();if map.keys.values().chain(map.buttons.iter()).any(|b|!ids.contains(b)&&!map.dpi_buttons.contains(b)){return Err("触发映射缺少对应宏".into());}
  let mask=map.buttons.iter().fold(0u32,|mask,b|mask|1u32<<(*b-1));let physical=self.physical.lock().map_err(|_|"物理按键状态锁异常")?;if physical.get(&source).is_some_and(|p|p.buttons&mask!=0||map.keys.keys().any(|k|p.keys.contains(k)))||map.keys.keys().any(|k|native_key_down(*k)){return Err("触发键仍按住，请松开后重新应用方案".into());}drop(physical);
  let mut configs=self.configs.lock().map_err(|_|"输入配置锁异常")?;self.router.configure(source.clone(),bindings)?;let previous=configs.insert(source.path.clone(),Config{source,map,last_mask:0,hooks,quiescing:false});drop(configs);if let Some(cancel)=previous.and_then(|c|c.hooks.cancel){cancel();}Ok(())
 }
 // Cancel owned synthetic output now, but keep observing the physical source
 // until restoring its native F13 mapping can no longer swallow a key-up.
 fn quiesce(&self,source:&Source,expected_keys:&[u16])->Result<bool,String>{
  let (watch,cancel)={let mut configs=self.configs.lock().map_err(|_|"输入配置锁异常")?;match configs.get_mut(&source.path){Some(config)=>{if config.source!=*source{return Err("恢复记录与正在监听的输入来源不一致".into());}config.quiescing=true;(ReleaseWatch{keys:config.map.keys.keys().copied().collect(),buttons:config.map.buttons.iter().fold(0u32,|mask,b|mask|1u32<<(*b-1))},config.hooks.cancel.take())},None=>(ReleaseWatch{keys:expected_keys.to_vec(),buttons:if source.vendor==0x1b1c{u32::MAX}else{0}},None)}};
  let watch=self.pending.lock().map_err(|_|"恢复等待状态锁异常")?.entry(source.clone()).or_insert(watch).clone();
  self.router.remove(&source.path);if let Some(cancel)=cancel{cancel();}
  let physical=self.physical.lock().map_err(|_|"物理按键状态锁异常")?;let held=physical.get(source).is_some_and(|p|p.buttons&watch.buttons!=0||watch.keys.iter().any(|k|p.keys.contains(k)));Ok(!held&&!watch.keys.iter().any(|k|native_key_down(*k)))
 }
 fn is_quiescing(&self,source:&Source)->bool{self.pending.lock().map(|p|p.contains_key(source)).unwrap_or(false)}
 fn stop_output(&self){let configs=self.configs.lock().map(|mut c|c.drain().map(|(_,c)|c).collect::<Vec<_>>()).unwrap_or_default();for config in configs{if let Some(cancel)=config.hooks.cancel{cancel();}}self.router.stop_all();if let Ok(mut pending)=self.pending.lock(){pending.clear();}}
 fn remove(&self,path:&str){if let Ok(mut pending)=self.pending.lock(){pending.retain(|source,_|source.path!=path);}let config=self.configs.lock().ok().and_then(|mut c|c.remove(path));self.router.remove(path);if let Some(config)=config{if let Some(cancel)=config.hooks.cancel{cancel();}}}
 fn event(&self,event:Event){
  if let Event::Removed{path}=event{if let Ok(mut physical)=self.physical.lock(){physical.retain(|source,_|source.path!=path);}self.remove(&path);return;}
  // Observe supported sources even between native remapping and route arming.
  // Always release this lock before the config/Router locks.
  if let Ok(mut physical)=self.physical.lock(){match &event{
   Event::Key{source,vkey,down} if crate::multibrand::supported(source.vendor,source.product)&&(0x7c..=0x87).contains(vkey)=>{let state=physical.entry(source.clone()).or_default();if *down{state.keys.insert(*vkey);}else{state.keys.remove(vkey);}},
   Event::Buttons{source,mask} if crate::multibrand::supported(source.vendor,source.product)=>physical.entry(source.clone()).or_default().buttons=*mask,
   _=>{}
  }}
  let Ok(mut configs)=self.configs.lock()else{return};let mut dpi_events=Vec::new();
  match event{
   Event::Key{source,vkey,down}=>{let Some(config)=configs.get(&source.path).filter(|c|c.source==source&&!c.quiescing)else{return};if let Some(button)=config.map.keys.get(&vkey){self.router.input(&source,*button,down);}},
   Event::Buttons{source,mask}=>{let Some(config)=configs.get_mut(&source.path).filter(|c|c.source==source&&!c.quiescing)else{return};let changed=config.last_mask^mask;config.last_mask=mask;for button in &config.map.buttons{let bit=1u32<<(*button-1);if changed&bit!=0{if config.map.dpi_buttons.contains(button){if let Some(callback)=&config.hooks.dpi{dpi_events.push((callback.clone(),*button,mask&bit!=0));}}else{self.router.input(&source,*button,mask&bit!=0);}}}},
   _=>{},
  }
  drop(configs); // Never acquire a device manager/HID lock while holding routing state.
  for (callback,button,down) in dpi_events{if let Err(e)=callback(button,down){if let Ok(mut errors)=self.errors.lock(){if errors.len()<128{errors.push(e);}}}}
 }

}
fn native_key_down(vkey:u16)->bool{
 #[cfg(all(windows,not(test)))]{unsafe{windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow().is_null()||windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(i32::from(vkey)) as u16&0x8000!=0}}
 #[cfg(any(not(windows),test))]{let _=vkey;false}
}
fn verify_required_serial(source:&Source,expected:Option<&str>,actual:Option<&str>)->Result<(),String>{
 let expected=expected.map(str::trim).filter(|s|!crate::profile_routing::serial_key(source.vendor,source.product,Some(s)).is_empty()).ok_or("软件宏与软件按键映射需要可核验的鼠标序列号；当前可使用已实现的板载功能")?;
 if actual.map(str::trim)!=Some(expected){return Err("鼠标输入接口序列号与目标鼠标不一致或不可读，未启用软件宏".into());}Ok(())
}
fn verify_input_source(sources:Vec<SourceInfo>,source:&Source,serial:Option<&str>)->Result<SourceInfo,String>{
 let candidate=sources.iter().cloned().find(|v|v.source==*source).ok_or("未找到配置指定的输入来源；请重新选择此鼠标的输入设备")?;
 let expected_kind=if source.vendor==0x1b1c{"Vendor HID"}else{"Keyboard report"};
 if candidate.kind!=expected_kind{return Err(format!("此方案需要选择 {expected_kind} 输入接口"));}
 verify_required_serial(source,serial,candidate.serial.as_deref())?;
 if sources.iter().filter(|s|s.source.vendor==source.vendor&&s.source.product==source.product&&s.kind==expected_kind&&s.serial.as_deref().map(str::trim)==serial.map(str::trim)).count()!=1{return Err("鼠标序列号对应多个输入接口，无法确定独立鼠标，未启用软件宏".into());}
 Ok(candidate)
}
#[derive(Clone,Default)]pub struct Runtime(Arc<Mutex<Option<Native>>>);
struct Native{hub:Arc<Hub>,#[cfg(windows)]_watcher:Option<raw_input::Watcher>,#[cfg(test)]simulation:bool}
#[derive(Serialize)]pub struct Status{pub source_count:usize,pub errors:Vec<String>}
impl Runtime{
 pub fn verify(source:&Source,serial:Option<&str>)->Result<SourceInfo,String>{verify_input_source(raw_input::sources()?,source,serial)}
 pub fn prepare(&self,source:&Source,serial:Option<&str>)->Result<(),String>{
  #[cfg(test)]if source.path.starts_with("simulation:")&&crate::multibrand::supported(source.vendor,source.product)&&self.0.lock().map_err(|_|"软件宏运行时锁异常")?.as_ref().is_some_and(|n|n.simulation){return Ok(());}
  Self::verify(source,serial)?;let mut native=self.0.lock().map_err(|_|"软件宏运行时锁异常")?;if native.is_none(){
   #[cfg(windows)]{let sink:SharedSink=Arc::new(Mutex::new(gearwise_input::create_sink().map_err(|e|e.to_string())?));let hub=Arc::new(Hub::new(sink));let callback_hub=hub.clone();let watcher=raw_input::Watcher::start(Arc::new(move|e|callback_hub.event(e)))?;*native=Some(Native{hub,_watcher:Some(watcher),#[cfg(test)]simulation:false});}
   #[cfg(not(windows))]return Err("软件宏输入采集目前需要 Windows".into());
  }Ok(())}
 pub fn arm(&self,source:Source,bindings:Vec<Binding>,map:InputMap,hooks:Hooks)->Result<(),String>{let native=self.0.lock().map_err(|_|"软件宏运行时锁异常")?;native.as_ref().ok_or("输入采集尚未初始化")?.hub.configure(source,bindings,map,hooks)}
 pub fn remove(&self,path:&str){if let Ok(native)=self.0.lock(){if let Some(native)=native.as_ref(){native.hub.remove(path);}}}
 pub fn quiesce(&self,source:&Source,expected_keys:&[u16])->Result<bool,String>{let native=self.0.lock().map_err(|_|"软件宏运行时锁异常")?;let ready=match native.as_ref(){Some(n)=>n.hub.quiesce(source,expected_keys)?,None=>true};Ok(ready&&!expected_keys.iter().any(|k|native_key_down(*k)))}
 pub fn is_quiescing(&self,source:&Source)->bool{self.0.lock().map(|n|n.as_ref().is_some_and(|n|n.hub.is_quiescing(source))).unwrap_or(false)}
 pub(crate) fn is_armed(&self,source:&Source)->bool{self.0.lock().map(|n|n.as_ref().is_some_and(|n|n.hub.configs.lock().map(|c|c.get(&source.path).is_some_and(|c|c.source==*source&&!c.quiescing)).unwrap_or(false))).unwrap_or(false)}
 pub(crate) fn manual_recovery_ready(&self,source:&Source,keys:&[u16])->Result<bool,String>{
  let native=self.0.lock().map_err(|_|"软件宏运行时锁异常")?;
  let hub=&native.as_ref().ok_or("请先准备恢复并选择当前鼠标的输入来源")?.hub;
  if hub.configs.lock().map_err(|_|"输入配置锁异常")?.contains_key(&source.path){return Err("此输入来源仍有运行或停止中的方案，请先停止并等待恢复完成".into());}
  if source.vendor==0x1b1c&&!hub.physical.lock().map_err(|_|"物理按键状态锁异常")?.contains_key(source){return Err("尚未收到此鼠标的按钮状态；请移动鼠标并松开所有按钮后重试".into());}
  hub.quiesce(source,keys)
 }
 pub fn has_quiescing(&self)->bool{self.0.lock().map(|n|n.as_ref().is_some_and(|n|n.hub.pending.lock().map(|p|!p.is_empty()).unwrap_or(true))).unwrap_or(true)}
 pub fn status(&self)->Status{self.0.lock().ok().and_then(|n|n.as_ref().map(|n|Status{source_count:n.hub.configs.lock().map(|c|c.values().filter(|c|!c.quiescing).count()).unwrap_or(0),errors:{let mut errors=n.hub.router.errors();if let Ok(extra)=n.hub.errors.lock(){errors.extend(extra.clone());}errors}})).unwrap_or(Status{source_count:0,errors:vec![]})}
 pub(crate) fn notice(&self,message:String){if let Ok(native)=self.0.lock(){if let Some(native)=native.as_ref(){if let Ok(mut errors)=native.hub.errors.lock(){if errors.len()<128{errors.push(message);}}}}}
 pub fn shutdown(&self){let native=self.0.lock().ok().and_then(|mut state|state.take());if let Some(native)=native{native.hub.stop_output();drop(native);}}
 #[cfg(test)]pub(crate) fn simulation(sink:SharedSink)->Self{Self(Arc::new(Mutex::new(Some(Native{hub:Arc::new(Hub::new(sink)),#[cfg(windows)]_watcher:None,simulation:true}))))}
 #[cfg(test)]pub(crate) fn simulate_event(&self,event:Event){let native=self.0.lock().unwrap();if let Some(native)=native.as_ref(){native.hub.event(event);}}
}
#[cfg(test)]mod tests{
 use super::*;use gearwise_core::{model::MouseButton,profile::{Macro,MacroMode,MacroStep}};use gearwise_input::InputSink;use std::time::{Duration,Instant};
 #[derive(Clone,Default)]struct Rec(Arc<Mutex<Vec<String>>>);impl InputSink for Rec{fn key_down(&mut self,k:u8)->gearwise_core::Result<()>{self.0.lock().unwrap().push(format!("d{k}"));Ok(())}fn key_up(&mut self,k:u8)->gearwise_core::Result<()>{self.0.lock().unwrap().push(format!("u{k}"));Ok(())}fn mouse_down(&mut self,_:MouseButton)->gearwise_core::Result<()>{Ok(())}fn mouse_up(&mut self,_:MouseButton)->gearwise_core::Result<()>{Ok(())}fn scroll(&mut self,_:i32,_:i32)->gearwise_core::Result<()>{Ok(())}fn media(&mut self,_:u16)->gearwise_core::Result<()>{Ok(())}}
 fn source()->Source{Source{vendor:0x1038,product:0x1720,path:"simulation:steel".into()}}
 fn binding()->Binding{Binding{button:6,action:Macro{id:"m".into(),name:"a".into(),mode:MacroMode::Once,steps:vec![MacroStep::KeyDown(4),MacroStep::KeyUp(4)]}}}
 fn wait(rec:&Rec,n:usize){let end=Instant::now()+Duration::from_secs(1);while rec.0.lock().unwrap().len()<n&&Instant::now()<end{std::thread::sleep(Duration::from_millis(1));}assert_eq!(rec.0.lock().unwrap().len(),n);}
 #[test]fn raw_keyboard_source_runs_exact_button_macro_and_ignores_repeat(){let rec=Rec::default();let hub=Hub::new(Arc::new(Mutex::new(rec.clone())));hub.configure(source(),vec![binding()],InputMap{keys:BTreeMap::from([(0x7c,6)]),buttons:vec![],dpi_buttons:vec![]},Hooks::default()).unwrap();hub.event(Event::Key{source:source(),vkey:0x7c,down:true});wait(&rec,2);hub.event(Event::Key{source:source(),vkey:0x7c,down:true});std::thread::sleep(Duration::from_millis(5));assert_eq!(rec.0.lock().unwrap().len(),2);hub.event(Event::Key{source:source(),vkey:0x7c,down:false});hub.event(Event::Key{source:source(),vkey:0x7c,down:true});wait(&rec,4);}
 #[test]fn second_device_and_unbound_key_never_trigger(){let rec=Rec::default();let hub=Hub::new(Arc::new(Mutex::new(rec.clone())));hub.configure(source(),vec![binding()],InputMap{keys:BTreeMap::from([(0x7c,6)]),buttons:vec![],dpi_buttons:vec![]},Hooks::default()).unwrap();let mut other=source();other.path="simulation:other".into();hub.event(Event::Key{source:other,vkey:0x7c,down:true});hub.event(Event::Key{source:source(),vkey:0x7d,down:true});assert!(rec.0.lock().unwrap().is_empty());}
 #[test]fn corsair_physical_bit_edges_run_once_per_press(){let rec=Rec::default();let hub=Hub::new(Arc::new(Mutex::new(rec.clone())));let source=Source{vendor:0x1b1c,product:0x1b2e,path:"simulation:corsair".into()};hub.configure(source.clone(),vec![binding()],InputMap{keys:BTreeMap::new(),buttons:vec![6],dpi_buttons:vec![]},Hooks::default()).unwrap();hub.event(Event::Buttons{source:source.clone(),mask:32});wait(&rec,2);hub.event(Event::Buttons{source:source.clone(),mask:32});std::thread::sleep(Duration::from_millis(5));assert_eq!(rec.0.lock().unwrap().len(),2);hub.event(Event::Buttons{source:source.clone(),mask:0});hub.event(Event::Buttons{source,mask:32});wait(&rec,4);}
 #[test]fn removal_disarms_source(){let rec=Rec::default();let hub=Hub::new(Arc::new(Mutex::new(rec.clone())));hub.configure(source(),vec![binding()],InputMap{keys:BTreeMap::from([(0x7c,6)]),buttons:vec![],dpi_buttons:vec![]},Hooks::default()).unwrap();hub.event(Event::Removed{path:source().path});hub.event(Event::Key{source:source(),vkey:0x7c,down:true});assert!(rec.0.lock().unwrap().is_empty());}
 #[test]fn quiesce_releases_owned_output_but_waits_for_the_same_physical_trigger(){let rec=Rec::default();let hub=Hub::new(Arc::new(Mutex::new(rec.clone())));let mut b=binding();b.action.mode=MacroMode::WhileHeld;b.action.steps=vec![MacroStep::KeyDown(4),MacroStep::Delay(5000),MacroStep::KeyUp(4)];hub.configure(source(),vec![b],InputMap{keys:BTreeMap::from([(0x7c,6)]),buttons:vec![],dpi_buttons:vec![]},Hooks::default()).unwrap();hub.event(Event::Key{source:source(),vkey:0x7c,down:true});wait(&rec,1);assert!(!hub.quiesce(&source(),&[0x7c]).unwrap());wait(&rec,2);let mut other=source();other.path.push_str("-other");hub.event(Event::Key{source:other,vkey:0x7c,down:false});assert!(!hub.quiesce(&source(),&[0x7c]).unwrap());hub.event(Event::Key{source:source(),vkey:0x7c,down:false});assert!(hub.quiesce(&source(),&[0x7c]).unwrap());hub.event(Event::Key{source:source(),vkey:0x7c,down:true});assert!(!hub.quiesce(&source(),&[0x7c]).unwrap());assert_eq!(*rec.0.lock().unwrap(),vec!["d4","u4"]);}
 #[test]fn pre_arm_keyboard_state_is_not_lost_when_native_routing_is_registered_later(){let rec=Rec::default();let hub=Hub::new(Arc::new(Mutex::new(rec.clone())));hub.event(Event::Key{source:source(),vkey:0x7c,down:true});assert!(!hub.quiesce(&source(),&[0x7c]).unwrap());assert!(hub.is_quiescing(&source()));hub.event(Event::Key{source:source(),vkey:0x7c,down:false});assert!(hub.quiesce(&source(),&[0x7c]).unwrap());assert!(rec.0.lock().unwrap().is_empty());hub.event(Event::Removed{path:source().path});assert!(!hub.is_quiescing(&source()));}
 #[test]fn arming_held_input_and_rearming_pending_restore_cannot_start_an_unintended_macro(){let rec=Rec::default();let hub=Hub::new(Arc::new(Mutex::new(rec.clone())));let map=InputMap{keys:BTreeMap::from([(0x7c,6)]),buttons:vec![],dpi_buttons:vec![]};hub.event(Event::Key{source:source(),vkey:0x7c,down:true});assert!(hub.configure(source(),vec![binding()],map.clone(),Hooks::default()).is_err());assert!(!hub.quiesce(&source(),&[0x7c]).unwrap());hub.event(Event::Key{source:source(),vkey:0x7c,down:false});assert!(hub.configure(source(),vec![binding()],map.clone(),Hooks::default()).is_err());assert!(hub.quiesce(&source(),&[0x7c]).unwrap());hub.remove(&source().path);hub.configure(source(),vec![binding()],map,Hooks::default()).unwrap();assert!(rec.0.lock().unwrap().is_empty());hub.event(Event::Key{source:source(),vkey:0x7c,down:true});wait(&rec,2);}
 #[test]fn recovery_cannot_cancel_a_different_brand_with_a_reused_path(){let rec=Rec::default();let hub=Hub::new(Arc::new(Mutex::new(rec.clone())));hub.configure(source(),vec![binding()],InputMap{keys:BTreeMap::from([(0x7c,6)]),buttons:vec![],dpi_buttons:vec![]},Hooks::default()).unwrap();let mut other=source();other.vendor=0x1532;other.product=0x00b4;assert!(hub.quiesce(&other,&[0x7c]).is_err());hub.event(Event::Key{source:source(),vkey:0x7c,down:true});wait(&rec,2);}
 #[test]fn shutdown_is_shared_by_clones_and_callbacks_can_reenter_status_without_deadlock(){use std::sync::atomic::{AtomicUsize,Ordering};let runtime=Runtime::simulation(Arc::new(Mutex::new(Rec::default())));let observer=runtime.clone();let callback_runtime=runtime.clone();let cancelled=Arc::new(AtomicUsize::new(0));let c=cancelled.clone();runtime.arm(source(),vec![binding()],InputMap{keys:BTreeMap::from([(0x7c,6)]),buttons:vec![],dpi_buttons:vec![]},Hooks{dpi:None,cancel:Some(Arc::new(move||{assert_eq!(callback_runtime.status().source_count,0);c.fetch_add(1,Ordering::SeqCst);} ))}).unwrap();let(tx,rx)=std::sync::mpsc::channel();let thread=std::thread::spawn(move||{runtime.shutdown();tx.send(()).unwrap();});rx.recv_timeout(Duration::from_secs(1)).expect("shutdown must not hold the runtime mutex while cancelling callbacks/joining input capture");thread.join().unwrap();assert_eq!(observer.status().source_count,0);assert_eq!(cancelled.load(Ordering::SeqCst),1);}
}

#[cfg(test)]mod dpi_hook_tests{
 use super::*;use gearwise_core::model::MouseButton;use gearwise_input::InputSink;
 struct NoOutput;impl InputSink for NoOutput{fn key_down(&mut self,_:u8)->gearwise_core::Result<()>{panic!("DPI must not inject keyboard input")}fn key_up(&mut self,_:u8)->gearwise_core::Result<()>{panic!("DPI must not inject keyboard input")}fn mouse_down(&mut self,_:MouseButton)->gearwise_core::Result<()>{panic!("unexpected mouse input")}fn mouse_up(&mut self,_:MouseButton)->gearwise_core::Result<()>{panic!("unexpected mouse input")}fn scroll(&mut self,_:i32,_:i32)->gearwise_core::Result<()>{panic!("unexpected input")}fn media(&mut self,_:u16)->gearwise_core::Result<()>{panic!("unexpected input")}}
 fn source()->Source{Source{vendor:0x1b1c,product:0x1b2e,path:"simulation:m65".into()}}
 #[test]fn dpi_physical_edges_are_deduplicated_and_release_is_delivered(){let hub=Hub::new(Arc::new(Mutex::new(NoOutput)));let events=Arc::new(Mutex::new(Vec::new()));let output=events.clone();hub.configure(source(),vec![],InputMap{keys:BTreeMap::new(),buttons:vec![6,8],dpi_buttons:vec![6,8]},Hooks{dpi:Some(Arc::new(move|b,d|{output.lock().unwrap().push((b,d));Ok(())})),cancel:None}).unwrap();for mask in [128,128,160,32,0]{hub.event(Event::Buttons{source:source(),mask});}assert_eq!(*events.lock().unwrap(),vec![(8,true),(6,true),(8,false),(6,false)]);}
 #[test]fn replacing_or_removing_source_cancels_owned_callbacks(){use std::sync::atomic::{AtomicUsize,Ordering};let hub=Hub::new(Arc::new(Mutex::new(NoOutput)));let cancelled=Arc::new(AtomicUsize::new(0));for _ in 0..2{let c=cancelled.clone();hub.configure(source(),vec![],InputMap{keys:BTreeMap::new(),buttons:vec![],dpi_buttons:vec![]},Hooks{dpi:None,cancel:Some(Arc::new(move||{c.fetch_add(1,Ordering::SeqCst);} ))}).unwrap();}assert_eq!(cancelled.load(Ordering::SeqCst),1);hub.remove(&source().path);hub.remove(&source().path);assert_eq!(cancelled.load(Ordering::SeqCst),2);}
 #[test]fn held_vendor_button_defers_mode_restore_and_disables_dpi_callbacks(){use std::sync::atomic::{AtomicUsize,Ordering};let hub=Hub::new(Arc::new(Mutex::new(NoOutput)));let calls=Arc::new(AtomicUsize::new(0));let c=calls.clone();hub.configure(source(),vec![],InputMap{keys:BTreeMap::new(),buttons:vec![8],dpi_buttons:vec![8]},Hooks{dpi:Some(Arc::new(move|_,_|{c.fetch_add(1,Ordering::SeqCst);Ok(())})),cancel:None}).unwrap();hub.event(Event::Buttons{source:source(),mask:128});assert_eq!(calls.load(Ordering::SeqCst),1);assert!(!hub.quiesce(&source(),&[]).unwrap());let mut other=source();other.path.push_str("-other");hub.event(Event::Buttons{source:other,mask:0});assert!(!hub.quiesce(&source(),&[]).unwrap());hub.event(Event::Buttons{source:source(),mask:0});assert!(hub.quiesce(&source(),&[]).unwrap());assert_eq!(calls.load(Ordering::SeqCst),1);}
}

#[cfg(test)]mod pulsar_identity_tests{
 use super::*;
 #[test]fn pulsar_host_macros_require_matching_readable_serials(){let s=Source{vendor:0x3554,product:0xf507,path:"keyboard-a".into()};assert!(verify_required_serial(&s,Some("unit-a"),Some("unit-a")).is_ok());for(expected,actual)in[(None,None),(Some("unit-a"),None),(None,Some("unit-a")),(Some("unit-a"),Some("unit-b")),(Some(""),Some(""))]{assert!(verify_required_serial(&s,expected,actual).is_err());}}
}

#[cfg(test)]mod source_inventory_tests{
 use super::*;
 fn info(path:&str,serial:&str,kind:&str)->SourceInfo{SourceInfo{source:Source{vendor:0x3554,product:0xf507,path:path.into()},kind:kind.into(),serial:Some(serial.into())}}
 #[test]fn serial_inventory_distinguishes_two_mice_and_refuses_ambiguous_serial_or_wrong_collection(){let a=info("keyboard-a","a","Keyboard report");let b=info("keyboard-b","b","Keyboard report");assert!(verify_input_source(vec![a.clone(),b.clone()],&a.source,Some("a")).is_ok());assert!(verify_input_source(vec![a.clone(),b.clone()],&b.source,Some("a")).is_err());let duplicate=info("other-keyboard-a","a","Keyboard report");assert!(verify_input_source(vec![a.clone(),duplicate],&a.source,Some("a")).is_err());let vendor=info("vendor-a","a","Vendor HID");assert!(verify_input_source(vec![a.clone(),vendor.clone()],&a.source,Some("a")).is_ok());assert!(verify_input_source(vec![vendor.clone()],&vendor.source,Some("a")).is_err());}
}

#[cfg(test)]mod basilisk_source_tests{
 use super::*;
 #[test]fn basilisk_keyboard_source_must_be_unique_and_serial_verified(){for product in [0x00aa,0x00ab]{let s=Source{vendor:0x1532,product,path:"keyboard-a".into()};let a=SourceInfo{source:s.clone(),serial:Some("a".into()),kind:"Keyboard report".into()};assert!(verify_input_source(vec![a.clone()],&s,Some("a")).is_ok());assert!(verify_input_source(vec![a.clone()],&s,None).is_err());let mut other=a.clone();other.serial=None;assert!(verify_input_source(vec![other],&s,Some("a")).is_err());let mut duplicate=a.clone();duplicate.source.path="duplicate".into();assert!(verify_input_source(vec![a.clone(),duplicate],&s,Some("a")).is_err());let mut b=a.clone();b.serial=Some("b".into());assert!(verify_input_source(vec![b],&s,Some("a")).is_err());}}
}

#[cfg(test)]mod all_brand_source_identity_tests{
 use super::*;
 #[test]fn corsair_steelseries_and_naga_software_routes_require_one_verified_matching_unit(){
  for(vendor,product,kind)in[(0x1b1c,0x1b2e,"Vendor HID"),(0x1038,0x1720,"Keyboard report"),(0x1532,0x00b4,"Keyboard report")]{
   let source=Source{vendor,product,path:"input-A".into()};let a=SourceInfo{source:source.clone(),kind:kind.into(),serial:Some("unit-A".into())};
   assert!(verify_input_source(vec![a.clone()],&source,Some("unit-A")).is_ok());
   for serial in[None,Some(""),Some("000000"),Some("unit-B")]{assert!(verify_input_source(vec![a.clone()],&source,serial).is_err());}
   let mut missing=a.clone();missing.serial=None;assert!(verify_input_source(vec![missing],&source,Some("unit-A")).is_err());
   let mut duplicate=a.clone();duplicate.source.path="input-B".into();assert!(verify_input_source(vec![a.clone(),duplicate],&source,Some("unit-A")).is_err());
  }
 }
}
