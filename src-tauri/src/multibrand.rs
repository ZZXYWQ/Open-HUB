//! Shared mouse bridge, with exact model-specific drivers. Drivers are isolated from Logitech/MCHOSE and selected
//! by exact VID/PID. No writes are performed by scan/read.
use gearwise_core::model::{ButtonAction, DeviceKind, DpiFunction};
use gearwise_core::{Capabilities, DeviceDescriptor, DeviceManager, DeviceState, Setting};
use gearwise_hid::{HidapiBackend, VendorFilter};
use serde::{Deserialize, Serialize};
use std::sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}};
#[path="manual_recovery.rs"]
mod manual_recovery;
pub use manual_recovery::RecoveryList;
#[path="rate_reconnect.rs"]
mod rate_reconnect;
pub use rate_reconnect::{PendingList,PendingCandidates};
#[path="session_plan.rs"]
mod session_plan;
pub use session_plan::SessionView;
#[cfg(test)]
#[path="import_execution_tests.rs"]
mod import_execution_tests;
#[cfg(test)]
#[path="viper_models.rs"]
mod viper_models;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub vendor: u16,
    pub product: u16,
    pub settings: Vec<Setting>,
    #[serde(default)] pub macros:Vec<gearwise_core::profile::Macro>,
    #[serde(default)] pub source:Option<crate::host_macros::Source>,
}
#[derive(Clone, Serialize)]
pub struct View {
    pub descriptor: DeviceDescriptor,
    pub capabilities: Capabilities,
    pub state: DeviceState,
    pub readback: &'static str,
    pub hardware_tested: bool,
    #[serde(skip_serializing_if="Vec::is_empty")]
    pub native_macros:Vec<gearwise_core::profile::Macro>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub session: Option<SessionView>,
}
#[derive(Clone,Default)]
pub struct Service(Arc<Mutex<Option<DeviceManager>>>,Arc<Mutex<std::collections::HashMap<String,Cleanup>>>,Arc<AtomicBool>,Arc<Mutex<std::collections::HashMap<String,session_plan::Session>>>);
#[derive(Clone)]struct Cleanup{source:crate::host_macros::Source,settings:Vec<Setting>,expected:std::collections::HashMap<u16,ButtonAction>,software_input:bool,cancelled:Arc<AtomicBool>,shift:Arc<Mutex<Option<gearwise_core::model::DpiState>>>,journal:Option<Arc<crate::macro_recovery::Journal>>,serial:Option<String>}
pub fn supported(v: u16, p: u16) -> bool {
    crate::legacy::supported(v,p) || crate::atk::supported(v,p) || crate::hyperx::supported(v,p) || matches!(
        (v, p),
        (0x1532, 0x0084) | (0x1532, 0x00b4) | (0x1532, 0x007a | 0x007b | 0x00a5 | 0x00a6 | 0x00c0 | 0x00c1) | (0x1b1c, 0x1b2e) | (0x1038, 0x1720)
    )
}
fn view(d: &mut dyn gearwise_core::Device) -> Result<View, String> {
    let state = d.read_state().map_err(|e| e.to_string())?;
    Ok(View {
        descriptor: d.descriptor().clone(),
        capabilities: d.capabilities().clone(),
        state,
        readback: if d.descriptor().vendor_id == 0x1038 {
            "write_only_cached"
        } else if d.descriptor().vendor_id == 0x1b1c {
            "stored_and_session_cached"
        } else {
            "device_partial"
        },
        hardware_tested: false,
        native_macros:d.macro_definitions(),
        session: None,
    })
}
impl Service {
    fn with<T>(
        &self,
        f: impl FnOnce(&mut DeviceManager) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut guard = self
            .0
            .lock()
            .map_err(|_| "Three-brand manager lock poisoned")?;
        if guard.is_none() {
            let backend = HidapiBackend::with_filter(VendorFilter::only([0x1532, 0x1b1c, 0x1038, 0x373b, 0x3554, 0x03f0, 0x0b05, 0x1e7d, 0x28da, 0x1ea7, 0x04d9]))
                .map_err(|e| e.to_string())?;
            *guard = Some(DeviceManager::new(
                Box::new(ExactBackend(backend)),
                vec![
                    Box::new(crate::naga::NagaDriver),
                    Box::new(crate::atk::AtkDriver),
                    Box::new(crate::hyperx::HasteDriver),
                    Box::new(crate::legacy::WiredDriver),
                    Box::new(gearwise_razer::RazerDriver::new()),
                    Box::new(gearwise_corsair::CorsairDriver),
                    Box::new(gearwise_steelseries::SteelSeriesDriver::default()),
                ],
            ));
        }
        f(guard.as_mut().unwrap())
    }
    pub fn scan(&self) -> Result<(Vec<DeviceDescriptor>, Vec<String>), String> {
        self.with(|m| {
            let r = m.scan();
            Ok((
                m.devices().map(|d| d.descriptor().clone()).collect(),
                r.issues.into_iter().map(|v| v.message).collect(),
            ))
        })
    }
    pub fn peers(&self) -> Vec<DeviceDescriptor> {
        self.0
            .lock()
            .ok()
            .and_then(|m| {
                m.as_ref()
                    .map(|m| m.devices().map(|d| d.descriptor().clone()).collect())
            })
            .unwrap_or_default()
    }
    pub fn read(&self, id: &str,runtime:&crate::macro_runtime::Runtime) -> Result<View, String> {
        self.with(|m|{let mut out=view(m.device_mut(raw_id(id)).ok_or("鼠标已断开")?.as_mut())?;self.attach_session(&mut out,runtime)?;Ok(out)})
    }
    pub fn apply(&self,id:&str,plan:&Plan,runtime:&crate::macro_runtime::Runtime)->Result<View,String>{self.apply_at(id,plan,runtime,&rate_reconnect::root()?,None)}
    fn apply_at(&self,id:&str,plan:&Plan,runtime:&crate::macro_runtime::Runtime,root:&std::path::Path,resume:Option<(&str,&str,&str)>)->Result<View,String>{
        validate(plan)?;
        let mut compiled=compile(plan)?;
        let peer=self.peers().into_iter().find(|d|d.id==raw_id(id)).ok_or("鼠标已断开")?;
        if (peer.vendor_id,peer.product_id)!=(plan.vendor,plan.product){return Err("方案与当前鼠标的品牌/型号不一致".into());}
        let source=if compiled.bindings.is_empty()&&!compiled.software_input{None}else{let source=plan.source.clone().ok_or("软件宏需要选择此鼠标的具体输入来源")?;runtime.prepare(&source,peer.serial.as_deref())?;Some(source)};
        self.with(|manager|{
            rate_reconnect::guard_apply(root,manager,id,resume)?;
            let current=manager.device_mut(raw_id(id)).ok_or("鼠标已断开")?.descriptor();
            if (current.vendor_id,current.product_id)!=(plan.vendor,plan.product){return Err("方案与当前鼠标的品牌/型号不一致".into());}
            if peer.serial.as_deref().filter(|s|!s.trim().is_empty()).is_some_and(|serial|current.serial.as_deref()!=Some(serial)){return Err("鼠标序列号已改变，请重新选择设备".into());}
            self.restore_one(manager,raw_id(id),runtime)?;
            let device=manager.device_mut(raw_id(id)).ok_or("鼠标已断开")?;
            if device.descriptor().vendor_id!=plan.vendor||device.descriptor().product_id!=plan.product{return Err("方案与当前鼠标的品牌/型号不一致".into());}
            let native=Plan{vendor:plan.vendor,product:plan.product,settings:compiled.settings.clone(),macros:vec![],source:None};
            validate_capabilities(&native,device.capabilities())?;
            let before=view(device.as_mut())?;
            if native_macros(plan.vendor,plan.product){device.set_macro_definitions(&plan.macros).map_err(|e|e.to_string())?;}
            if (plan.vendor,plan.product)==(0x1b1c,0x1b2e)&&plan.settings.iter().any(|s|matches!(s,Setting::ReportRate(rate) if before.state.report_rate!=Some(*rate))){
                let message=rate_reconnect::begin(root,device.as_mut(),plan)?;manager.drop_device(raw_id(id));return Err(message);
            }
            let mut restore=Vec::new();
            resolve_defaults(&mut compiled,&before)?;
            for binding in compiled.bindings.iter().filter(|_|!compiled.software_input){
                let original=before.state.buttons.get(&binding.button).cloned();
                if plan.vendor==0x1532&&original.is_none(){return Err("无法读取此 Naga 按键原始动作，不能安全配置软件宏".into());}
                restore.push(Setting::Button{id:binding.button,action:original.unwrap_or(ButtonAction::Default)});
            }
            let expected=compiled.settings.iter().filter_map(|s|if let Setting::Button{id,action}=s{compiled.bindings.iter().any(|b|b.button==*id).then(||(*id,action.clone()))}else{None}).collect::<std::collections::HashMap<_,_>>();
            let dir=root.join("backups");
            std::fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
            let snapshot=dir.join(format!("three-brand-{}.json",crate::profiles::new_id()));
            std::fs::write(&snapshot,serde_json::to_vec_pretty(&serde_json::json!({"before":before,"macro_restore":restore,"owned_expected":expected,"source":source})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
            let cancelled=Arc::new(AtomicBool::new(false));let shift=Arc::new(Mutex::new(None));
            let journal=if let Some(source)=source.clone(){Some(Arc::new(crate::macro_recovery::Journal::create(&root.join("recovery"),crate::macro_recovery::Record{version:1,device_id:raw_id(id).into(),serial:peer.serial.clone(),source,settings:restore.clone(),expected:expected.clone(),software_input:compiled.software_input,shift:None})?))}else{None};
            if let Some(source)=source.clone(){self.1.lock().map_err(|_|"宏恢复记录锁异常")?.insert(raw_id(id).into(),Cleanup{source,settings:restore,expected,software_input:compiled.software_input,cancelled:cancelled.clone(),shift:shift.clone(),journal:journal.clone(),serial:peer.serial.clone()});}
            let operation=(||->Result<View,String>{
                for (index,setting) in compiled.settings.iter().enumerate(){if matches!(setting,Setting::ReportRate(rate) if before.state.report_rate==Some(*rate)){continue;}device.apply(setting).map_err(|e|format!("第 {} 项写入失败：{e}；此前设置可能已生效，快照：{}",index+1,snapshot.display()))?;}
                view(device.as_mut())
            })();
            let mut result=match operation{Ok(v)=>v,Err(e)=>{let restored=self.restore_one(manager,raw_id(id),runtime);return Err(format!("{e}；宏触发键恢复：{}",restored.map(|_|"完成".to_string()).unwrap_or_else(|e|e)));}};
            let needs_software=source.is_some();
            let hooks=if compiled.software_input{
                let manager=self.0.clone();let device_id=raw_id(id).to_string();let actions=compiled.dpi_actions.clone();let stopped=cancelled.clone();let saved=shift.clone();
                crate::macro_runtime::Hooks{dpi:Some(Arc::new(move|button,down|{let mut manager=manager.lock().map_err(|_|"DPI 设备锁异常")?;if stopped.load(Ordering::Acquire){return Ok(());}let device=manager.as_mut().and_then(|m|m.device_mut(&device_id)).ok_or("DPI 鼠标已断开")?;let action=actions.get(&button).ok_or("DPI 动作缺失")?;software_dpi_journal(device.as_mut(),action,down,&saved,journal.as_deref())})),cancel:Some(Arc::new(move||{cancelled.store(true,Ordering::Release);} ))}
            }else{crate::macro_runtime::Hooks{dpi:None,cancel:Some(Arc::new(move||{cancelled.store(true,Ordering::Release);} ))}};
            if let Some(source)=source{if let Err(e)=runtime.arm(source,compiled.bindings,compiled.input,hooks){let restore=self.restore_one(manager,raw_id(id),runtime);return Err(format!("{e}；恢复结果：{restore:?}"));}}
            self.remember_session(&result,plan,needs_software)?;
            self.attach_session(&mut result,runtime)?;
            if let Some((key,_,_))=resume{rate_reconnect::finish(root,key)?;}
            Ok(result)
        })
    }
    fn restore_one(&self,manager:&mut DeviceManager,id:&str,runtime:&crate::macro_runtime::Runtime)->Result<(),String>{
        let cleanup=self.1.lock().map_err(|_|"宏恢复记录锁异常")?.get(id).cloned();
        if let Some(cleanup)=cleanup{
            cleanup.cancelled.store(true,Ordering::Release);
            let trigger_keys=cleanup.expected.values().filter_map(|a|match a{ButtonAction::Key{usage:0x68..=0x73,mods:gearwise_core::model::Modifiers(0)}=>if let ButtonAction::Key{usage,..}=a{Some(u16::from(*usage)+0x14)}else{None},_=>None}).collect::<Vec<_>>();
            if !runtime.quiesce(&cleanup.source,&trigger_keys)?{return Err(if runtime.is_quiescing(&cleanup.source){"宏已停止；请松开仍按住的触发键，随后会自动恢复原始映射"}else{"宏已停止；请松开触发键后重新检查待恢复记录"}.into());}
            runtime.remove(&cleanup.source.path);
            let device=manager.device_mut(id).ok_or("鼠标已断开，宏已停止；原始按键需连接后再恢复")?;
            if (device.descriptor().vendor_id,device.descriptor().product_id)!=(cleanup.source.vendor,cleanup.source.product){return Err("恢复目标与原始品牌/型号不一致".into());}
            if cleanup.serial.as_deref().filter(|s|!s.trim().is_empty()).is_some_and(|s|device.descriptor().serial.as_deref()!=Some(s)){return Err("鼠标序列号已改变，保留恢复记录，未写入其他鼠标".into());}
            if cleanup.software_input{device.stop_software_actions().map_err(|e|format!("软件按键硬件模式恢复失败：{e}"))?;let mut shift=cleanup.shift.lock().map_err(|_|"狙击 DPI 锁异常")?;if let Some(original)=shift.as_ref(){let owned=cleanup.journal.as_ref().map(|j|j.record()).transpose()?.and_then(|r|r.shift);let actual=device.read_state().map_err(|e|e.to_string())?.dpi;if owned.as_ref().is_some_and(|s|actual.as_ref()!=Some(&s.expected)&&actual.as_ref()!=Some(original)){return Err("本次会话的 DPI 已改变，保留恢复记录，未覆盖当前设置".into());}device.apply(&Setting::Dpi(original.clone())).map_err(|e|format!("狙击 DPI 恢复失败：{e}"))?;if let Some(j)=&cleanup.journal{j.shift(None)?;}*shift=None;}}
            let actual=if cleanup.source.vendor==0x1532{Some(device.read_state().map_err(|e|format!("宏已停止，无法读取当前按键以验证恢复归属：{e}"))?)}else{None};
            let mut changed=Vec::new();
            for setting in &cleanup.settings{
                if let(Setting::Button{id,action},Some(actual))=(setting,&actual){let current=actual.buttons.get(id);if current==Some(action){continue;}if !cleanup.expected.contains_key(id)||cleanup.expected.get(id)!=current{changed.push(*id);continue;}}
                device.apply(setting).map_err(|e|format!("宏已停止，但原始按键恢复失败：{e}"))?;
            }
            if let Some(journal)=&cleanup.journal{journal.finish()?;}
            self.1.lock().map_err(|_|"宏恢复记录锁异常")?.remove(id);
            if !changed.is_empty(){return Err(format!("宏已停止；按键 {changed:?} 已被其他控制器改变，保留当前动作，未覆盖外部设置"));}
        }Ok(())
    }
    pub fn stop_software(&self,runtime:&crate::macro_runtime::Runtime)->Result<(),String>{
        let ids=self.1.lock().map_err(|_|"宏恢复记录锁异常")?.keys().cloned().collect::<Vec<_>>();
        let mut errors=Vec::new();
        for id in ids{if let Err(e)=self.with(|m|self.restore_one(m,&id,runtime)){errors.push(e);}}
        if !runtime.has_quiescing(){runtime.shutdown();}else if let Err(e)=self.schedule_release_restore(runtime){errors.push(e);}
        if errors.is_empty(){Ok(())}else{Err(errors.join("；"))}
    }
    pub fn stop_device_software(&self,id:&str,runtime:&crate::macro_runtime::Runtime)->Result<(),String>{
        let result=self.with(|m|self.restore_one(m,raw_id(id),runtime));
        if runtime.has_quiescing(){self.schedule_release_restore(runtime)?;}
        // Keep the observer: other devices or a concurrent prepared apply may
        // still need it. Only the explicit global stop/exit shuts it down.
        result
    }
    fn release_pending_ids(&self,runtime:&crate::macro_runtime::Runtime)->Result<Vec<String>,String>{
        let candidates=self.1.lock().map_err(|_|"宏恢复记录锁异常")?.iter().filter(|(_,c)|c.cancelled.load(Ordering::Acquire)).map(|(id,c)|(id.clone(),c.source.clone())).collect::<Vec<_>>();
        Ok(candidates.into_iter().filter(|(_,source)|runtime.is_quiescing(source)).map(|(id,_)|id).collect())
    }
    fn schedule_release_restore(&self,runtime:&crate::macro_runtime::Runtime)->Result<(),String>{
        if self.2.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){return Ok(());}
        let service=self.clone();let runtime=runtime.clone();let registration=self.2.clone();
        let spawned=std::thread::Builder::new().name("open-hub-release-restore".into()).spawn(move||{
            let mut owns_registration=true;
            let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||->Result<(),String>{loop{
                let ids=service.release_pending_ids(&runtime)?;
                if ids.is_empty(){
                    registration.store(false,Ordering::Release);owns_registration=false;
                    // Close the race with a stop request arriving as we exit.
                    if !service.release_pending_ids(&runtime)?.is_empty()&&registration.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_ok(){owns_registration=true;continue;}
                    return Ok(());
                }
                for id in ids{
                    let restored=service.with(|manager|{
                        let cleanup=service.1.lock().map_err(|_|"宏恢复记录锁异常")?.get(&id).cloned();
                        if cleanup.as_ref().is_some_and(|c|c.cancelled.load(Ordering::Acquire)&&runtime.is_quiescing(&c.source)){service.restore_one(manager,&id,&runtime)}else{Ok(())}
                    });
                    if let Err(error)=restored{let still_waiting=service.1.lock().map_err(|_|"宏恢复记录锁异常")?.get(&id).is_some_and(|c|runtime.is_quiescing(&c.source));if !still_waiting{runtime.notice(format!("按键恢复提示：{error}"));}}
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }}));
            if owns_registration{registration.store(false,Ordering::Release);}
            match result{Ok(Err(e))=>runtime.notice(e),Err(_)=>runtime.notice("按键恢复等待任务异常退出，恢复记录仍需检查".into()),_=>{}}
        });
        if let Err(error)=spawned{self.2.store(false,Ordering::Release);return Err(format!("宏已停止，无法启动等待恢复任务：{error}"));}Ok(())
    }

