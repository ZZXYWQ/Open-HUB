//! Explicit, reviewed recovery after restart. Never guesses a mouse from a brand.
use super::*;
use crate::{host_macros::Source, macro_recovery::{self,Record}, macro_runtime::Runtime};
use std::path::Path;

#[derive(Serialize)]
pub struct RecoveryItem {
    pub key:String,
    /// Full reviewed record and target identity, rechecked under the manager lock.
    pub revision:String,
    pub record:Record,
}
#[derive(Serialize)]
pub struct RecoveryList { pub records:Vec<RecoveryItem>,pub errors:Vec<String> }
fn review(record:&Record,descriptor:&DeviceDescriptor)->Result<serde_json::Value,String>{
    let record=serde_json::to_value(record).map_err(|e|e.to_string())?;
    Ok(serde_json::json!({"record":record,"target":{"id":descriptor.id,"serial":descriptor.serial,"vendor":descriptor.vendor_id,"product":descriptor.product_id}}))
}
fn revision(record:&Record,descriptor:&DeviceDescriptor)->Result<String,String>{
    serde_json::to_string(&review(record,descriptor)?).map_err(|e|e.to_string())
}
fn target(record:&Record,descriptor:&DeviceDescriptor,source:Option<&Source>)->Result<(),String>{
    if (record.source.vendor,record.source.product)!=(descriptor.vendor_id,descriptor.product_id)
        || !supported(descriptor.vendor_id,descriptor.product_id) {return Err("恢复记录与所选鼠标的具体型号不一致".into());}
    if record.serial.as_deref().filter(|s|!s.trim().is_empty()).is_some_and(|s|descriptor.serial.as_deref()!=Some(s)){
        return Err("恢复记录序列号与所选鼠标不一致，未写入".into());
    }
    if (crate::atk::is_pulsar(record.source.vendor,record.source.product)||gearwise_razer::basilisk::supported(record.source.vendor,record.source.product))&&!record.matches(descriptor){return Err("恢复记录的板载组或序列号不能核对；请切回原组并重新扫描，未写入".into());}
    if source.is_some_and(|s|(s.vendor,s.product)!=(descriptor.vendor_id,descriptor.product_id)||s.path.is_empty()||s.path.len()>4096){
        return Err("恢复输入来源与所选型号不一致".into());
    }
    Ok(())
}
impl Service {
    pub fn recovery_list(&self,id:&str)->Result<RecoveryList,String>{self.recovery_list_from(&macro_recovery::directory()?,id)}
    pub(super) fn recovery_list_from(&self,dir:&Path,id:&str)->Result<RecoveryList,String>{
        self.with(|manager|{
            let descriptor=manager.device_mut(raw_id(id)).ok_or("鼠标已断开")?.descriptor().clone();
            let(journals,mut errors)=macro_recovery::load(dir);let mut records=Vec::new();
            for journal in journals {let record=journal.record()?;
                if (record.source.vendor,record.source.product)!=(descriptor.vendor_id,descriptor.product_id){continue;}
                // Include a serial mismatch for review, but restoring it is rejected.
                let Some(key)=journal.path.file_name().and_then(|s|s.to_str()).map(str::to_owned) else {errors.push("恢复文件名无效".into());continue;};
                records.push(RecoveryItem{key,revision:revision(&record,&descriptor)?,record});
            }
            records.sort_by(|a,b|a.key.cmp(&b.key));Ok(RecoveryList{records,errors})
        })
    }
    pub fn observe_recovery(&self,id:&str,source:&Source,runtime:&Runtime)->Result<(),String>{
        self.prepare_input_target(id,source,runtime).map(|_|())
    }
    pub fn restore_recovery(&self,id:&str,key:&str,expected_revision:&str,source:&Source,confirmed:bool,runtime:&Runtime)->Result<(),String>{
        let(target,peers)=self.with(|m|Self::input_context(m,id))?;runtime.verify_target(source,&target,&peers)?;
        self.restore_recovery_from(&macro_recovery::directory()?,id,key,expected_revision,source,confirmed,runtime)
    }
    pub(super) fn restore_recovery_from(&self,dir:&Path,id:&str,key:&str,expected_revision:&str,source:&Source,confirmed:bool,runtime:&Runtime)->Result<(),String>{
        if !confirmed{return Err("请核对原始动作、目标鼠标与覆盖范围后确认恢复".into());}
        if key.len()>256||expected_revision.len()>128*1024{return Err("恢复请求大小无效".into());}
        self.with(|manager|{
            // Select only an enumerated journal; the request can never supply a path.
            let(journals,_)=macro_recovery::load(dir);
            let journal=journals.into_iter().find(|j|j.path.file_name().and_then(|v|v.to_str())==Some(key)).ok_or("恢复记录已完成或不存在，请刷新")?;
            let record=journal.record()?;
            let descriptor=manager.device_mut(raw_id(id)).ok_or("鼠标已断开")?.descriptor().clone();
            let reviewed:serde_json::Value=serde_json::from_str(expected_revision).map_err(|_|"恢复记录已变化或无效，请刷新并重新核对")?;
            if review(&record,&descriptor)?!=reviewed{return Err("恢复记录已变化或目标鼠标已更换，请刷新并重新核对".into());}
            target(&record,&descriptor,Some(source))?;
            let(current,peers)=Self::input_context(manager,id)?;runtime.verify_target(source,&current,&peers)?;
            {
                let active=self.1.lock().map_err(|_|"宏恢复记录锁异常")?;
                for (active_id,cleanup) in active.iter(){
                    let same=cleanup.journal.as_ref().is_some_and(|j|j.path==journal.path);
                    if same&&runtime.is_quiescing(&cleanup.source){return Err("此记录仍由后台等待物理释放，请先松开原鼠标按钮并等待恢复".into());}
                    if (active_id==raw_id(id)||cleanup.source==*source||same)&&(!same||!cleanup.cancelled.load(Ordering::Acquire)){
                        return Err("此鼠标已有其他运行或待恢复方案，请先停止该方案".into());
                    }
                }
            }
            let keys=record.expected.values().filter_map(|a|match a{ButtonAction::Key{usage:0x68..=0x73,..}=>if let ButtonAction::Key{usage,..}=a{Some(u16::from(*usage)+0x14)}else{None},_=>None}).collect::<Vec<_>>();
            if !runtime.manual_recovery_ready(source,&keys)?{return Err("触发键或鼠标按钮仍按住，请全部松开后重试".into());}
            let device=manager.device_mut(raw_id(id)).ok_or("鼠标已断开")?;
            // User explicitly reviewed these originals. Cached state is never
            // presented or used as proof of routing/DPI ownership here.
            if record.software_input {device.restore_hardware_input().map_err(|e|format!("硬件模式恢复失败，记录已保留：{e}"))?;}
            if let Some(shift)=&record.shift {device.apply(&Setting::Dpi(shift.original.clone())).map_err(|e|format!("原始 DPI 恢复失败，记录已保留：{e}"))?;}
            if record.version==2{crate::steelseries::restore_table(device.as_mut(),&record.settings)?;}else{
                for setting in &record.settings {device.apply(setting).map_err(|e|format!("原始按键恢复失败，记录已保留：{e}"))?;}
            }
            journal.finish()?;
            self.1.lock().map_err(|_|"宏恢复记录锁异常")?.retain(|_,c|!c.journal.as_ref().is_some_and(|j|j.path==journal.path));
            runtime.remove(&source.path);
            Ok(())
        })
    }
}

