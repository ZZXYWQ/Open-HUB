//! Cross-layer regression: source conversion -> on-disk profile store -> real
//! driver over memory HID -> physical-source dispatch -> recorded macro output.
//! Only the input sink and HID transport are synthetic; no OS input is emitted.
use super::*;
use crate::{macro_runtime::Runtime, profiles::{Profile,ProfileStore}};
use gearwise_core::{transport::MockBackend, profile::MacroStep, model::{Modifiers,MouseButton}};

#[derive(Clone,Default)]
struct Output(Arc<Mutex<Vec<String>>>);
impl gearwise_input::InputSink for Output {
 fn key_down(&mut self,k:u8)->gearwise_core::Result<()>{self.0.lock().unwrap().push(format!("key_down:{k}"));Ok(())}
 fn key_up(&mut self,k:u8)->gearwise_core::Result<()>{self.0.lock().unwrap().push(format!("key_up:{k}"));Ok(())}
 fn mouse_down(&mut self,b:MouseButton)->gearwise_core::Result<()>{self.0.lock().unwrap().push(format!("mouse_down:{b:?}"));Ok(())}
 fn mouse_up(&mut self,b:MouseButton)->gearwise_core::Result<()>{self.0.lock().unwrap().push(format!("mouse_up:{b:?}"));Ok(())}
 fn scroll(&mut self,_:i32,_:i32)->gearwise_core::Result<()>{panic!("unexpected scroll")}
 fn media(&mut self,_:u16)->gearwise_core::Result<()>{panic!("unexpected media")}
}
fn run(kind:&str,expected:&[&str]) {
 let root=std::env::temp_dir().join(format!("import-execution-{}",crate::profiles::new_id()));
 std::fs::create_dir(&root).unwrap();
 let imported=match kind {
  "synapse"=>{
   let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/Naga-supported-synthetic.synapse4");
   let prepared=crate::synapse_profiles::prepare(&path).unwrap();
   assert!(prepared.conversion_error.is_none());
   let candidate=prepared.candidates.into_iter().next().unwrap();
   candidate.profile.unwrap_or_else(||panic!("{:?}",candidate.issues))
  },
  "icue"=>{
   let path=root.join("source.cueprofile");std::fs::write(&path,crate::icue_profiles::tests::fixture()).unwrap();
   crate::icue_profiles::prepare(&path).unwrap().candidates.remove(0).profile.unwrap()
  },
  "gg"=>{
   let path=root.join("source.db");let db=crate::gg_profiles::tests::fixture();
   db.execute("VACUUM INTO ?1",[path.to_str().unwrap()]).unwrap();
   crate::gg_profiles::prepare(&path).unwrap().candidates.remove(0).profile.unwrap()
  },
  _=>panic!("unknown source")
 };
 let mut store=ProfileStore::default();
 for(id,vendor,model)in[("legacy-logitech",0x046d,0xc539),("legacy-mchose",0x3837,0x4026)] {
  let mut profile=Profile::desktop();profile.id=id.into();profile.target_vendor=Some(vendor);profile.target_model=Some(model);store.profiles.push(profile);
 }
 let legacy=serde_json::to_value(&store.profiles).unwrap();
 crate::profile_transfer::merge(&mut store,vec![imported.clone()]).unwrap();
 let path=root.join("profiles.json");store.save(&path).unwrap();
 let loaded=ProfileStore::load(&path);let restored=loaded.get(&imported.id).unwrap();
 assert_eq!(restored.multi,imported.multi);assert_eq!(loaded.profiles.len(),4);
 assert_eq!(serde_json::to_value(&loaded.profiles[..3]).unwrap(),legacy);
 assert!(!restored.targets(0x046d,&[restored.target_model.unwrap()]));
 assert!(!restored.targets(0x3837,&[restored.target_model.unwrap()]));
 let mut plan=restored.multi.clone().unwrap();
 let body=plan.macros.iter().find(|m|m.steps.iter().any(|s|matches!(s,MacroStep::Delay(23|25)))).unwrap();
 let button=plan.settings.iter().find_map(|s|match s {Setting::Button{id,action:ButtonAction::Macro(m)}if *m==body.id=>Some(*id),_=>None}).unwrap();
 let(device,log)=simulated(plan.vendor,plan.product).unwrap();let id=device.descriptor().id.clone();
 let mut manager=DeviceManager::new(Box::new(MockBackend::new(vec![],|_|Err(gearwise_core::Error::Unsupported("no physical HID")))),vec![]);
 manager.add_virtual(vec![device]);let service=Service::default();*service.0.lock().unwrap()=Some(manager);
 let source=crate::host_macros::Source{vendor:plan.vendor,product:plan.product,path:format!("simulation:source-import-{kind}")};
 plan.source=Some(source.clone());let output=Output::default();let runtime=Runtime::simulation(Arc::new(Mutex::new(output.clone())));
 let view=service.apply_at(&id,&plan,&runtime,&root,None).unwrap();
 assert!(!view.hardware_tested);
 let mut expected_session=plan.clone();
 // iCUE has no polling field: import must not invent/write a source value.
 // Once applied, the desired-session view may include the target's known rate.
 if kind=="icue"{assert_eq!(restored.report_rate_hz,None);assert_eq!(view.state.report_rate,Some(1000));expected_session.settings.push(Setting::ReportRate(1000));}
 assert_eq!(view.session.as_ref().unwrap().plan,expected_session);
 let dpi=view.state.dpi.as_ref().unwrap();assert_eq!(dpi.active as usize,restored.active_level);
 assert_eq!(dpi.stages.iter().map(|d|(d.x,d.y)).collect::<Vec<_>>(),restored.levels.iter().map(|d|(d.x,d.y.unwrap())).collect::<Vec<_>>());
 if let Some(rate)=restored.report_rate_hz{assert_eq!(view.state.report_rate,Some(rate as u16));}
 assert!(!log.lock().unwrap().is_empty());assert!(output.0.lock().unwrap().is_empty());
 let vkey=if plan.vendor==0x1b1c{None}else{
  match view.state.buttons.get(&button).unwrap(){ButtonAction::Key{usage,mods:Modifiers(0)}if(0x68..=0x73).contains(usage)=>Some(u16::from(*usage)+0x14),other=>panic!("missing native trigger: {other:?}")}
 };
 let event=|down|if let Some(vkey)=vkey{crate::raw_input::Event::Key{source:source.clone(),vkey,down}}else{crate::raw_input::Event::Buttons{source:source.clone(),mask:if down{1u32<<(button-1)}else{0}}};
 runtime.simulate_event(event(true));
 let limit=std::time::Instant::now()+std::time::Duration::from_secs(2);
 while output.0.lock().unwrap().len()<expected.len()&&std::time::Instant::now()<limit{std::thread::sleep(std::time::Duration::from_millis(1));}
 runtime.simulate_event(event(false));
 assert_eq!(*output.0.lock().unwrap(),expected);
 service.stop_device_software(&id,&runtime).unwrap();assert_eq!(runtime.status().source_count,0);
 assert_eq!(service.read(&id,&runtime).unwrap().session.unwrap().plan.macros,plan.macros);
 assert_eq!(ProfileStore::load(&path).get(&imported.id).unwrap().multi,imported.multi);
 runtime.shutdown();
 assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));
 std::fs::remove_dir_all(root).unwrap();
}
#[test]fn naga_synapse_file_disk_reload_native_trigger_and_macro_output(){run("synapse",&["key_down:4","key_down:5","key_up:4","key_up:5"]);}
#[test]fn m65_icue_file_disk_reload_vendor_button_and_macro_output(){run("icue",&["key_down:4","key_down:5","key_up:4","key_up:5"]);}
#[test]fn rival_gg_database_disk_reload_native_trigger_and_mixed_macro_output(){run("gg",&["key_down:4","mouse_down:Right","key_up:4","mouse_up:Right"]);}