    pub fn recover_pending(&self,runtime:&crate::macro_runtime::Runtime)->Result<Vec<String>,String>{
        self.recover_from(&crate::macro_recovery::directory()?,runtime)
    }
    fn recover_from(&self,directory:&std::path::Path,runtime:&crate::macro_runtime::Runtime)->Result<Vec<String>,String>{
        let (journals,mut issues)=crate::macro_recovery::load(directory);
        let result=self.with(|manager|{
            for journal in journals{
                if !journal.path.try_exists().map_err(|e|e.to_string())?{continue;}
                let tracked=self.1.lock().map_err(|_|"宏恢复记录锁异常")?.iter().filter(|(_,c)|c.journal.as_ref().is_some_and(|j|j.path==journal.path)).map(|(id,c)|(id.clone(),c.cancelled.load(Ordering::Acquire))).collect::<Vec<_>>();
                if tracked.iter().any(|(_,cancelled)|!*cancelled){continue;}
                let record=journal.record()?;
                if runtime.is_quiescing(&record.source){continue;}
                if record.source.vendor!=0x1532{issues.push("海盗船/赛睿待恢复设置需要手动检查：尚不能可靠核实当前软件路由归属，重启后未自动覆盖".into());continue;}
                let matches=manager.devices().filter(|d|record.matches(d.descriptor())).map(|d|d.descriptor().id.clone()).collect::<Vec<_>>();
                if matches.len()!=1{issues.push(format!("有待恢复的宏按键（{:04X}:{:04X}）；需要唯一匹配的鼠标序列号，当前未自动写入",record.source.vendor,record.source.product));continue;}
                let id=&matches[0];
                {let mut active=self.1.lock().map_err(|_|"宏恢复记录锁异常")?;for (old,_) in &tracked{active.remove(old);}}
                if self.1.lock().map_err(|_|"宏恢复记录锁异常")?.contains_key(id){issues.push("当前鼠标已有运行中的宏，保留旧恢复记录，未覆盖".into());continue;}
                self.1.lock().map_err(|_|"宏恢复记录锁异常")?.insert(id.clone(),Cleanup{source:record.source,settings:record.settings,expected:record.expected,software_input:record.software_input,cancelled:Arc::new(AtomicBool::new(true)),shift:Arc::new(Mutex::new(record.shift.map(|s|s.original))),journal:Some(Arc::new(journal)),serial:record.serial});
            }
            let pending=self.1.lock().map_err(|_|"宏恢复记录锁异常")?.iter().filter(|(_,c)|c.cancelled.load(Ordering::Acquire)).map(|(id,c)|(id.clone(),c.serial.clone(),c.source.vendor,c.source.product,c.source.clone())).collect::<Vec<_>>();
            for (id,serial,vendor,product,source) in pending{
                // This is the same still-observed physical input source from an
                // explicit stop in this process, not a restart identity guess.
                if runtime.is_quiescing(&source){if let Err(e)=self.restore_one(manager,&id,runtime){issues.push(e);}continue;}
                if vendor!=0x1532{issues.push("宏已停止；软件路由归属仍需手动核实，保留恢复记录".into());continue;}
                let Some(serial)=serial.filter(|s|!s.trim().is_empty()) else {issues.push("宏已停止；缺少唯一序列号，保留恢复记录，等待手动检查".into());continue;};
                if manager.devices().filter(|d|d.descriptor().vendor_id==vendor&&d.descriptor().product_id==product&&d.descriptor().serial.as_deref()==Some(&serial)).count()!=1{issues.push("宏已停止；无法唯一识别原始鼠标，未自动恢复".into());continue;}
                if manager.device_mut(&id).is_some(){if let Err(e)=self.restore_one(manager,&id,runtime){issues.push(e);}}
            }
            Ok(issues)
        })?;
        // Keep the idle observer until explicit stop/exit: a concurrent apply
        // may already have prepared it while waiting for the manager lock.
        Ok(result)
    }

}
fn raw_id(id: &str) -> &str {
    id.strip_prefix("multi:").unwrap_or(id)
}
struct ExactBackend(HidapiBackend);
impl gearwise_core::HidBackend for ExactBackend {
    fn enumerate(&self) -> gearwise_core::Result<Vec<gearwise_core::HidInterfaceInfo>> {
        use gearwise_core::HidBackend;
        Ok(self
            .0
            .enumerate()?
            .into_iter()
            .filter(|i| supported(i.vendor_id, i.product_id))
            .collect())
    }
    fn open(&self, path: &str) -> gearwise_core::Result<Box<dyn gearwise_core::HidTransport>> {
        use gearwise_core::HidBackend;
        match self.0.open(path){Ok(v)=>Ok(v),Err(original)=>{
 #[cfg(windows)] if path.to_ascii_uppercase().contains("VID_1532"){return crate::feature_transport::FeatureOnly::open(path).map(|v|Box::new(v) as Box<dyn gearwise_core::HidTransport>);}
 Err(original)}}
    }
}
pub fn validate(p: &Plan) -> Result<(), String> {
    let _compiled=compile(p)?;
    if !supported(p.vendor, p.product) {
        return Err("尚未实现该具体型号".into());
    }
    if p.settings.is_empty() || p.settings.len() > 64 {
        return Err("配置需要 1–64 项设置".into());
    }
    let mut seen = std::collections::HashSet::new();
    for s in &p.settings {
        let k = match s {
            Setting::Dpi(_) => "dpi".into(),
            Setting::ReportRate(_) => "rate".into(),
            Setting::Lighting { zone, .. } => format!("light:{zone}"),
            Setting::Button { id, .. } => format!("button:{id}"),
            _ => return Err("该设置尚未接入".into()),
        };
        if !seen.insert(k) {
            return Err("同一项设置不能重复".into());
        }
        if crate::atk::model(p.vendor,p.product).is_some_and(|m|!m.native_macros){match s{Setting::Dpi(d)=>crate::atk::validate_dpi(d,p.product).map_err(|e|e.to_string())?,Setting::ReportRate(r)if crate::atk::model(p.vendor,p.product).unwrap().rates.contains(r)=>{},Setting::Lighting{zone:0,state}if state.brightness==100&&matches!(state.effect,gearwise_core::model::LightingEffect::Off|gearwise_core::model::LightingEffect::Static{..})=>{},Setting::Button{id,action} if (2..=crate::atk::model(p.vendor,p.product).unwrap().buttons).contains(id) && matches!(action,ButtonAction::Default|ButtonAction::Disabled|ButtonAction::Mouse(gearwise_core::model::MouseButton::Left|gearwise_core::model::MouseButton::Right|gearwise_core::model::MouseButton::Middle|gearwise_core::model::MouseButton::Back|gearwise_core::model::MouseButton::Forward)|ButtonAction::Dpi(DpiFunction::Cycle|DpiFunction::Up|DpiFunction::Down))=>{},_=>return Err("Pulsar 已接通基础鼠标动作、DPI 动作及禁用；键盘和板载宏格式仍待验证".into())}continue;}
        if let Some(model)=crate::legacy::model(p.vendor,p.product){crate::legacy::validate_setting(model,s).map_err(|e|e.to_string())?;continue;}
        if crate::hyperx::supported(p.vendor,p.product){crate::hyperx::validate_setting(s).map_err(|e|e.to_string())?;continue;}
        match s {
            Setting::Dpi(d) => {
                if crate::atk::supported(p.vendor,p.product){crate::atk::validate_dpi(d,p.product).map_err(|e|e.to_string())?;}
                let (max, step, count, xy) = if let Some(model)=crate::atk::model(p.vendor,p.product){(model.max_dpi,50,8,true)}else{match p.vendor {
                    0x1532 if p.product==0x00b4 => (30000,1,1,true),
                    0x1532 => {let model=gearwise_razer::models::find(p.product).ok_or("未知雷蛇型号")?;let dpi=model.dpi.ok_or("该型号不支持 DPI")?;(dpi.max,1,if dpi.stages{5}else{1},true)},
                    0x1b1c => (12000, 1, 5, true),
                    0x373b => (if p.product==0x1017{36000}else{30000},50,8,true),
                    _ => (12000, 100, 2, false),
                }};
                if d.stages.is_empty()
                    || d.stages.len() > count
                    || d.active as usize >= d.stages.len()
                    || d.stages.iter().any(|s| {
                        s.x < if crate::atk::supported(p.vendor,p.product)&&p.product==0x1017{50}else{100}
                            || s.y < if crate::atk::supported(p.vendor,p.product)&&p.product==0x1017{50}else{100}
                            || s.x > max
                            || s.y > max
                            || s.x % step != 0
                            || s.y % step != 0
                            || (!xy && s.x != s.y)
                    })
                {
                    return Err("DPI 档位、范围或独立 XY 无效".into());
                }
            }
            Setting::ReportRate(r) => {
                let allowed = if let Some(model)=crate::atk::model(p.vendor,p.product){model.rates.clone()}else if p.vendor==0x1532&&p.product!=0x00b4 {
                    gearwise_razer::models::find(p.product).ok_or("未知雷蛇型号")?.polling.rates.to_vec()
                } else if p.vendor == 0x1532 {
                    vec![125, 500, 1000]
                } else {
                    vec![125, 250, 500, 1000]
                };
                if !allowed.contains(r) {
                    return Err("该型号不支持此回报率".into());
                }
            }
            Setting::Lighting { zone, state } => {
                if state.brightness > 100 {
                    return Err("亮度应为 0–100".into());
                }
                if p.vendor==0x1532&&!gearwise_razer::models::find(p.product).is_some_and(|model|model.zones.iter().any(|z|u16::from(z.led)==u16::from(*zone)&&z.effects.contains(&state.effect.kind()))){return Err("此雷蛇型号不支持该灯区或光效".into());}
            }
            Setting::Button { id, action } => {
                if p.vendor==0x1532&&matches!(p.product,0x00c0|0x00c1){validate_viper_button(*id,action)?;continue;}
                if p.vendor==0x1532&&p.product==0x00b4{if !crate::naga::known_button(*id){return Err("未知 Naga 按键".into());}match action{ButtonAction::Default|ButtonAction::Mouse(_)|ButtonAction::Macro(_)=>{},ButtonAction::Key{usage,..} if (4..=0xa4).contains(usage)=>{},_=>return Err("Naga 不支持此板载动作".into())}if *id==1{return Err("保留左键原始点击".into());}continue;}
                if p.vendor==0x1b1c&&p.product==0x1b2e{if !(2..=8).contains(id){return Err("保留左键，且只允许 M65 PRO 已知按键".into());}match action{ButtonAction::Default|ButtonAction::Disabled|ButtonAction::Mouse(_)|ButtonAction::Macro(_)|ButtonAction::Dpi(DpiFunction::Up|DpiFunction::Down|DpiFunction::Cycle)=>{},ButtonAction::Dpi(DpiFunction::Shift(d)) if (100..=12000).contains(d)=>{},ButtonAction::Key{usage,..} if (4..=0xa4).contains(usage)=>{},_=>return Err("M65 PRO 按键动作无效".into())}continue;}
                if crate::atk::supported(p.vendor,p.product) {if !(2..=crate::atk::model(p.vendor,p.product).unwrap().buttons).contains(id)||!matches!(action,ButtonAction::Default|ButtonAction::Disabled|ButtonAction::Key{..}|ButtonAction::Macro(_)|ButtonAction::Mouse(gearwise_core::model::MouseButton::Left|gearwise_core::model::MouseButton::Right|gearwise_core::model::MouseButton::Middle|gearwise_core::model::MouseButton::Back|gearwise_core::model::MouseButton::Forward)|ButtonAction::Dpi(DpiFunction::Cycle|DpiFunction::Up|DpiFunction::Down)){return Err("该精确型号仅开放已实现的基础按键、键盘点按和板载宏；未知原厂动作保持不变".into());}continue;}
                if p.vendor != 0x1038 || !(1..=6).contains(id) {
                    return Err("此型号的按键写入尚未实现".into());
                }
                match action {
                    ButtonAction::Default
                    | ButtonAction::Disabled
                    | ButtonAction::Mouse(_)
                    | ButtonAction::Dpi(DpiFunction::Cycle) | ButtonAction::Macro(_) => {}
                    ButtonAction::Key { usage, mods }
                        if (4..=0xa4).contains(usage) && mods.0.count_ones() <= 3 => {}
                    _ => return Err("该型号尚不支持此按键动作或宏".into()),
                }
                if *id == 1
                    && !matches!(
                        action,
                        ButtonAction::Default
                            | ButtonAction::Mouse(gearwise_core::model::MouseButton::Left)
                    )
                {
                    return Err("保留左键原始点击".into());
                }
            }
            _ => unreachable!(),
        }
    }
    Ok(())
}
fn validate_capabilities(p: &Plan, c: &Capabilities) -> Result<(), String> {
    for s in &p.settings {
        match s {
            Setting::Dpi(d) => {
                let cap = c.dpi.as_ref().ok_or("设备未报告 DPI 能力")?;
                if d.stages.len() > cap.max_stages as usize
                    || d.stages.iter().any(|s| {
                        cap.snap(s.x) != s.x
                            || cap.snap(s.y) != s.y
                            || (!cap.separate_xy && s.x != s.y)
                    })
                {
                    return Err("设置不符合当前设备能力".into());
                }
            }
            Setting::ReportRate(r) if !c.report_rates.contains(r) => {
                return Err("设备不支持该回报率".into())
            }
            Setting::Lighting { zone, state } => {
                if !c
                    .lighting
                    .iter()
                    .any(|z| z.id == *zone && z.effects.contains(&state.effect.kind()))
                {
                    return Err("设备不支持该灯区或光效".into());
                }
            }
            Setting::Button { id, .. }
                if !c.buttons.iter().any(|b| b.id == *id && b.remappable) =>
            {
                return Err("此按键不支持修改".into())
            }
            _ => {}
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use gearwise_core::model::{DpiStage, DpiState, Modifiers};
    fn plan(v: u16, p: u16) -> Plan {
        Plan { macros:vec![], source:None,
            vendor: v,
            product: p,
            settings: vec![
                Setting::Dpi(DpiState {
                    stages: vec![DpiStage::uniform(800)],
                    active: 0,
                }),
                Setting::ReportRate(1000),
            ],
        }
    }
    #[test]
    fn exact_model_allowlist_never_claims_logitech_mchose() {
        assert!(!supported(0x046d, 0x0084));
        assert!(!supported(0x3837, 0x1720));
        assert!(!supported(0x1532, 0xffff));
    }
    #[test]
    fn plans_roundtrip_for_three_models() {
        for (v, p) in [(0x1532, 0x0084), (0x1532, 0x00b4), (0x1b1c, 0x1b2e), (0x1038, 0x1720)] {
            let p = plan(v, p);
            validate(&p).unwrap();
            assert_eq!(
                serde_json::from_str::<Plan>(&serde_json::to_string(&p).unwrap()).unwrap(),
                p
            );
        }
    }
    #[test]
    fn no_silent_rounding_or_xy_loss() {
        let mut p = plan(0x1038, 0x1720);
        p.settings[0] = Setting::Dpi(DpiState {
            stages: vec![DpiStage { x: 850, y: 800 }],
            active: 0,
        });
        assert!(validate(&p).is_err());
    }
    #[test]
    fn rejects_duplicate_settings_before_any_device_access() {
        let mut p = plan(0x1532, 0x0084);
        p.settings.push(Setting::ReportRate(500));
        assert!(validate(&p).is_err());
    }
    #[test]
    fn rejects_unsupported_macro_and_last_left() {
        let mut p = plan(0x1038, 0x1720);
        p.settings.push(Setting::Button {
            id: 6,
            action: ButtonAction::Macro("test".into()),
        });
        assert!(validate(&p).is_err());
        p.settings.pop();
        p.settings.push(Setting::Button {
            id: 1,
            action: ButtonAction::Key {
                usage: 4,
                mods: Modifiers(0),
            },
        });
        assert!(validate(&p).is_err());
    }
}

type MockLog = std::sync::Arc<std::sync::Mutex<Vec<gearwise_core::transport::MockRecord>>>;
fn simulated(
    vendor: u16,
    product: u16,
) -> Result<(Box<dyn gearwise_core::Device>, MockLog), String> {
    match (vendor, product) {
        (v,p) if crate::legacy::supported(v,p)=>crate::legacy::simulated(v,p).map_err(|e|e.to_string()),
        (v,p) if crate::atk::supported(v,p)=>crate::atk::simulated(product).map_err(|e|e.to_string()),
        (v,p) if crate::hyperx::supported(v,p)=>crate::hyperx::simulated(product).map_err(|e|e.to_string()),
        (0x1532, 0x0084) => Ok(gearwise_razer::simulated_deathadder_v2()),
        (0x1532, 0x007a|0x007b|0x00a5|0x00a6|0x00c0|0x00c1) => gearwise_razer::simulated_viper(product).map_err(|e|e.to_string()),
        (0x1532, 0x00b4) => Ok(crate::naga::simulated()),
        (0x1b1c, 0x1b2e) => Ok(gearwise_corsair::simulated_m65_pro()),
        (0x1038, 0x1720) => Ok(gearwise_steelseries::simulated_rival310()),
        _ => Err("未支持该模拟型号".into()),
    }
}
pub fn templates() -> Result<Vec<View>, String> {
    crate::legacy::models().iter().map(|m|(m.vendor,m.product)).chain(crate::atk::models().iter().map(|m|(m.vendor,m.product))).chain([(0x03f0,0x048e),(0x03f0,0x028e),(0x1532, 0x0084), (0x1532, 0x00b4), (0x1b1c, 0x1b2e), (0x1038, 0x1720),(0x1532,0x007a),(0x1532,0x007b),(0x1532,0x00a5),(0x1532,0x00a6),(0x1532,0x00c0),(0x1532,0x00c1)].into_iter())
        .map(|(v, p)| {
            let (mut d, _) = simulated(v, p)?;
            let mut out = view(d.as_mut())?;
            out.descriptor.simulated = true;
            out.descriptor.id = format!("simulation:{}", out.descriptor.id);
            Ok(out)
        })
        .collect()
}
pub fn simulate(plan: &Plan) -> Result<serde_json::Value, String> {
    validate(plan)?;
    let (mut device, log) = simulated(plan.vendor, plan.product)?;
    let mut compiled=compile(plan)?;
    resolve_defaults(&mut compiled,&view(device.as_mut())?)?;
    let native_plan=Plan{vendor:plan.vendor,product:plan.product,settings:compiled.settings.clone(),macros:vec![],source:None};
    validate_capabilities(&native_plan, device.capabilities())?;
    log.lock().map_err(|_| "Mock log lock")?.clear();
    if native_macros(plan.vendor,plan.product){device.set_macro_definitions(&plan.macros).map_err(|e|e.to_string())?;}
    for s in &compiled.settings {
        device.apply(s).map_err(|e| e.to_string())?;
    }
    let source=crate::host_macros::Source{vendor:plan.vendor,product:plan.product,path:"simulation:driver-plan".into()};
    let mut software_traces=Vec::new();
    for binding in &compiled.bindings{software_traces.push(crate::host_macros::simulate(source.clone(),binding.button,binding.action.clone())?);}
    let original=device.read_state().map_err(|e|e.to_string())?.dpi;let saved=Mutex::new(None);
    let mut dpi_traces=Vec::new();
    for (button,action) in &compiled.dpi_actions{software_dpi(device.as_mut(),action,true,&saved)?;let pressed=device.read_state().map_err(|e|e.to_string())?.dpi;software_dpi(device.as_mut(),action,false,&saved)?;dpi_traces.push(serde_json::json!({"button":button,"action":action,"pressed":pressed,"released":device.read_state().map_err(|e|e.to_string())?.dpi}));if let Some(original)=&original{device.apply(&Setting::Dpi(original.clone())).map_err(|e|e.to_string())?;}}
    if compiled.software_input{device.stop_software_actions().map_err(|e|e.to_string())?;}
    let ops = log.lock().map_err(|_| "Mock log lock")?;
    let packets = ops
        .iter()
        .filter_map(|r| match r {
            gearwise_core::transport::MockRecord::Write(p)
            | gearwise_core::transport::MockRecord::SetFeature(p) => Some(
                p.iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            _ => None,
        })
        .collect::<Vec<_>>();
    if packets.is_empty() {
        return Err("模拟驱动没有产生报文".into());
    }
    Ok(
        serde_json::json!({"simulated":true,"hardware_tested":false,"packet_count":packets.len(),"packets":packets,"software_macros":software_traces,"dpi_actions":dpi_traces,"software_mode_restored":compiled.software_input,"note":"运行真实驱动代码与内存 HID 传输，不访问 USB；无法证明实机兼容性。"}),
    )
}
#[cfg(test)]
mod simulation_tests {
    use super::*;
    use gearwise_core::model::{DpiStage, DpiState};
    #[test]
    fn every_exact_model_runs_its_real_driver_over_memory_transport() {
        for (v, p) in [(0x1532, 0x0084), (0x1532, 0x00b4), (0x1b1c, 0x1b2e), (0x1038, 0x1720)] {
            let result = simulate(&Plan { macros:vec![], source:None,
                vendor: v,
                product: p,
                settings: vec![
                    Setting::Dpi(DpiState {
                        stages: vec![DpiStage::uniform(1600)],
                        active: 0,
                    }),
                    Setting::ReportRate(500),
                ],
            })
            .unwrap();
            assert!(result["packet_count"].as_u64().unwrap() > 0);
            assert_eq!(result["hardware_tested"], false);
        }
    }
}


pub struct Compiled {
 pub settings:Vec<Setting>, pub bindings:Vec<crate::host_macros::Binding>,
 pub input:crate::macro_runtime::InputMap, pub software_input:bool,
 pub dpi_actions:std::collections::BTreeMap<u16,DpiFunction>,
}
fn native_macros(v:u16,p:u16)->bool{crate::legacy::native_macros(v,p)||crate::atk::model(v,p).is_some_and(|m|m.native_macros)||crate::hyperx::supported(v,p)}
fn validate_viper_button(id:u16,action:&ButtonAction)->Result<(),String>{
 if matches!(id,9|10|0x60){return if matches!(action,ButtonAction::Default|ButtonAction::Disabled){Ok(())}else{Err("蝰蛇滚轮方向与底部 DPI 键目前只支持启用原动作或禁用".into())};}
 if !matches!(id,2|4|5){return Err("蝰蛇 V3 Pro 仅开放已核对的右键、后退、前进和滚轮/DPI 控制；保留左键".into());}
 if !matches!(action,ButtonAction::Default|ButtonAction::Disabled|ButtonAction::Mouse(gearwise_core::model::MouseButton::Left|gearwise_core::model::MouseButton::Right|gearwise_core::model::MouseButton::Back|gearwise_core::model::MouseButton::Forward)){return Err("蝰蛇 V3 Pro 键盘和宏协议仍待验证".into());}Ok(())
}
pub fn compile(plan:&Plan)->Result<Compiled,String>{
 use gearwise_core::profile::{Macro,MacroMode,MacroStep};
 let corsair=(plan.vendor,plan.product)==(0x1b1c,0x1b2e);
 let software_input=corsair&&plan.settings.iter().any(|s|matches!(s,Setting::Button{action,..} if !matches!(action,ButtonAction::Default)));
 let mut native=Vec::new();let mut bindings=Vec::new();let mut keys=std::collections::BTreeMap::new();let mut buttons=Vec::new();let mut dpi_actions=std::collections::BTreeMap::new();
 let mut definitions=std::collections::HashMap::new();if plan.macros.len()>32{return Err("最多保存 32 个软件宏".into());}
 if crate::atk::model(plan.vendor,plan.product).is_some_and(|m|!m.native_macros)&&!plan.macros.is_empty(){return Err("Pulsar 板载宏协议尚未核对".into());}
 for m in &plan.macros{crate::host_macros::validate(m)?;if crate::legacy::supported(plan.vendor,plan.product){crate::legacy::validate_macro(plan.vendor,plan.product,m).map_err(|e|e.to_string())?;}if crate::hyperx::supported(plan.vendor,plan.product){crate::hyperx::validate_macro(m).map_err(|e|e.to_string())?;}if crate::atk::supported(plan.vendor,plan.product){crate::atk::validate_macro(m).map_err(|e|e.to_string())?;}if definitions.insert(m.id.as_str(),m).is_some(){return Err("宏标识重复".into());}}
 if let Some(source)=&plan.source{if source.vendor!=plan.vendor||source.product!=plan.product{return Err("软件宏来源与方案品牌/型号不一致".into());}}
 for s in &plan.settings{
  if corsair {if let Setting::Button{id,action}=s{
   if *id==1||!(2..=8).contains(id){return Err("保留左键，且只允许 M65 PRO 已知按键".into());}
   let steps=match action{
    ButtonAction::Key{usage,mods}=>{let mut steps=Vec::new();let modifiers=(0..8).filter(|n|mods.0&(1<<n)!=0).map(|n|0xe0+n).collect::<Vec<_>>();for m in &modifiers{steps.push(MacroStep::KeyDown(*m));}steps.push(MacroStep::KeyDown(*usage));steps.push(MacroStep::WaitForRelease);steps.push(MacroStep::KeyUp(*usage));for m in modifiers.iter().rev(){steps.push(MacroStep::KeyUp(*m));}Some(steps)},
    ButtonAction::Mouse(b)=>Some(vec![MacroStep::MouseDown(*b),MacroStep::WaitForRelease,MacroStep::MouseUp(*b)]),
    ButtonAction::Macro(macro_id)=>{let m=definitions.get(macro_id.as_str()).ok_or("按键引用的宏内容缺失")?;bindings.push(crate::host_macros::Binding{button:*id,action:(*m).clone()});buttons.push(*id);None},
    ButtonAction::Dpi(f)=>{dpi_actions.insert(*id,f.clone());None},
    ButtonAction::Default|ButtonAction::Disabled=>None,
    _=>return Err("M65 PRO 尚不支持此动作".into()),
   };
   if let Some(steps)=steps{let action=Macro{id:format!("openhub-remap-{id}"),name:format!("按住按键 {id}"),mode:MacroMode::WhileHeld,steps};crate::host_macros::validate(&action)?;bindings.push(crate::host_macros::Binding{button:*id,action});buttons.push(*id);}
   native.push(s.clone());continue;
  }}
  if plan.vendor==0x1532&&matches!(plan.product,0x00c0|0x00c1){if let Setting::Button{id,action}=s{validate_viper_button(*id,action)?;native.push(s.clone());continue;}}
  if let Some(model)=crate::legacy::model(plan.vendor,plan.product){crate::legacy::validate_setting(model,s).map_err(|e|e.to_string())?;if let Setting::Button{action:ButtonAction::Macro(id),..}=s{crate::legacy::validate_macro(plan.vendor,plan.product,definitions.get(id.as_str()).ok_or("板载宏内容缺失")?).map_err(|e|e.to_string())?;}native.push(s.clone());continue;}
  if crate::hyperx::supported(plan.vendor,plan.product){crate::hyperx::validate_setting(s).map_err(|e|e.to_string())?;if let Setting::Button{action:ButtonAction::Macro(id),..}=s{crate::hyperx::validate_macro(definitions.get(id.as_str()).ok_or("HyperX 板载宏定义缺失")?).map_err(|e|e.to_string())?;}native.push(s.clone());continue;}
  if let Some(model)=crate::atk::model(plan.vendor,plan.product) {if let Setting::Button{action:ButtonAction::Key{usage,mods},..}=s{if !model.native_macros{return Err("Pulsar 键盘格式仍待验证".into());}crate::atk::validate_key(*usage,*mods).map_err(|e|e.to_string())?;}if let Setting::Button{action:ButtonAction::Macro(id),..}=s{if !model.native_macros{return Err("Pulsar 板载宏格式仍待验证".into());}let definition=definitions.get(id.as_str()).ok_or("ATK 板载宏定义缺失")?;crate::atk::validate_macro(definition).map_err(|e|e.to_string())?;}native.push(s.clone());continue;}
  if let Setting::Button{id,action:ButtonAction::Macro(macro_id)}=s{
   if !matches!((plan.vendor,plan.product),(0x1532,0x00b4)|(0x1038,0x1720)){return Err("该型号的软件宏输入尚未接通".into());}
   if *id==1{return Err("保留左键原始点击".into());}
   let m=definitions.get(macro_id.as_str()).ok_or("按键引用的宏内容缺失")?;
   if bindings.len()>=12{return Err("同时最多绑定 12 个软件宏按钮（F13–F24）".into());}
   let usage=0x68+bindings.len() as u8;let vk=0x7c+bindings.len() as u16;
   keys.insert(vk,*id);native.push(Setting::Button{id:*id,action:ButtonAction::Key{usage,mods:gearwise_core::model::Modifiers(0)}});bindings.push(crate::host_macros::Binding{button:*id,action:(*m).clone()});
  }else{native.push(s.clone());}
 }
 if software_input{for (id,function) in [(6,DpiFunction::Up),(7,DpiFunction::Down),(8,DpiFunction::Shift(0))]{if !plan.settings.iter().any(|s|matches!(s,Setting::Button{id:b,action} if *b==id&&!matches!(action,ButtonAction::Default))){dpi_actions.insert(id,function);}}for id in dpi_actions.keys(){buttons.push(*id);}}
 if !keys.is_empty(){let reserved=keys.keys().map(|v|0x68+(*v-0x7c) as u8).collect::<Vec<_>>();for s in &plan.settings{if let Setting::Button{action:ButtonAction::Key{usage,..},..}=s{if reserved.contains(usage){return Err("普通快捷键与软件宏的 F13–F24 触发键冲突".into());}}}}
 Ok(Compiled{settings:native,bindings,input:crate::macro_runtime::InputMap{keys,buttons,dpi_buttons:dpi_actions.keys().copied().collect()},software_input,dpi_actions})
}#[cfg(test)]mod macro_compile_tests{
 use super::*;use gearwise_core::{profile::{Macro,MacroMode,MacroStep},model::Modifiers};
 fn plan()->Plan{Plan{vendor:0x1038,product:0x1720,settings:vec![Setting::Button{id:6,action:ButtonAction::Macro("m".into())}],macros:vec![Macro{id:"m".into(),name:"B".into(),mode:MacroMode::Once,steps:vec![MacroStep::KeyDown(5),MacroStep::KeyUp(5)]}],source:None}}
 #[test]fn macro_compiles_to_reserved_native_key_and_exact_source_map(){let c=compile(&plan()).unwrap();assert_eq!(c.settings,vec![Setting::Button{id:6,action:ButtonAction::Key{usage:0x68,mods:Modifiers(0)}}]);assert_eq!(c.input.keys[&0x7c],6);assert_eq!(c.bindings[0].action.steps.len(),2);}
 #[test]fn incomplete_or_conflicting_macro_is_rejected_before_io(){let mut p=plan();p.macros.clear();assert!(compile(&p).is_err());p=plan();p.settings.push(Setting::Button{id:5,action:ButtonAction::Key{usage:0x68,mods:Modifiers(0)}});assert!(compile(&p).is_err());p=plan();p.source=Some(crate::host_macros::Source{vendor:0x046d,product:0x1720,path:"wrong".into()});assert!(compile(&p).is_err());}
}

// Caller holds the manager lock. The saved state is retained until the write
// succeeds, so a failed release can be retried by stop_software().
fn software_dpi(device:&mut dyn gearwise_core::Device,action:&DpiFunction,down:bool,saved:&Mutex<Option<gearwise_core::model::DpiState>>)->Result<(),String>{
 software_dpi_journal(device,action,down,saved,None)
}
fn software_dpi_journal(device:&mut dyn gearwise_core::Device,action:&DpiFunction,down:bool,saved:&Mutex<Option<gearwise_core::model::DpiState>>,journal:Option<&crate::macro_recovery::Journal>)->Result<(),String>{
 use gearwise_core::model::DpiStage;
 if (device.descriptor().vendor_id,device.descriptor().product_id)!=(0x1b1c,0x1b2e){return Err("软件 DPI 仅用于 M65 PRO".into());}
 let mut shift=saved.lock().map_err(|_|"狙击 DPI 锁异常")?;
 if let DpiFunction::Shift(value)=action{
  if down{if shift.is_some(){return Ok(());}if !(100..=12000).contains(value){return Err("狙击 DPI 超出范围".into());}let original=device.read_state().map_err(|e|e.to_string())?.dpi.ok_or("没有 DPI 状态")?;let mut next=original.clone();let stage=next.stages.get_mut(next.active as usize).ok_or("活动档位无效")?;*stage=DpiStage::uniform(*value);if let Some(j)=journal{j.shift(Some(crate::macro_recovery::ShiftRestore{original:original.clone(),expected:next.clone()}))?;}*shift=Some(original);device.apply(&Setting::Dpi(next)).map_err(|e|e.to_string())?;}
  else if let Some(original)=shift.as_ref(){if let Some(j)=journal{if let Some(owned)=j.record()?.shift{let actual=device.read_state().map_err(|e|e.to_string())?.dpi;if actual.as_ref()!=Some(&owned.expected)&&actual.as_ref()!=Some(original){j.shift(None)?;*shift=None;return Err("本次会话的 DPI 已改变，未覆盖当前设置".into());}}}device.apply(&Setting::Dpi(original.clone())).map_err(|e|e.to_string())?;if let Some(j)=journal{j.shift(None)?;}*shift=None;}
  return Ok(());
 }
 if !down{return Ok(());}if shift.is_some(){return Err("狙击键按住时暂不切换档位".into());}
 let dpi=device.read_state().map_err(|e|e.to_string())?.dpi.ok_or("没有 DPI 状态")?;let count=dpi.stages.len();if count==0||usize::from(dpi.active)>=count{return Err("DPI 档位无效".into());}
 let active=usize::from(dpi.active);let target=match action{DpiFunction::Up=>(active+1).min(count-1),DpiFunction::Down=>active.saturating_sub(1),DpiFunction::Cycle=>(active+1)%count,_=>unreachable!()};
 if target!=active{device.apply(&Setting::DpiActiveStage(target as u8)).map_err(|e|e.to_string())?;}Ok(())
}
#[cfg(test)]mod corsair_action_tests{
 use super::*;use gearwise_core::{model::{DpiStage,DpiState,Modifiers},profile::{Macro,MacroMode,MacroStep}};
 fn plan(settings:Vec<Setting>)->Plan{Plan{vendor:0x1b1c,product:0x1b2e,settings,macros:vec![],source:None}}
 #[test]fn simple_remap_preserves_default_dpi_controls(){let p=plan(vec![Setting::Button{id:4,action:ButtonAction::Key{usage:4,mods:Modifiers(1)}}]);validate(&p).unwrap();let c=compile(&p).unwrap();assert!(c.software_input);assert_eq!(c.input.buttons,vec![4,6,7,8]);assert_eq!(c.dpi_actions[&6],DpiFunction::Up);assert_eq!(c.dpi_actions[&7],DpiFunction::Down);assert_eq!(c.dpi_actions[&8],DpiFunction::Shift(0));assert_eq!(c.bindings[0].action.mode,MacroMode::WhileHeld);assert_eq!(c.bindings[0].action.steps,vec![MacroStep::KeyDown(0xe0),MacroStep::KeyDown(4),MacroStep::WaitForRelease,MacroStep::KeyUp(4),MacroStep::KeyUp(0xe0)]);}
 #[test]fn macro_on_sniper_replaces_only_its_default_action(){let mut p=plan(vec![Setting::Button{id:8,action:ButtonAction::Macro("m".into())}]);p.macros.push(Macro{id:"m".into(),name:"test".into(),mode:MacroMode::Once,steps:vec![MacroStep::KeyDown(5),MacroStep::KeyUp(5)]});validate(&p).unwrap();let c=compile(&p).unwrap();assert!(!c.dpi_actions.contains_key(&8));assert!(c.input.buttons.contains(&8));assert_eq!(c.bindings[0].button,8);assert!(c.input.keys.is_empty());}
 #[test]fn unchanged_defaults_do_not_take_over_input(){let p=plan(vec![Setting::Button{id:4,action:ButtonAction::Default}]);let c=compile(&p).unwrap();assert!(!c.software_input);assert!(c.bindings.is_empty()&&c.dpi_actions.is_empty());}
 #[test]fn dpi_step_cycle_and_sniper_restore_execute_actual_driver(){let(mut d,log)=gearwise_corsair::simulated_m65_pro();let original=DpiState{stages:vec![DpiStage{x:800,y:900},DpiStage{x:1600,y:1700}],active:0};d.apply(&Setting::Dpi(original.clone())).unwrap();let saved=Mutex::new(None);software_dpi(d.as_mut(),&DpiFunction::Up,true,&saved).unwrap();assert_eq!(d.read_state().unwrap().dpi.unwrap().active,1);software_dpi(d.as_mut(),&DpiFunction::Cycle,true,&saved).unwrap();assert_eq!(d.read_state().unwrap().dpi.unwrap(),original);software_dpi(d.as_mut(),&DpiFunction::Shift(400),true,&saved).unwrap();assert_eq!(d.read_state().unwrap().dpi.unwrap().current(),Some(DpiStage::uniform(400)));assert!(software_dpi(d.as_mut(),&DpiFunction::Up,true,&saved).is_err());software_dpi(d.as_mut(),&DpiFunction::Shift(400),false,&saved).unwrap();assert_eq!(d.read_state().unwrap().dpi.unwrap(),original);assert!(saved.lock().unwrap().is_none());assert!(!log.lock().unwrap().is_empty());}
 #[test]fn software_button_mode_can_restore_hardware_routing(){let(mut d,log)=gearwise_corsair::simulated_m65_pro();log.lock().unwrap().clear();d.apply(&Setting::Button{id:4,action:ButtonAction::Disabled}).unwrap();d.stop_software_actions().unwrap();let packets=log.lock().unwrap().iter().filter_map(|r|if let gearwise_core::transport::MockRecord::Write(p)=r{Some(p.clone())}else{None}).collect::<Vec<_>>();assert!(packets.iter().any(|p|p.windows(3).any(|b|b==[7,4,2])));assert!(packets.iter().any(|p|p.windows(3).any(|b|b==[7,4,1])));assert!(packets.iter().any(|p|p.windows(6).any(|b|b==[7,0x40,20,0,1,0x80])));}
}
fn resolve_defaults(compiled:&mut Compiled,before:&View)->Result<(),String>{
 if compiled.dpi_actions.get(&8)==Some(&DpiFunction::Shift(0)){let sniper=before.state.buttons.get(&8).or_else(||before.capabilities.buttons.iter().find(|b|b.id==8).map(|b|&b.default_action));let Some(ButtonAction::Dpi(DpiFunction::Shift(dpi)))=sniper else{return Err("无法读取 M65 PRO 原始狙击 DPI；请先显式配置狙击键动作".into())};if !(100..=12000).contains(dpi){return Err("读取到的狙击 DPI 超出范围".into());}compiled.dpi_actions.insert(8,DpiFunction::Shift(*dpi));}Ok(())
}
#[cfg(test)]mod complete_simulation_tests{
 use super::*;use gearwise_core::model::Modifiers;
 #[test]fn corsair_plan_runs_remap_hold_dpi_and_hardware_restore(){let p=Plan{vendor:0x1b1c,product:0x1b2e,settings:vec![Setting::Button{id:4,action:ButtonAction::Key{usage:5,mods:Modifiers(1)}}],macros:vec![],source:None};let result=simulate(&p).unwrap();assert_eq!(result["software_mode_restored"],true);assert_eq!(result["software_macros"][0]["events"],serde_json::json!(["key_down:224","key_down:5","key_up:5","key_up:224"]));assert_eq!(result["dpi_actions"].as_array().unwrap().len(),3);let shift=result["dpi_actions"].as_array().unwrap().iter().find(|a|a["button"]==8).unwrap();assert_eq!(shift["action"],serde_json::json!({"Shift":400}));assert_eq!(result["hardware_tested"],false);}
 #[test]fn unknown_original_sniper_is_rejected_before_driver_input_mode(){let(mut d,_)=gearwise_corsair::simulated_m65_pro();let mut before=view(d.as_mut()).unwrap();before.state.buttons.remove(&8);before.capabilities.buttons.iter_mut().filter(|b|b.id==8).for_each(|b|b.default_action=ButtonAction::Default);let p=Plan{vendor:0x1b1c,product:0x1b2e,settings:vec![Setting::Button{id:4,action:ButtonAction::Disabled}],macros:vec![],source:None};assert!(resolve_defaults(&mut compile(&p).unwrap(),&before).is_err());}
}
#[cfg(test)]mod ownership_restore_tests{
 use super::*;use gearwise_core::transport::MockBackend;use gearwise_core::model::Modifiers;
 fn scenario(current:u8)->(Service,DeviceManager,String){let(mut device,_)=crate::naga::simulated();let id=device.descriptor().id.clone();device.apply(&Setting::Button{id:0x40,action:ButtonAction::Key{usage:current,mods:Modifiers(0)}}).unwrap();let mut manager=DeviceManager::new(Box::new(MockBackend::new(vec![],|_|Err(gearwise_core::Error::Unsupported("no physical HID")))),vec![]);manager.add_virtual(vec![device]);let service=Service::default();service.1.lock().unwrap().insert(id.clone(),Cleanup{source:crate::host_macros::Source{vendor:0x1532,product:0x00b4,path:"simulation:owned".into()},settings:vec![Setting::Button{id:0x40,action:ButtonAction::Key{usage:4,mods:Modifiers(0)}}],expected:std::collections::HashMap::from([(0x40,ButtonAction::Key{usage:0x68,mods:Modifiers(0)})]),software_input:false,cancelled:Arc::new(AtomicBool::new(false)),shift:Arc::new(Mutex::new(None)),journal:None,serial:None});(service,manager,id)}
 #[test]fn own_trigger_is_restored_and_already_restored_binding_is_preserved(){for current in [0x68,4]{let(service,mut manager,id)=scenario(current);service.restore_one(&mut manager,&id,&crate::macro_runtime::Runtime::default()).unwrap();assert_eq!(manager.device_mut(&id).unwrap().read_state().unwrap().buttons[&0x40],ButtonAction::Key{usage:4,mods:Modifiers(0)});assert!(service.1.lock().unwrap().is_empty());}}
 #[test]fn external_controller_change_is_not_overwritten_during_macro_stop(){let(service,mut manager,id)=scenario(5);let error=service.restore_one(&mut manager,&id,&crate::macro_runtime::Runtime::default()).unwrap_err();assert!(error.contains("未覆盖外部设置"));assert_eq!(manager.device_mut(&id).unwrap().read_state().unwrap().buttons[&0x40],ButtonAction::Key{usage:5,mods:Modifiers(0)});assert!(service.1.lock().unwrap().is_empty());}
}

