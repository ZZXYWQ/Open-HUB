//! Durable write-ahead records for temporary native macro mappings.
//! These records never contain executable macros and never select by brand alone.
use crate::host_macros::Source;
use gearwise_core::{DeviceDescriptor, Setting, model::{ButtonAction, DpiState}};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, io::{Read,Write}, path::{Path, PathBuf}, sync::Mutex};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShiftRestore { pub original: DpiState, pub expected: DpiState }
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub version: u8,
    pub device_id: String,
    pub serial: Option<String>,
    pub source: Source,
    pub settings: Vec<Setting>,
    pub expected: HashMap<u16, ButtonAction>,
    pub software_input: bool,
    pub shift: Option<ShiftRestore>,
}
impl Record {
    pub fn matches(&self, d: &DeviceDescriptor) -> bool {
        let bank=|id:&str|id.rsplit_once(":board:").and_then(|(_,v)|v.parse::<u8>().ok()).filter(|n|*n<4);
        let same_bank=!crate::atk::is_pulsar(self.source.vendor,self.source.product)||(bank(&self.device_id).is_some()&&bank(&self.device_id)==bank(&d.id));
        same_bank && (self.source.vendor,self.source.product)==(d.vendor_id,d.product_id)
            && self.serial.as_deref().filter(|s| !s.trim().is_empty())
                .is_some_and(|s| d.serial.as_deref()==Some(s))
    }
    fn validate(&self) -> Result<(), String> {
        if self.version!=1 || !crate::multibrand::supported(self.source.vendor,self.source.product)
            || self.device_id.is_empty() || self.device_id.len()>4096 || self.source.path.len()>4096
            || self.settings.len()>12 || self.expected.len()>12 {
            return Err("恢复记录的版本、型号或大小无效".into());
        }
        if self.software_input != ((self.source.vendor,self.source.product)==(0x1b1c,0x1b2e)) {
            return Err("恢复记录的软件路由类型与型号不一致".into());
        }
        let mut ids=std::collections::HashSet::new();
        for setting in &self.settings {
            let Setting::Button{id,action}=setting else {return Err("恢复记录只允许临时按键动作".into());};
            if !ids.insert(*id) || matches!(action,ButtonAction::Macro(_)) || !self.expected.contains_key(id) {
                return Err("恢复记录的按键重复或缺少归属信息".into());
            }
            crate::multibrand::validate(&crate::multibrand::Plan{vendor:self.source.vendor,product:self.source.product,settings:vec![setting.clone()],macros:vec![],source:None})?;
            if !matches!(self.expected[id],ButtonAction::Key{usage:0x68..=0x73,mods:gearwise_core::model::Modifiers(0)}) {
                return Err("恢复记录包含未知的临时触发键".into());
            }
        }
        if let Some(shift)=&self.shift {
            if !self.software_input{return Err("此型号不应包含软件狙击 DPI".into());}
            for dpi in [&shift.original,&shift.expected] {
                crate::multibrand::validate(&crate::multibrand::Plan{vendor:self.source.vendor,product:self.source.product,settings:vec![Setting::Dpi(dpi.clone())],macros:vec![],source:None})?;
            }
        }
        Ok(())
    }
}
pub struct Journal { pub path: PathBuf, record: Mutex<Record> }
impl Journal {
    pub fn create(dir:&Path,record:Record)->Result<Self,String>{
        record.validate()?;
        std::fs::create_dir_all(dir).map_err(|e|e.to_string())?;
        let journal=Self{path:dir.join(format!("macro-{}.json",crate::profiles::new_id())),record:Mutex::new(record)};
        {let record=journal.record.lock().map_err(|_|"恢复记录锁异常")?;journal.persist(&record)?;}
        Ok(journal)
    }
    fn persist(&self,record:&Record)->Result<(),String>{
        record.validate()?;
        let bytes=serde_json::to_vec_pretty(record).map_err(|e|e.to_string())?;
        let tmp=self.path.with_extension("json.tmp");
        let mut file=std::fs::File::create(&tmp).map_err(|e|format!("不能创建宏恢复记录：{e}"))?;
        file.write_all(&bytes).and_then(|_|file.sync_all()).map_err(|e|e.to_string())?;
        drop(file);
        std::fs::rename(&tmp,&self.path).map_err(|e|format!("不能提交宏恢复记录：{e}"))
    }
    pub fn record(&self)->Result<Record,String>{Ok(self.record.lock().map_err(|_|"恢复记录锁异常")?.clone())}
    pub fn shift(&self,shift:Option<ShiftRestore>)->Result<(),String>{
        let mut record=self.record.lock().map_err(|_|"恢复记录锁异常")?;
        let mut next=record.clone();next.shift=shift;self.persist(&next)?;*record=next;Ok(())
    }
    pub fn finish(&self)->Result<(),String>{match std::fs::remove_file(&self.path){Ok(())=>Ok(()),Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(()),Err(e)=>Err(format!("恢复完成，但无法清除恢复记录：{e}"))}}
}
pub fn directory()->Result<PathBuf,String>{Ok(PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA unavailable")?).join("Open HUB/recovery"))}
pub fn load(dir:&Path)->(Vec<Journal>,Vec<String>){
    let entries=match std::fs::read_dir(dir){Ok(v)=>v,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return(vec![],vec![]),Err(e)=>return(vec![],vec![format!("无法读取宏恢复目录：{e}")])};
    let mut records=Vec::new();let mut errors=Vec::new();
    for entry in entries.take(128){
        let result=(||->Result<Option<Journal>,String>{
            let path=entry.map_err(|e|e.to_string())?.path();
            if path.extension().and_then(|s|s.to_str())!=Some("json"){return Ok(None);}
            let Some(record)=read_record_file(&path)?else{return Ok(None)};Ok(Some(Journal{path,record:Mutex::new(record)}))
        })();
        match result{Ok(Some(v))=>records.push(v),Ok(None)=>{},Err(e)=>errors.push(e)}
    }
    (records,errors)
}
fn read_record_file(path:&Path)->Result<Option<Record>,String>{
    let metadata=match std::fs::symlink_metadata(path){Ok(v)=>v,Err(e)if e.kind()==std::io::ErrorKind::NotFound=>return Ok(None),Err(e)=>return Err(e.to_string())};
    if !metadata.is_file()||metadata.len()>64*1024{return Err(format!("恢复记录大小或类型无效：{}",path.display()));}
    let file=match std::fs::File::open(path){Ok(v)=>v,Err(e)if e.kind()==std::io::ErrorKind::NotFound=>return Ok(None),Err(e)=>return Err(e.to_string())};
    let mut bytes=Vec::new();file.take(64*1024+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;if bytes.len()>64*1024{return Err("恢复记录读取期间超过大小限制".into());}
    let record:Record=serde_json::from_slice(&bytes).map_err(|e|format!("恢复记录损坏，已保留原文件：{e}"))?;record.validate()?;Ok(Some(record))
}

#[cfg(test)] mod tests {
    use super::*;
    fn record()->Record{Record{version:1,device_id:"device:a".into(),serial:Some("unit-a".into()),source:Source{vendor:0x1532,product:0x00b4,path:"input:a".into()},settings:vec![Setting::Button{id:0x40,action:ButtonAction::Key{usage:4,mods:gearwise_core::model::Modifiers(0)}}],expected:HashMap::from([(0x40,ButtonAction::Key{usage:0x68,mods:gearwise_core::model::Modifiers(0)})]),software_input:false,shift:None}}
    #[test] fn record_survives_restart_and_is_only_removed_after_finish(){
        let dir=std::env::temp_dir().join(format!("recovery-{}",crate::profiles::new_id()));
        let j=Journal::create(&dir,record()).unwrap();let path=j.path.clone();drop(j);
        let(mut loaded,errors)=load(&dir);assert!(errors.is_empty());assert_eq!(loaded.len(),1);assert!(path.exists());
        assert_eq!(loaded[0].record().unwrap().settings,record().settings);loaded.remove(0).finish().unwrap();assert!(load(&dir).0.is_empty());std::fs::remove_dir(dir).unwrap();
    }
    #[test] fn malformed_and_cross_model_records_cannot_be_used(){
        let mut r=record();r.source.vendor=0x046d;assert!(r.validate().is_err());r=record();r.settings.push(Setting::ReportRate(1000));assert!(r.validate().is_err());r=record();r.expected.clear();assert!(r.validate().is_err());
    }
    #[test] fn serial_identity_is_required_even_when_model_and_path_match(){
        let(mut d,_)=crate::naga::simulated();let mut descriptor=d.descriptor().clone();descriptor.id="device:a".into();descriptor.serial=Some("unit-b".into());assert!(!record().matches(&descriptor));descriptor.serial=Some("unit-a".into());assert!(record().matches(&descriptor));let mut r=record();r.serial=None;assert!(!r.matches(&descriptor));let _=d.read_state();
    }
    #[test]fn completed_record_can_disappear_during_a_scan_but_bad_records_remain_visible(){let dir=std::env::temp_dir().join(format!("recovery-scan-{}",crate::profiles::new_id()));let journal=Journal::create(&dir,record()).unwrap();let path=journal.path.clone();assert!(read_record_file(&path).unwrap().is_some());journal.finish().unwrap();assert!(read_record_file(&path).unwrap().is_none());std::fs::write(&path,b"{ broken JSON").unwrap();assert!(read_record_file(&path).is_err());assert_eq!(std::fs::read(&path).unwrap(),b"{ broken JSON");std::fs::write(&path,vec![b' ';64*1024+1]).unwrap();assert!(read_record_file(&path).is_err());std::fs::remove_file(path).unwrap();std::fs::remove_dir(dir).unwrap();}
}