#[cfg(test)]mod tests {
    use super::*;
    use crate::macro_recovery::Journal;
    use gearwise_core::{Device,transport::MockBackend,model::{Modifiers,DpiState,DpiStage}};
    struct NoOutput;
    impl gearwise_input::InputSink for NoOutput {
        fn key_down(&mut self,_:u8)->gearwise_core::Result<()>{panic!("recovery must not inject")}
        fn key_up(&mut self,_:u8)->gearwise_core::Result<()>{panic!("recovery must not inject")}
        fn mouse_down(&mut self,_:gearwise_core::model::MouseButton)->gearwise_core::Result<()>{panic!("recovery must not inject")}
        fn mouse_up(&mut self,_:gearwise_core::model::MouseButton)->gearwise_core::Result<()>{panic!("recovery must not inject")}
        fn scroll(&mut self,_:i32,_:i32)->gearwise_core::Result<()>{panic!("recovery must not inject")}
        fn media(&mut self,_:u16)->gearwise_core::Result<()>{panic!("recovery must not inject")}
    }
    struct Fault {inner:Box<dyn Device>,descriptor:DeviceDescriptor,fail:Arc<AtomicBool>}
    impl Device for Fault {
        fn descriptor(&self)->&DeviceDescriptor{&self.descriptor}
        fn capabilities(&self)->&Capabilities{self.inner.capabilities()}
        fn read_state(&mut self)->gearwise_core::Result<DeviceState>{self.inner.read_state()}
        fn apply(&mut self,s:&Setting)->gearwise_core::Result<()>{if self.fail.load(Ordering::Acquire){Err(gearwise_core::Error::Unsupported("simulated write failure"))}else{self.inner.apply(s)}}
        fn stop_software_actions(&mut self)->gearwise_core::Result<()>{self.inner.stop_software_actions()}
        fn restore_hardware_input(&mut self)->gearwise_core::Result<()>{self.inner.restore_hardware_input()}
    }
    fn setup(vendor:u16,product:u16)->(Service,std::path::PathBuf,Runtime,Source,MockLog,Arc<AtomicBool>){
        let(mut device,log)=simulated(vendor,product).unwrap();
        let button=if vendor==0x1532{64}else{4};
        if vendor!=0x1b1c{device.apply(&Setting::Button{id:button,action:ButtonAction::Key{usage:0x68,mods:Modifiers(0)}}).unwrap();}
        // A fresh M65 driver has no remembered input ownership after restart.
        // Manual recovery must nevertheless emit the explicit hardware reset.
        let source=Source{vendor,product,path:"simulation:manual:input".into()};
        let dir=std::env::temp_dir().join(format!("manual-recovery-{}",crate::profiles::new_id()));
        let dpi=DpiState{stages:vec![DpiStage::uniform(1600)],active:0};
        Journal::create(&dir,Record{version:1,device_id:"old-device".into(),serial:None,source:Source{path:"old-input".into(),..source.clone()},
            settings:if vendor==0x1b1c{vec![]}else{vec![Setting::Button{id:button,action:ButtonAction::Key{usage:4,mods:Modifiers(0)}}]},
            expected:if vendor==0x1b1c{Default::default()}else{std::collections::HashMap::from([(button,ButtonAction::Key{usage:0x68,mods:Modifiers(0)})])},software_input:vendor==0x1b1c,
            shift:if vendor==0x1b1c{Some(macro_recovery::ShiftRestore{original:dpi.clone(),expected:DpiState{stages:vec![DpiStage::uniform(400)],active:0}})}else{None}}).unwrap();
        let fail=Arc::new(AtomicBool::new(false));let mut manager=DeviceManager::new(Box::new(MockBackend::new(vec![],|_|Err(gearwise_core::Error::Unsupported("no physical HID")))),vec![]);
        let descriptor=device.descriptor().clone();manager.add_virtual(vec![Box::new(Fault{inner:device,descriptor,fail:fail.clone()})]);let service=Service::default();*service.0.lock().unwrap()=Some(manager);
        let runtime=Runtime::simulation(Arc::new(Mutex::new(NoOutput)));log.lock().unwrap().clear();
        (service,dir,runtime,source,log,fail)
    }
    fn id(service:&Service)->String{service.peers()[0].id.clone()}
    fn cleanup(dir:&Path){for j in macro_recovery::load(dir).0{j.finish().unwrap();}std::fs::remove_dir(dir).unwrap();}
    #[test]fn manual_recovery_restores_real_drivers_for_all_three_exact_models(){
        for(v,p)in[(0x1532,0x00b4),(0x1b1c,0x1b2e),(0x1038,0x1720)]{
            let(s,dir,runtime,source,log,_)=setup(v,p);let id=id(&s);let list=s.recovery_list_from(&dir,&id).unwrap();assert_eq!(list.records.len(),1);assert!(log.lock().unwrap().is_empty());
            if v==0x1b1c{runtime.simulate_event(crate::raw_input::Event::Buttons{source:source.clone(),mask:0});}
            let r=&list.records[0];s.restore_recovery_from(&dir,&id,&r.key,&r.revision,&source,true,&runtime).unwrap();
            let state=s.with(|m|Ok(m.device_mut(&id).unwrap().read_state().unwrap())).unwrap();
            if v==0x1b1c{assert_eq!(state.dpi.unwrap().stages[0].x,1600);assert!(log.lock().unwrap().iter().any(|r|matches!(r,gearwise_core::transport::MockRecord::Write(bytes) if bytes.starts_with(&[0,7,4,1]))));}
            else{assert_eq!(state.buttons[&if v==0x1532{64}else{4}],ButtonAction::Key{usage:4,mods:Modifiers(0)});}
            assert!(macro_recovery::load(&dir).0.is_empty());runtime.shutdown();cleanup(&dir);
        }
    }
    #[test]fn confirmation_revision_source_model_and_traversal_fail_without_writes(){
        let(s,dir,runtime,source,log,_)=setup(0x1038,0x1720);let id=id(&s);let list=s.recovery_list_from(&dir,&id).unwrap();let r=&list.records[0];
        for(key,rev,src,confirmed)in[(&*r.key,&*r.revision,source.clone(),false),(&*r.key,"stale",source.clone(),true),("../macro.json",&*r.revision,source.clone(),true),(&*r.key,&*r.revision,Source{vendor:0x046d,..source.clone()},true)]{
            assert!(s.restore_recovery_from(&dir,&id,key,rev,&src,confirmed,&runtime).is_err());assert!(log.lock().unwrap().is_empty());
        }runtime.shutdown();cleanup(&dir);
    }
    #[test]fn held_and_unobserved_vendor_input_and_write_failure_keep_journal_for_retry(){
        let(s,dir,runtime,source,log,fail)=setup(0x1b1c,0x1b2e);let id=id(&s);let list=s.recovery_list_from(&dir,&id).unwrap();let r=&list.records[0];
        assert!(s.restore_recovery_from(&dir,&id,&r.key,&r.revision,&source,true,&runtime).unwrap_err().contains("按钮状态"));assert!(log.lock().unwrap().is_empty());
        runtime.simulate_event(crate::raw_input::Event::Buttons{source:source.clone(),mask:8});
        assert!(s.restore_recovery_from(&dir,&id,&r.key,&r.revision,&source,true,&runtime).is_err());assert!(log.lock().unwrap().is_empty());
        runtime.simulate_event(crate::raw_input::Event::Buttons{source:source.clone(),mask:0});fail.store(true,Ordering::Release);
        assert!(s.restore_recovery_from(&dir,&id,&r.key,&r.revision,&source,true,&runtime).unwrap_err().contains("DPI"));assert_eq!(macro_recovery::load(&dir).0.len(),1);
        fail.store(false,Ordering::Release);s.restore_recovery_from(&dir,&id,&r.key,&r.revision,&source,true,&runtime).unwrap();assert!(macro_recovery::load(&dir).0.is_empty());runtime.shutdown();cleanup(&dir);
    }
    #[test]fn known_serial_mismatch_and_unknown_model_are_never_accepted(){
        let(s,dir,runtime,source,_,_)=setup(0x1532,0x00b4);let descriptor=s.peers()[0].clone();let mut record=macro_recovery::load(&dir).0[0].record().unwrap();record.serial=Some("another-unit".into());assert!(target(&record,&descriptor,Some(&source)).is_err());record.serial=None;let mut other=descriptor;other.vendor_id=0x046d;assert!(target(&record,&other,Some(&source)).is_err());runtime.shutdown();cleanup(&dir);
    }
    #[test]fn held_keyboard_and_active_route_do_not_allow_manual_override(){
        let(s,dir,runtime,source,log,_)=setup(0x1038,0x1720);let id=id(&s);let list=s.recovery_list_from(&dir,&id).unwrap();let r=&list.records[0];
        runtime.simulate_event(crate::raw_input::Event::Key{source:source.clone(),vkey:0x7c,down:true});
        assert!(s.restore_recovery_from(&dir,&id,&r.key,&r.revision,&source,true,&runtime).unwrap_err().contains("仍按住"));assert!(log.lock().unwrap().is_empty());
        runtime.simulate_event(crate::raw_input::Event::Key{source:source.clone(),vkey:0x7c,down:false});runtime.remove(&source.path);
        runtime.arm(source.clone(),vec![],crate::macro_runtime::InputMap{keys:Default::default(),buttons:vec![],dpi_buttons:vec![]},crate::macro_runtime::Hooks::default()).unwrap();
        assert!(s.restore_recovery_from(&dir,&id,&r.key,&r.revision,&source,true,&runtime).unwrap_err().contains("运行或停止"));assert!(log.lock().unwrap().is_empty());assert_eq!(runtime.status().source_count,1);runtime.shutdown();cleanup(&dir);
    }
    #[test]fn a_changed_journal_cannot_reuse_an_old_review(){
        let(s,dir,runtime,source,log,_)=setup(0x1b1c,0x1b2e);let id=id(&s);let list=s.recovery_list_from(&dir,&id).unwrap();let r=&list.records[0];
        let journals=macro_recovery::load(&dir).0;journals[0].shift(None).unwrap();
        assert!(s.restore_recovery_from(&dir,&id,&r.key,&r.revision,&source,true,&runtime).unwrap_err().contains("记录已变化"));assert!(log.lock().unwrap().is_empty());assert_eq!(macro_recovery::load(&dir).0.len(),1);runtime.shutdown();cleanup(&dir);
    }
    #[test]fn a_replaced_same_model_target_cannot_reuse_the_previous_confirmation(){
        let(s,dir,runtime,source,_,_)=setup(0x1532,0x00b4);let id=id(&s);let list=s.recovery_list_from(&dir,&id).unwrap();let r=&list.records[0];
        let(device,log)=simulated(0x1532,0x00b4).unwrap();let mut descriptor=device.descriptor().clone();descriptor.id=id.clone();descriptor.serial=Some("replacement-unit".into());
        s.with(|m|{m.remove_virtual();m.add_virtual(vec![Box::new(Fault{inner:device,descriptor,fail:Arc::new(AtomicBool::new(false))})]);Ok(())}).unwrap();log.lock().unwrap().clear();
        assert!(s.restore_recovery_from(&dir,&id,&r.key,&r.revision,&source,true,&runtime).unwrap_err().contains("目标鼠标已更换"));assert!(log.lock().unwrap().is_empty());assert_eq!(macro_recovery::load(&dir).0.len(),1);runtime.shutdown();cleanup(&dir);
    }
}

#[cfg(test)]mod bank_identity_tests{
 use super::*;
 #[test]fn bank_scoped_manual_recovery_never_relaxes_serial_requirements(){for(v,p)in[(0x3554,0xf507),(0x1532,0x00aa),(0x1532,0x00ab)]{let(mut device,_)=super::super::simulated(v,p).unwrap();let _=device.read_state().unwrap();let mut d=device.descriptor().clone();let mut r=Record{version:1,device_id:d.id.clone(),serial:None,source:Source{vendor:v,product:p,path:"simulation:manual".into()},settings:vec![],expected:Default::default(),software_input:false,shift:None};d.serial=None;assert!(target(&r,&d,None).is_err());d.serial=Some("a".into());r.serial=d.serial.clone();assert!(target(&r,&d,None).is_ok());r.device_id="old:board:6".into();assert!(target(&r,&d,None).is_err());}}
}