#[cfg(test)] mod durable_shift_tests {
 use super::*;
 use crate::macro_recovery::{Journal,Record};
 use gearwise_core::model::{DpiStage,DpiState};
 fn journal()->(std::path::PathBuf,Journal){
  let dir=std::env::temp_dir().join(format!("shift-journal-{}",crate::profiles::new_id()));
  let journal=Journal::create(&dir,Record{version:1,device_id:"corsair:a".into(),serial:Some("unit-a".into()),source:crate::host_macros::Source{vendor:0x1b1c,product:0x1b2e,path:"input:a".into()},settings:vec![],expected:Default::default(),software_input:true,shift:None}).unwrap();(dir,journal)
 }
 #[test]fn sniper_restoration_survives_restart_with_original_xy_and_active_stage(){
  let(dir,journal)=journal();let(mut device,_)=gearwise_corsair::simulated_m65_pro();
  let original=DpiState{stages:vec![DpiStage{x:800,y:900},DpiStage{x:1600,y:1700}],active:1};device.apply(&Setting::Dpi(original.clone())).unwrap();
  let saved=Mutex::new(None);software_dpi_journal(device.as_mut(),&DpiFunction::Shift(400),true,&saved,Some(&journal)).unwrap();
  let(loaded,errors)=crate::macro_recovery::load(&dir);assert!(errors.is_empty());let persisted=loaded[0].record().unwrap().shift.unwrap();assert_eq!(persisted.original,original);assert_eq!(device.read_state().unwrap().dpi,Some(persisted.expected));
  let restarted=Mutex::new(Some(persisted.original));software_dpi_journal(device.as_mut(),&DpiFunction::Shift(400),false,&restarted,Some(&loaded[0])).unwrap();assert_eq!(device.read_state().unwrap().dpi,Some(original));assert!(loaded[0].record().unwrap().shift.is_none());
  loaded[0].finish().unwrap();std::fs::remove_dir(dir).unwrap();
 }
 #[test]fn failed_write_ahead_record_prevents_temporary_hardware_write(){
  let(dir,journal)=journal();journal.finish().unwrap();std::fs::remove_dir(&dir).unwrap();let(mut device,log)=gearwise_corsair::simulated_m65_pro();log.lock().unwrap().clear();
  assert!(software_dpi_journal(device.as_mut(),&DpiFunction::Shift(400),true,&Mutex::new(None),Some(&journal)).is_err());
  let writes=log.lock().unwrap().iter().filter(|r|matches!(r,gearwise_core::transport::MockRecord::Write(_))).count();assert_eq!(writes,0);
 }
 #[test]fn releasing_sniper_does_not_overwrite_a_changed_session_dpi_cache(){
  let(dir,journal)=journal();let(mut device,_)=gearwise_corsair::simulated_m65_pro();let saved=Mutex::new(None);software_dpi_journal(device.as_mut(),&DpiFunction::Shift(400),true,&saved,Some(&journal)).unwrap();
  let external=DpiState{stages:vec![DpiStage::uniform(2400)],active:0};device.apply(&Setting::Dpi(external.clone())).unwrap();let error=software_dpi_journal(device.as_mut(),&DpiFunction::Shift(400),false,&saved,Some(&journal)).unwrap_err();assert!(error.contains("未覆盖"));assert_eq!(device.read_state().unwrap().dpi,Some(external));assert!(saved.lock().unwrap().is_none());journal.finish().unwrap();std::fs::remove_dir(dir).unwrap();
 }
}

