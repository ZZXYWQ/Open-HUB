//! Real runtime/identity/router with recording output, never native HID or SendInput.
use super::*;
use crate::{input_identity::{Fixture,Target},raw_input::SourceInfo};
use gearwise_core::{model::MouseButton,profile::{Macro,MacroMode,MacroStep}};
use gearwise_input::InputSink;
use std::{sync::Barrier,time::{Duration,Instant}};

#[derive(Clone,Default)]struct Rec(Arc<Mutex<Vec<String>>>);
impl InputSink for Rec {
 fn key_down(&mut self,k:u8)->gearwise_core::Result<()>{self.0.lock().unwrap().push(format!("d{k}"));Ok(())}
 fn key_up(&mut self,k:u8)->gearwise_core::Result<()>{self.0.lock().unwrap().push(format!("u{k}"));Ok(())}
 fn mouse_down(&mut self,_:MouseButton)->gearwise_core::Result<()>{panic!("unexpected output")}
 fn mouse_up(&mut self,_:MouseButton)->gearwise_core::Result<()>{panic!("unexpected output")}
 fn scroll(&mut self,_:i32,_:i32)->gearwise_core::Result<()>{panic!("unexpected output")}
 fn media(&mut self,_:u16)->gearwise_core::Result<()>{panic!("unexpected output")}
}
fn source(unit:&str)->Source { Source {vendor:0x1532,product:0x00b6,path:format!("simulation:keyboard:{unit}")} }
fn target(unit:&str)->Target { Target {id:format!("razer:{unit}"),vendor:0x1532,product:0x00b6,serial:Some(format!("RAZER-UNIT-{unit}")),control_path:Some(format!("simulation:control:{unit}")),connection_key:Some(format!("razer-open:{unit}")),wired:true} }
fn fixture()->Fixture {
 let mut sources=Vec::new();let mut containers=HashMap::new();
 for (unit,container) in [("A",[1;16]),("B",[2;16])] {
  let t=target(unit);let s=source(unit);
  sources.push(SourceInfo {source:s.clone(),kind:"Keyboard report".into(),serial:t.serial.clone()});
  containers.insert(s.path,container);containers.insert(t.control_path.unwrap(),container);
 }
 Fixture {sources,containers}
}
fn setup()->(Runtime,Rec) { let rec=Rec::default();let rt=Runtime::simulation(Arc::new(Mutex::new(rec.clone())));rt.identity_fixture(fixture());(rt,rec) }
fn binding(key:u8,mode:MacroMode)->Binding {Binding {button:4,action:Macro {id:format!("key-{key}"),name:"Recorded".into(),mode,steps:vec![MacroStep::KeyDown(key),MacroStep::KeyUp(key)]}}}
fn map()->InputMap {InputMap {keys:BTreeMap::from([(0x7c,4)]),buttons:vec![],dpi_buttons:vec![]}}
fn prepare(rt:&Runtime,unit:&str)->PreparedProof {rt.prepare_target(&source(unit),&target(unit),&[target("A"),target("B")]).unwrap()}
fn arm(rt:&Runtime,p:&PreparedProof,unit:&str,key:u8) {rt.arm_prepared(p,source(unit),vec![binding(key,MacroMode::Once)],map(),Hooks::default()).unwrap();}
fn press(rt:&Runtime,unit:&str,down:bool) {rt.simulate_event(Event::Key {source:source(unit),vkey:0x7c,down});}
fn wait(rec:&Rec,n:usize) {let end=Instant::now()+Duration::from_secs(1);while rec.0.lock().unwrap().len()<n&&Instant::now()<end{std::thread::sleep(Duration::from_millis(1));}assert_eq!(rec.0.lock().unwrap().len(),n);}