#[cfg(test)] mod restart_recovery_tests {
 use super::*;
 use gearwise_core::{Device,transport::MockBackend,model::Modifiers};
 struct Identified{inner:Box<dyn Device>,descriptor:DeviceDescriptor}
 impl Device for Identified{
  fn descriptor(&self)->&DeviceDescriptor{&self.descriptor}
  fn capabilities(&self)->&Capabilities{self.inner.capabilities()}
  fn read_state(&mut self)->gearwise_core::Result<DeviceState>{self.inner.read_state()}
  fn apply(&mut self,s:&Setting)->gearwise_core::Result<()>{self.inner.apply(s)}
 }
 fn key(usage:u8)->ButtonAction{ButtonAction::Key{usage,mods:Modifiers(0)}}
 fn setup(serial:Option<&str>,current:u8,count:usize)->(Service,std::path::PathBuf){
  let dir=std::env::temp_dir().join(format!("restart-journal-{}",crate::profiles::new_id()));
  crate::macro_recovery::Journal::create(&dir,crate::macro_recovery::Record{version:1,device_id:"old:disconnected-path".into(),serial:Some("unit-a".into()),source:crate::host_macros::Source{vendor:0x1532,product:0x00b4,path:"old:input-path".into()},settings:vec![Setting::Button{id:0x40,action:key(4)}],expected:std::collections::HashMap::from([(0x40,key(0x68))]),software_input:false,shift:None}).unwrap();
  let mut manager=DeviceManager::new(Box::new(MockBackend::new(vec![],|_|Err(gearwise_core::Error::Unsupported("no physical HID")))),vec![]);
  for n in 0..count{let(mut inner,_)=crate::naga::simulated();inner.apply(&Setting::Button{id:0x40,action:key(current)}).unwrap();let mut descriptor=inner.descriptor().clone();descriptor.id=format!("connected:new-path:{n}");descriptor.serial=serial.map(str::to_owned);manager.add_virtual(vec![Box::new(Identified{inner,descriptor})]);}
  let service=Service::default();*service.0.lock().unwrap()=Some(manager);(service,dir)
 }
 fn current(service:&Service)->ButtonAction{service.with(|m|Ok(m.device_mut("connected:new-path:0").unwrap().read_state().unwrap().buttons[&0x40].clone())).unwrap()}
 #[derive(Clone,Default)]struct Rec(Arc<Mutex<Vec<String>>>);
 impl gearwise_input::InputSink for Rec{
  fn key_down(&mut self,k:u8)->gearwise_core::Result<()>{self.0.lock().unwrap().push(format!("d{k}"));Ok(())}
  fn key_up(&mut self,k:u8)->gearwise_core::Result<()>{self.0.lock().unwrap().push(format!("u{k}"));Ok(())}
  fn mouse_down(&mut self,_:gearwise_core::model::MouseButton)->gearwise_core::Result<()>{Ok(())}fn mouse_up(&mut self,_:gearwise_core::model::MouseButton)->gearwise_core::Result<()>{Ok(())}fn scroll(&mut self,_:i32,_:i32)->gearwise_core::Result<()>{Ok(())}fn media(&mut self,_:u16)->gearwise_core::Result<()>{Ok(())}
 }
 fn wait(rec:&Rec,n:usize){let end=std::time::Instant::now()+std::time::Duration::from_secs(1);while rec.0.lock().unwrap().len()<n&&std::time::Instant::now()<end{std::thread::sleep(std::time::Duration::from_millis(1));}assert_eq!(rec.0.lock().unwrap().len(),n);}
 fn held_naga()->(Service,std::path::PathBuf,crate::macro_runtime::Runtime,Rec,crate::host_macros::Source){
  use gearwise_core::profile::{Macro,MacroMode,MacroStep};let(service,dir)=setup(Some("unit-a"),0x68,1);let source=crate::macro_recovery::load(&dir).0[0].record().unwrap().source;let rec=Rec::default();let runtime=crate::macro_runtime::Runtime::simulation(Arc::new(Mutex::new(rec.clone())));
  runtime.arm(source.clone(),vec![crate::host_macros::Binding{button:0x40,action:Macro{id:"owned".into(),name:"hold".into(),mode:MacroMode::WhileHeld,steps:vec![MacroStep::KeyDown(4),MacroStep::Delay(5000),MacroStep::KeyUp(4)]}}],crate::macro_runtime::InputMap{keys:std::collections::BTreeMap::from([(0x7c,0x40)]),buttons:vec![],dpi_buttons:vec![]},crate::macro_runtime::Hooks::default()).unwrap();runtime.simulate_event(crate::raw_input::Event::Key{source:source.clone(),vkey:0x7c,down:true});wait(&rec,1);
  let issues=service.recover_from(&dir,&runtime).unwrap();assert!(issues.iter().any(|s|s.contains("触发键")));assert_eq!(current(&service),key(0x68));wait(&rec,2);assert_eq!(runtime.status().source_count,0);assert!(runtime.has_quiescing());(service,dir,runtime,rec,source)
 }
 #[test]fn owned_hid_mapping_restores_only_after_its_physical_key_up(){let(service,dir,runtime,rec,source)=held_naga();let mut other=source.clone();other.path.push_str("-other");runtime.simulate_event(crate::raw_input::Event::Key{source:other,vkey:0x7c,down:false});assert!(!service.recover_from(&dir,&runtime).unwrap().is_empty());assert_eq!(current(&service),key(0x68));runtime.simulate_event(crate::raw_input::Event::Key{source,vkey:0x7c,down:false});assert!(service.recover_from(&dir,&runtime).unwrap().is_empty());assert_eq!(current(&service),key(4));assert!(crate::macro_recovery::load(&dir).0.is_empty());assert!(!runtime.has_quiescing());assert_eq!(*rec.0.lock().unwrap(),vec!["d4","u4"]);runtime.shutdown();std::fs::remove_dir(dir).unwrap();}
 #[test]fn external_mapping_changed_during_release_wait_remains_untouched(){let(service,dir,runtime,_,source)=held_naga();service.with(|m|{m.device_mut("connected:new-path:0").unwrap().apply(&Setting::Button{id:0x40,action:key(5)}).unwrap();Ok(())}).unwrap();runtime.simulate_event(crate::raw_input::Event::Key{source,vkey:0x7c,down:false});let issues=service.recover_from(&dir,&runtime).unwrap();assert!(issues.iter().any(|s|s.contains("未覆盖外部设置")));assert_eq!(current(&service),key(5));assert!(crate::macro_recovery::load(&dir).0.is_empty());runtime.shutdown();std::fs::remove_dir(dir).unwrap();}
 #[test]fn changed_serial_during_release_wait_never_receives_restore_writes(){let(service,dir,runtime,_,source)=held_naga();let(other,other_dir)=setup(Some("unit-b"),0x68,1);*service.0.lock().unwrap()=other.0.lock().unwrap().take();runtime.simulate_event(crate::raw_input::Event::Key{source,vkey:0x7c,down:false});assert!(service.recover_from(&dir,&runtime).unwrap().iter().any(|s|s.contains("序列号")));assert_eq!(current(&service),key(0x68));assert!(!service.recover_from(&dir,&runtime).unwrap().is_empty());assert_eq!(current(&service),key(0x68));runtime.shutdown();for directory in [dir,other_dir]{for journal in crate::macro_recovery::load(&directory).0{journal.finish().unwrap();}std::fs::remove_dir(directory).unwrap();}}
 #[test]fn live_corsair_vendor_mode_waits_for_release_before_actual_hardware_mode_restore(){
  use gearwise_core::profile::{Macro,MacroMode,MacroStep};let(mut device,log)=gearwise_corsair::simulated_m65_pro();device.apply(&Setting::Button{id:4,action:ButtonAction::Disabled}).unwrap();let descriptor=device.descriptor().clone();let source=crate::host_macros::Source{vendor:0x1b1c,product:0x1b2e,path:"simulation:live-vendor".into()};let dir=std::env::temp_dir().join(format!("held-vendor-{}",crate::profiles::new_id()));let journal=Arc::new(crate::macro_recovery::Journal::create(&dir,crate::macro_recovery::Record{version:1,device_id:descriptor.id.clone(),serial:descriptor.serial.clone(),source:source.clone(),settings:vec![],expected:Default::default(),software_input:true,shift:None}).unwrap());
  let mut manager=DeviceManager::new(Box::new(MockBackend::new(vec![],|_|Err(gearwise_core::Error::Unsupported("no physical HID")))),vec![]);manager.add_virtual(vec![device]);let service=Service::default();*service.0.lock().unwrap()=Some(manager);service.1.lock().unwrap().insert(descriptor.id,Cleanup{source:source.clone(),settings:vec![],expected:Default::default(),software_input:true,cancelled:Arc::new(AtomicBool::new(false)),shift:Arc::new(Mutex::new(None)),journal:Some(journal.clone()),serial:descriptor.serial});
  let rec=Rec::default();let runtime=crate::macro_runtime::Runtime::simulation(Arc::new(Mutex::new(rec.clone())));runtime.arm(source.clone(),vec![crate::host_macros::Binding{button:4,action:Macro{id:"owned".into(),name:"once".into(),mode:MacroMode::Once,steps:vec![MacroStep::KeyDown(4),MacroStep::KeyUp(4)]}}],crate::macro_runtime::InputMap{keys:Default::default(),buttons:vec![4],dpi_buttons:vec![]},crate::macro_runtime::Hooks::default()).unwrap();runtime.simulate_event(crate::raw_input::Event::Buttons{source:source.clone(),mask:8});wait(&rec,2);log.lock().unwrap().clear();assert!(service.stop_software(&runtime).unwrap_err().contains("触发键"));assert!(runtime.has_quiescing());assert_eq!(runtime.status().source_count,0);assert!(log.lock().unwrap().is_empty());assert!(journal.path.exists());
  runtime.simulate_event(crate::raw_input::Event::Buttons{source,mask:0});wait_restore(&service,&journal.path);assert!(!journal.path.exists());assert!(!runtime.has_quiescing());assert!(log.lock().unwrap().iter().any(|r|matches!(r,gearwise_core::transport::MockRecord::Write(p) if p.windows(3).any(|v|v==[7,4,1]))));runtime.shutdown();std::fs::remove_dir(dir).unwrap();
 }
 fn wait_restore(service:&Service,path:&std::path::Path){let deadline=std::time::Instant::now()+std::time::Duration::from_secs(2);while (path.exists()||service.2.load(Ordering::Acquire))&&std::time::Instant::now()<deadline{std::thread::sleep(std::time::Duration::from_millis(2));}assert!(!path.exists());assert!(!service.2.load(Ordering::Acquire));}
 #[test]fn native_restore_worker_finishes_without_any_frontend_device_scan(){let(service,dir,runtime,rec,source)=held_naga();let path=crate::macro_recovery::load(&dir).0[0].path.clone();assert!(service.stop_software(&runtime).is_err());assert!(service.2.load(Ordering::Acquire));runtime.simulate_event(crate::raw_input::Event::Key{source,vkey:0x7c,down:false});wait_restore(&service,&path);assert_eq!(current(&service),key(4));assert_eq!(*rec.0.lock().unwrap(),vec!["d4","u4"]);assert_eq!(runtime.status().source_count,0);runtime.shutdown();std::fs::remove_dir(dir).unwrap();}
 #[test]fn restart_restores_owned_key_on_unique_serial_even_if_paths_changed(){
  let(service,dir)=setup(Some("unit-a"),0x68,1);assert!(service.recover_from(&dir,&crate::macro_runtime::Runtime::default()).unwrap().is_empty());assert_eq!(current(&service),key(4));assert!(crate::macro_recovery::load(&dir).0.is_empty());std::fs::remove_dir(dir).unwrap();
 }
 #[test]fn same_model_other_serial_missing_serial_or_duplicate_serial_never_gets_written(){
  for(serial,count) in [(Some("unit-b"),1),(None,1),(Some("unit-a"),2)]{let(service,dir)=setup(serial,0x68,count);assert!(!service.recover_from(&dir,&crate::macro_runtime::Runtime::default()).unwrap().is_empty());assert_eq!(current(&service),key(0x68));let(journals,_)=crate::macro_recovery::load(&dir);assert_eq!(journals.len(),1);journals[0].finish().unwrap();std::fs::remove_dir(dir).unwrap();}
 }
 #[test]fn external_binding_after_crash_is_preserved_and_old_record_is_finished(){
  let(service,dir)=setup(Some("unit-a"),5,1);let issues=service.recover_from(&dir,&crate::macro_runtime::Runtime::default()).unwrap();assert!(issues.iter().any(|s|s.contains("未覆盖外部设置")));assert_eq!(current(&service),key(5));assert!(crate::macro_recovery::load(&dir).0.is_empty());std::fs::remove_dir(dir).unwrap();
 }
 #[test]fn cancelled_cleanup_without_serial_is_not_automatically_written(){
  let(service,dir)=setup(None,0x68,1);let(mut journals,_)=crate::macro_recovery::load(&dir);let journal=Arc::new(journals.remove(0));let record=journal.record().unwrap();service.1.lock().unwrap().insert("connected:new-path:0".into(),Cleanup{source:record.source,settings:record.settings,expected:record.expected,software_input:false,cancelled:Arc::new(AtomicBool::new(true)),shift:Arc::new(Mutex::new(None)),journal:Some(journal.clone()),serial:None});
  assert!(!service.recover_from(&dir,&crate::macro_runtime::Runtime::default()).unwrap().is_empty());assert_eq!(current(&service),key(0x68));assert!(journal.path.exists());journal.finish().unwrap();std::fs::remove_dir(dir).unwrap();
 }
 #[test]fn cancelled_corsair_routing_is_not_reset_without_current_ownership_readback(){
  let(mut device,log)=gearwise_corsair::simulated_m65_pro();device.apply(&Setting::Button{id:4,action:ButtonAction::Disabled}).unwrap();let descriptor=device.descriptor().clone();let source=crate::host_macros::Source{vendor:0x1b1c,product:0x1b2e,path:"simulation:input".into()};let dir=std::env::temp_dir().join(format!("route-journal-{}",crate::profiles::new_id()));let journal=Arc::new(crate::macro_recovery::Journal::create(&dir,crate::macro_recovery::Record{version:1,device_id:descriptor.id.clone(),serial:descriptor.serial.clone(),source:source.clone(),settings:vec![],expected:Default::default(),software_input:true,shift:None}).unwrap());
  let mut manager=DeviceManager::new(Box::new(MockBackend::new(vec![],|_|Err(gearwise_core::Error::Unsupported("no physical HID")))),vec![]);manager.add_virtual(vec![device]);let service=Service::default();*service.0.lock().unwrap()=Some(manager);service.1.lock().unwrap().insert(descriptor.id,Cleanup{source,settings:vec![],expected:Default::default(),software_input:true,cancelled:Arc::new(AtomicBool::new(true)),shift:Arc::new(Mutex::new(None)),journal:Some(journal.clone()),serial:descriptor.serial});log.lock().unwrap().clear();
  assert!(!service.recover_from(&dir,&crate::macro_runtime::Runtime::default()).unwrap().is_empty());assert!(log.lock().unwrap().is_empty());assert!(journal.path.exists());journal.finish().unwrap();std::fs::remove_dir(dir).unwrap();
 }
 #[test]fn source_removal_discards_live_restore_authority_even_if_the_path_reappears(){
  let(mut device,log)=gearwise_corsair::simulated_m65_pro();device.apply(&Setting::Button{id:4,action:ButtonAction::Disabled}).unwrap();let descriptor=device.descriptor().clone();let source=crate::host_macros::Source{vendor:0x1b1c,product:0x1b2e,path:"simulation:removed-vendor".into()};let dir=std::env::temp_dir().join(format!("removed-vendor-{}",crate::profiles::new_id()));let journal=Arc::new(crate::macro_recovery::Journal::create(&dir,crate::macro_recovery::Record{version:1,device_id:descriptor.id.clone(),serial:None,source:source.clone(),settings:vec![],expected:Default::default(),software_input:true,shift:None}).unwrap());let mut manager=DeviceManager::new(Box::new(MockBackend::new(vec![],|_|Err(gearwise_core::Error::Unsupported("no physical HID")))),vec![]);manager.add_virtual(vec![device]);let service=Service::default();*service.0.lock().unwrap()=Some(manager);service.1.lock().unwrap().insert(descriptor.id,Cleanup{source:source.clone(),settings:vec![],expected:Default::default(),software_input:true,cancelled:Arc::new(AtomicBool::new(true)),shift:Arc::new(Mutex::new(None)),journal:Some(journal.clone()),serial:None});let runtime=crate::macro_runtime::Runtime::simulation(Arc::new(Mutex::new(Rec::default())));runtime.simulate_event(crate::raw_input::Event::Buttons{source:source.clone(),mask:8});assert!(!runtime.quiesce(&source,&[]).unwrap());assert!(runtime.has_quiescing());runtime.simulate_event(crate::raw_input::Event::Removed{path:source.path.clone()});assert!(!runtime.has_quiescing());runtime.simulate_event(crate::raw_input::Event::Buttons{source,mask:0});log.lock().unwrap().clear();assert!(!service.recover_from(&dir,&runtime).unwrap().is_empty());assert!(log.lock().unwrap().is_empty());assert!(journal.path.exists());runtime.shutdown();journal.finish().unwrap();std::fs::remove_dir(dir).unwrap();
 }
}

#[cfg(test)]mod atk_bridge_tests{
 use super::*;use gearwise_core::model::{DpiState,DpiStage,MouseButton};
 #[test]fn atk_templates_and_saved_plan_run_the_production_driver(){let all=templates().unwrap();for product in [0x1115,0x1017,0x10c9]{let view=all.iter().find(|v|v.descriptor.vendor_id==0x373b&&v.descriptor.product_id==product).unwrap();assert!(!view.hardware_tested);assert!(view.descriptor.simulated);let p=Plan{vendor:0x373b,product,macros:vec![],source:None,settings:vec![Setting::Dpi(DpiState{stages:vec![DpiStage{x:1200,y:1600}],active:0}),Setting::ReportRate(500),Setting::Button{id:4,action:ButtonAction::Mouse(MouseButton::Forward)}]};let encoded=serde_json::to_vec(&p).unwrap();let decoded:Plan=serde_json::from_slice(&encoded).unwrap();let result=simulate(&decoded).unwrap();assert!(result["packet_count"].as_u64().unwrap()>0);}}
}

#[cfg(test)]mod atk_native_plan_tests{
 use super::*;use gearwise_core::profile::{Macro,MacroMode,MacroStep};
 #[test]fn atk_plan_uses_native_macro_packets_instead_of_host_hooks(){let p=Plan{vendor:0x373b,product:0x1115,settings:vec![Setting::Button{id:4,action:ButtonAction::Macro("board".into())}],macros:vec![Macro{id:"board".into(),name:"board".into(),mode:MacroMode::WhileHeld,steps:vec![MacroStep::KeyDown(4),MacroStep::Delay(20),MacroStep::KeyUp(4)]}],source:None};let compiled=compile(&p).unwrap();assert!(compiled.bindings.is_empty());assert!(!compiled.software_input);assert!(matches!(compiled.settings[0],Setting::Button{action:ButtonAction::Macro(_),..}));let result=simulate(&p).unwrap();assert!(result["packet_count"].as_u64().unwrap()>10);assert!(result["software_macros"].as_array().unwrap().is_empty());}
}