#[test]
fn prepared_two_razer_units_are_independent_and_identical_path_rearrival_cannot_rearm_old_grant() {
 let(rt,rec)=setup();let a=prepare(&rt,"A");let b=prepare(&rt,"B");assert!(a.physical.is_some()&&b.physical.is_some());
 arm(&rt,&a,"A",4);arm(&rt,&b,"B",5);press(&rt,"A",true);wait(&rec,2);press(&rt,"B",true);wait(&rec,4);
 rt.simulate_event(Event::Removed {path:source("A").path});rt.simulate_event(Event::Arrived {source:source("A")});
 assert!(rt.assert_prepared(&a).is_err());assert!(rt.quiesce_prepared(&a,&source("A"),&[0x7c]).is_err());
 assert!(rt.arm_prepared(&a,source("A"),vec![binding(6,MacroMode::Once)],map(),Hooks::default()).is_err());
 assert!(rt.recheck_target(&b,&target("B"),&[target("A"),target("B")]).is_ok());
 press(&rt,"A",true);press(&rt,"B",false);press(&rt,"B",true);wait(&rec,6);
 assert_eq!(*rec.0.lock().unwrap(),vec!["d4","u4","d5","u5","d5","u5"]);
 let fresh=prepare(&rt,"A");assert!(rt.assert_prepared(&fresh).is_ok());assert!(rt.assert_prepared(&a).is_err());rt.shutdown();
}
#[test]
fn removal_during_fresh_identity_inventory_invalidates_the_already_captured_lease() {
 for event in [Event::Removed {path:source("A").path},Event::Arrived {source:source("A")},Event::InventoryUncertain,Event::Stopped] {
  let(rt,rec)=setup();let during=rt.clone();
  assert!(rt.prepare_target_after_capture(&source("A"),&target("A"),&[target("A"),target("B")],||during.simulate_event(event)).is_err());
  assert_eq!(rt.status().source_count,0);assert!(rec.0.lock().unwrap().is_empty());rt.shutdown();
 }
}
#[test]
fn old_prepare_cannot_stop_a_new_route_on_the_same_still_connected_source() {
 let(rt,rec)=setup();let old=prepare(&rt,"A");arm(&rt,&old,"A",4);
 let new=prepare(&rt,"A");let cancelled=Arc::new(std::sync::atomic::AtomicUsize::new(0));let count=cancelled.clone();
 rt.arm_prepared(&new,source("A"),vec![binding(5,MacroMode::Once)],map(),Hooks {dpi:None,cancel:Some(Arc::new(move||{count.fetch_add(1,std::sync::atomic::Ordering::SeqCst);} ))}).unwrap();
 assert!(rt.assert_prepared(&old).is_ok(),"the connection is live; the operation owner is stale");
 assert!(rt.quiesce_prepared(&old,&source("A"),&[0x7c]).is_err());assert!(rt.remove_prepared(&old).is_err());
 assert!(rt.unowned_recovery_ready(&source("A"),&[0x7c]).is_err());
 assert_eq!(cancelled.load(std::sync::atomic::Ordering::SeqCst),0);assert!(rt.is_armed(&source("A")));
 press(&rt,"A",true);wait(&rec,2);assert_eq!(*rec.0.lock().unwrap(),vec!["d5","u5"]);rt.shutdown();
}
#[test]
fn control_reopen_serial_swap_inventory_change_and_watcher_restart_reject_retained_prepare() {
 let(rt,rec)=setup();let proof=prepare(&rt,"A");
 for changed in [Target {connection_key:Some("new-open".into()),..target("A")},Target {serial:Some("REPLACEMENT".into()),..target("A")}] {
  assert!(rt.recheck_target(&proof,&changed,&[changed.clone(),target("B")]).is_err());
 }
 let mut missing=fixture();missing.sources.remove(0);rt.identity_fixture(missing);
 assert!(rt.recheck_target(&proof,&target("A"),&[target("A"),target("B")]).is_err());rt.identity_fixture(fixture());
 rt.shutdown();let replacement=Runtime::simulation(Arc::new(Mutex::new(rec)));let native=replacement.0.lock().unwrap().take();*rt.0.lock().unwrap()=native;rt.identity_fixture(fixture());
 assert!(rt.assert_prepared(&proof).is_err());let new=prepare(&rt,"A");assert!(rt.assert_prepared(&new).is_ok());rt.shutdown();
}
#[test]
fn final_arm_racing_disconnect_is_never_left_enabled_after_removal() {
 for _ in 0..32 {
  let(rt,rec)=setup();let proof=prepare(&rt,"A");let barrier=Arc::new(Barrier::new(2));let start=barrier.clone();let racing=rt.clone();
  let arm=std::thread::spawn(move||{start.wait();racing.arm_prepared(&proof,source("A"),vec![binding(4,MacroMode::Once)],map(),Hooks::default())});
  barrier.wait();rt.simulate_event(Event::Removed {path:source("A").path});let _=arm.join().unwrap();
  assert!(!rt.is_armed(&source("A")));press(&rt,"A",true);assert!(rec.0.lock().unwrap().is_empty());rt.shutdown();
 }
}
#[test]
fn permanent_listener_exit_cancels_toggle_output_and_cannot_mint_another_live_lease() {
 let(rt,rec)=setup();let proof=prepare(&rt,"A");let mut action=binding(4,MacroMode::Toggle);action.action.steps.insert(1,MacroStep::Delay(5000));
 rt.arm_prepared(&proof,source("A"),vec![action],map(),Hooks::default()).unwrap();press(&rt,"A",true);wait(&rec,1);
 rt.simulate_event(Event::Stopped);wait(&rec,2);assert_eq!(*rec.0.lock().unwrap(),vec!["d4","u4"]);
 assert!(rt.prepare_target(&source("A"),&target("A"),&[target("A"),target("B")]).is_err());
 assert!(rt.status().errors.iter().any(|e|e.contains("输入监听已停止")));assert_eq!(rt.status().source_count,0);rt.shutdown();
}