#[cfg(test)]mod compx_integration_tests{
 use super::*;
 #[test]fn new_brands_native_plans_preserve_board_macros_through_save_and_simulation(){
  use gearwise_core::profile::{Macro,MacroMode,MacroStep};
  for model in crate::atk::models().iter().filter(|m|matches!(m.brand.as_str(),"vxe"|"vgn")){
   let p=Plan{vendor:model.vendor,product:model.product,source:None,settings:vec![Setting::Dpi(gearwise_core::model::DpiState{stages:vec![gearwise_core::model::DpiStage{x:1200,y:1600}],active:0}),Setting::Button{id:4,action:ButtonAction::Macro("native".into())}],macros:vec![Macro{id:"native".into(),name:"native".into(),mode:MacroMode::Toggle,steps:vec![MacroStep::KeyDown(4),MacroStep::Delay(10),MacroStep::KeyUp(4)]}]};
   let saved=serde_json::to_vec(&p).unwrap();let loaded:Plan=serde_json::from_slice(&saved).unwrap();assert_eq!(loaded,p);
   let compiled=compile(&loaded).unwrap();assert!(!compiled.software_input&&compiled.bindings.is_empty());
   let result=simulate(&loaded).unwrap();assert!(result["packet_count"].as_u64().unwrap()>10);assert!(result["software_macros"].as_array().unwrap().is_empty());
   if let Some(dir)=std::env::var_os("OPEN_HUB_COMPX_FIXTURE_DIR"){
    let (mut device,_)=simulated(model.vendor,model.product).unwrap();device.set_macro_definitions(&loaded.macros).unwrap();
    for setting in &compiled.settings{device.apply(setting).unwrap();}
    let dto=view(device.as_mut()).unwrap();let dir=std::path::PathBuf::from(dir);std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{:04x}-{:04x}.json",model.vendor,model.product)),serde_json::to_vec_pretty(&dto).unwrap()).unwrap();
   }

  }
 }
}

#[cfg(test)]mod hyperx_plan_tests{
 use super::*;use gearwise_core::{model::{DpiState,DpiStage,Modifiers,LightingEffect,Rgb,ZoneState},profile::{Macro,MacroMode,MacroStep}};
 fn plan(product:u16)->Plan{Plan{vendor:0x03f0,product,source:None,macros:vec![Macro{id:"haste".into(),name:"Keyboard chord".into(),mode:MacroMode::Once,steps:vec![MacroStep::KeyDown(0xe0),MacroStep::KeyDown(4),MacroStep::Delay(50),MacroStep::KeyUp(4),MacroStep::KeyUp(0xe0)]}],settings:vec![Setting::Dpi(DpiState{stages:vec![DpiStage::uniform(800),DpiStage::uniform(1600)],active:1}),Setting::ReportRate(500),Setting::Button{id:4,action:ButtonAction::Macro("haste".into())},Setting::Lighting{zone:0,state:ZoneState{effect:LightingEffect::Static{color:Rgb{r:20,g:40,b:80}},brightness:70}}]}}
 #[test]fn hyperx_saved_complete_plans_use_native_driver_and_never_host_hooks(){for product in [0x048e,0x028e]{let p:Plan=serde_json::from_slice(&serde_json::to_vec(&plan(product)).unwrap()).unwrap();validate(&p).unwrap();let c=compile(&p).unwrap();assert!(c.bindings.is_empty()&&!c.software_input);let result=simulate(&p).unwrap();assert!(result["packet_count"].as_u64().unwrap()>15);assert!(result["software_macros"].as_array().unwrap().is_empty());assert_eq!(result["hardware_tested"],false);}}
 #[test]fn hyperx_saved_profile_rejects_wrong_models_modes_effects_and_missing_definition(){let mut p=plan(0x048e);p.product=0x058e;assert!(validate(&p).is_err());p=plan(0x048e);p.macros[0].mode=MacroMode::Toggle;assert!(validate(&p).is_err());p=plan(0x048e);p.macros.clear();assert!(validate(&p).is_err());p=plan(0x048e);p.settings.push(Setting::Button{id:1,action:ButtonAction::Key{usage:4,mods:Modifiers(0)}});assert!(validate(&p).is_err());p=plan(0x048e);p.settings[3]=Setting::Lighting{zone:0,state:ZoneState{effect:LightingEffect::ColorCycle{period_ms:1000},brightness:100}};assert!(validate(&p).is_err());}
 #[test]fn hyperx_templates_are_explicit_simulations(){let views=templates().unwrap();for product in [0x048e,0x028e]{let v=views.iter().find(|v|v.descriptor.vendor_id==0x03f0&&v.descriptor.product_id==product).unwrap();assert!(v.descriptor.simulated&&!v.hardware_tested);assert_eq!(v.capabilities.dpi.as_ref().unwrap().max,16000);assert!(v.state.lighting.is_empty());}}
}

#[cfg(test)]mod new_brand_fixtures{
 use super::*;use gearwise_core::{model::{DpiStage,DpiState,LightingEffect,Rgb,ZoneState},profile::{Macro,MacroMode,MacroStep}};
 #[test]fn export_hyperx_pulsar_native_dtos_and_execute_saved_plans(){
  let mut fixtures=Vec::new();for(v,p)in[(0x03f0,0x048e),(0x03f0,0x028e),(0x3554,0xf507),(0x3554,0xf508)]{
   let(mut device,_)=simulated(v,p).unwrap();let base=view(device.as_mut()).unwrap();let mut settings=vec![Setting::Dpi(DpiState{stages:vec![DpiStage::uniform(800),DpiStage::uniform(1600)],active:1}),Setting::ReportRate(500),Setting::Lighting{zone:0,state:ZoneState{effect:LightingEffect::Static{color:Rgb{r:20,g:40,b:80}},brightness:100}}];let mut macros=Vec::new();
   if v==0x03f0{macros.push(Macro{id:"fixture-hyperx".into(),name:"HyperX test chord".into(),mode:MacroMode::Once,steps:vec![MacroStep::KeyDown(4),MacroStep::Delay(50),MacroStep::KeyUp(4)]});device.set_macro_definitions(&macros).unwrap();settings.push(Setting::Button{id:4,action:ButtonAction::Macro(macros[0].id.clone())});}else{settings.push(Setting::Button{id:4,action:ButtonAction::Mouse(gearwise_core::model::MouseButton::Middle)});}
   let plan=Plan{vendor:v,product:p,source:None,macros,settings};validate(&plan).unwrap();let encoded=serde_json::to_vec(&plan).unwrap();let loaded:Plan=serde_json::from_slice(&encoded).unwrap();simulate(&loaded).unwrap();for s in &loaded.settings{device.apply(s).unwrap();}let configured=view(device.as_mut()).unwrap();assert_eq!(configured.state.dpi.as_ref().unwrap().stages[1].x,1600);assert_eq!(configured.state.report_rate,Some(500));fixtures.push(serde_json::json!({"base":base,"configured":configured}));
  }
  if let Some(dir)=std::env::var_os("OPEN_HUB_NEW_BRANDS_FIXTURE_DIR"){let dir=std::path::PathBuf::from(dir);std::fs::create_dir_all(&dir).unwrap();std::fs::write(dir.join("native-views.json"),serde_json::to_vec_pretty(&fixtures).unwrap()).unwrap();}
 }
}

#[cfg(test)]mod ui_saved_new_brands{
 use super::*;
 #[test]#[ignore="requires explicit OPEN_HUB_NEW_BRANDS_UI_PROFILES from browser regression"]fn browser_saved_profiles_validate_reload_and_execute_native_drivers(){
  let path=std::env::var_os("OPEN_HUB_NEW_BRANDS_UI_PROFILES").expect("set UI-created profile fixture path");let profiles:Vec<crate::profiles::Profile>=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();assert_eq!(profiles.len(),6);
  for profile in profiles{crate::profile_transfer::validate(&profile).unwrap();let loaded:crate::profiles::Profile=serde_json::from_slice(&serde_json::to_vec(&profile).unwrap()).unwrap();let p=loaded.multi.as_ref().unwrap();assert!(supported(p.vendor,p.product));let r=simulate(p).unwrap();assert!(r["packet_count"].as_u64().unwrap()>0);assert!(!loaded.targets(0x046d,&[p.product]));assert!(!loaded.targets(0x3837,&[p.product]));}
 }
}

#[cfg(test)]mod scoped_button_plans{
 use super::*;
 #[test]fn pulsar_basic_buttons_and_viper_captured_controls_execute_independently(){
  for(vendor,product,id)in[(0x3554,0xf507,4),(0x3554,0xf508,3),(0x1532,0x00c0,9),(0x1532,0x00c1,0x60)]{
   let p=Plan{vendor,product,settings:vec![Setting::Button{id,action:ButtonAction::Disabled}],macros:vec![],source:None};validate(&p).unwrap();let loaded:Plan=serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();assert!(simulate(&loaded).unwrap()["packet_count"].as_u64().unwrap()>0);
   let(mut target,_)=simulated(vendor,product).unwrap();let(mut other,_)=simulated(vendor,product).unwrap();let before=other.read_state().unwrap();for s in &p.settings{target.apply(s).unwrap();}assert_eq!(target.read_state().unwrap().buttons[&id],ButtonAction::Disabled);assert_eq!(other.read_state().unwrap().buttons,before.buttons);
  }
 }
 #[test]fn pulsar_keyboard_and_viper_toggle_keyboard_are_rejected_in_both_compile_and_validation(){
  for(vendor,product,id)in[(0x3554,0xf507,4),(0x1532,0x00c1,9)]{let p=Plan{vendor,product,settings:vec![Setting::Button{id,action:ButtonAction::Key{usage:4,mods:gearwise_core::model::Modifiers(0)}}],macros:vec![],source:None};assert!(validate(&p).is_err());assert!(compile(&p).is_err());}
 }
}
