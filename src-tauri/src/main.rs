#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(test)]
mod multibrand_protocol;
use sidearm_synapse_parser as synapse_import;
mod macro_runtime;
mod raw_input;
mod feature_transport;
mod naga;
mod atk;
mod hyperx;
mod x11;
mod legacy;
mod appearance;
mod host_macros;
mod multibrand;
mod macro_recovery;
mod import_keys;
mod synapse_profiles;
mod icue_profiles;
mod gg_profiles;
mod profile_transfer;
mod sinowealth;
mod device;
mod device_db;
mod events;
mod foreground;
mod hidpp;
mod manager;
mod mchose;
mod mchose_config;
mod magdock;
mod audio_meter;
mod rgb;
mod screen;
mod effects;
mod onboard;
mod profiles;
mod mhub_import;
mod settings;
mod tray;
mod updater;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager as _, State};

use device::{DeviceState, FeatureInfo};
use manager::Manager;
use profiles::{ImportSummary, Profile, ProfileStore};

/// The manager is created lazily so a HID init failure is reported to the UI
/// instead of aborting startup.
struct AppState(Mutex<Option<Manager>>);

fn manager<'a>(state: &'a State<AppState>) -> Result<MutexGuard<'a, Option<Manager>>, String> {
    let mut guard = state.0.lock().map_err(|_| "device manager lock poisoned".to_string())?;
    if guard.is_none() {
        *guard = Some(Manager::new(events::sink()).map_err(|e| e.to_string())?);
    }
    Ok(guard)
}

#[derive(Serialize)]
struct DeviceSummary {
    vendor_id: u16,
    id: String,
    name: String,
    product: String,
    product_id: String,
    kind: &'static str,
    firmware_name: String,
    model_ids: Vec<String>,
    connection: String,
    protocol: String,
    features: Vec<FeatureInfo>,
}

#[derive(Serialize)]
struct ScanResult {
    devices: Vec<DeviceSummary>,
    warnings: Vec<String>,
}

#[tauri::command(async)]
fn scan_devices(app: AppHandle, state: State<AppState>) -> Result<ScanResult, String> {
    let mut guard = manager(&state)?;
    let warnings = guard.as_mut().expect("manager initialised").scan().map_err(|e| e.to_string())?;
    drop(guard);
    apply_to_new_devices(&app);
    let guard = manager(&state)?;
    let mut devices=summaries(guard.as_ref().expect("manager initialised"));drop(guard);
    let mut warnings=warnings;match app.state::<sinowealth::Service>().scan(){Ok(peers)=>devices.extend(peers.iter().map(sino_summary)),Err(e)=>warnings.push(format!("SinoWealth 检测：{e}"))};
    let (multi,issues)=app.state::<multibrand::Service>().scan()?;devices.extend(multi.iter().map(multi_summary));warnings.extend(issues);warnings.extend(app.state::<multibrand::Service>().recover_pending(&app.state::<macro_runtime::Runtime>())?);
    Ok(ScanResult { devices, warnings })
}

/// Current devices without rescanning (the event loop keeps them current).
#[tauri::command(async)]
fn list_devices(state: State<AppState>,sino:State<sinowealth::Service>,multi:State<multibrand::Service>) -> Result<Vec<DeviceSummary>, String> {
    let guard = manager(&state)?;
    let mut out=summaries(guard.as_ref().expect("manager initialised"));out.extend(sino.peers().iter().map(sino_summary));out.extend(multi.peers().iter().map(multi_summary));Ok(out)
}

fn multi_summary(d:&gearwise_core::DeviceDescriptor)->DeviceSummary{DeviceSummary{vendor_id:d.vendor_id,id:format!("multi:{}",d.id),name:d.name.clone(),product:d.name.clone(),product_id:format!("{:04X}",d.product_id),kind:"mouse",firmware_name:d.firmware.clone().unwrap_or_default(),model_ids:vec![format!("{:04X}",d.product_id)],connection:format!("{:?}",d.connection),protocol:"Open HUB 型号驱动（未实机验证）".into(),features:vec![]}}
#[tauri::command(async)]
fn multi_read(service:State<multibrand::Service>,runtime:State<macro_runtime::Runtime>,id:String)->Result<multibrand::View,String>{service.read(&id,&runtime)}
#[tauri::command(async)]
fn multi_apply(service:State<multibrand::Service>,runtime:State<macro_runtime::Runtime>,id:String,plan:multibrand::Plan)->Result<multibrand::View,String>{service.apply(&id,&plan,&runtime)}
#[tauri::command(async)]
fn multi_templates()->Result<Vec<multibrand::View>,String>{multibrand::templates()}
#[tauri::command(async)]
fn multi_simulate(plan:multibrand::Plan)->Result<serde_json::Value,String>{multibrand::simulate(&plan)}
#[tauri::command(async)]
fn multi_macro_simulate(source:host_macros::Source,button:u16,action:gearwise_core::profile::Macro)->Result<serde_json::Value,String>{host_macros::simulate(source,button,action)}
#[tauri::command(async)]
fn multi_input_sources()->Result<Vec<raw_input::SourceInfo>,String>{raw_input::sources()}
#[tauri::command(async)]
fn multi_macro_status(runtime:State<macro_runtime::Runtime>)->macro_runtime::Status{runtime.status()}
#[tauri::command(async)]
fn multi_stop_macros(service:State<multibrand::Service>,runtime:State<macro_runtime::Runtime>)->Result<(),String>{service.stop_software(&runtime)}
#[tauri::command(async)]
fn multi_stop_device_macros(service:State<multibrand::Service>,runtime:State<macro_runtime::Runtime>,id:String)->Result<(),String>{service.stop_device_software(&id,&runtime)}
#[tauri::command(async)]
fn multi_recovery_list(service:State<multibrand::Service>,id:String)->Result<multibrand::RecoveryList,String>{service.recovery_list(&id)}
#[tauri::command(async)]
fn multi_recovery_observe(service:State<multibrand::Service>,runtime:State<macro_runtime::Runtime>,id:String,source:host_macros::Source)->Result<(),String>{service.observe_recovery(&id,&source,&runtime)}
#[tauri::command(async)]
fn multi_recovery_restore(service:State<multibrand::Service>,runtime:State<macro_runtime::Runtime>,id:String,key:String,revision:String,source:host_macros::Source,confirmed:bool)->Result<(),String>{service.restore_recovery(&id,&key,&revision,&source,confirmed,&runtime)}
#[tauri::command(async)]
fn multi_reconnect_list(service:State<multibrand::Service>)->Result<multibrand::PendingList,String>{service.reconnect_list()}
#[tauri::command(async)]
fn multi_reconnect_candidates(service:State<multibrand::Service>,key:String,revision:String)->Result<multibrand::PendingCandidates,String>{service.reconnect_candidates(&key,&revision)}
#[tauri::command(async)]
fn multi_reconnect_resume(service:State<multibrand::Service>,runtime:State<macro_runtime::Runtime>,key:String,revision:String,id:String,candidate_revision:String,source:Option<host_macros::Source>,confirmed:bool)->Result<multibrand::View,String>{service.reconnect_resume(&key,&revision,&id,&candidate_revision,source,confirmed,&runtime)}
#[tauri::command(async)]
fn multi_reconnect_retry(service:State<multibrand::Service>,key:String,revision:String,id:String,candidate_revision:String,confirmed:bool)->Result<String,String>{service.reconnect_retry(&key,&revision,&id,&candidate_revision,confirmed)}
#[tauri::command(async)]
fn multi_reconnect_discard(service:State<multibrand::Service>,key:String,revision:String)->Result<(),String>{service.reconnect_discard(&key,&revision)}
fn sino_summary(p:&sinowealth::Peer)->DeviceSummary{DeviceSummary{vendor_id:p.model.vendor,id:p.id.clone(),name:p.model.name.clone(),product:p.model.name.clone(),product_id:format!("{:04X}",p.model.product),kind:"mouse",firmware_name:p.model.firmware.clone(),model_ids:vec![format!("{:04X}",p.model.product)],connection:"USB 有线".into(),protocol:"SinoWealth wired".into(),features:vec![]}}
#[tauri::command(async)]
fn sino_read(sino:State<sinowealth::Service>,id:String,profile:Option<u8>)->Result<sinowealth::View,String>{sino.read(&id,profile)}
#[tauri::command(async)]
fn sino_apply(sino:State<sinowealth::Service>,id:String,plan:sinowealth::Plan)->Result<sinowealth::View,String>{sino.write(&id,plan)}
#[tauri::command(async)]
fn sino_models()->Vec<sinowealth::Model>{sinowealth::models()}
#[tauri::command(async)]
fn sino_simulate(plan:sinowealth::Plan)->Result<serde_json::Value,String>{sinowealth::simulate(&plan)}

#[tauri::command(async)]
fn preview_synapse_import(path:String)->Result<synapse_import::types::ParsedSynapseProfiles,String>{synapse_import::preview(std::path::Path::new(&path))}
#[tauri::command(async)]
fn prepare_synapse_import(path:String)->Result<synapse_profiles::Prepared,String>{synapse_profiles::prepare(std::path::Path::new(&path))}
#[tauri::command(async)]
fn prepare_icue_import(path:String)->Result<icue_profiles::Prepared,String>{icue_profiles::prepare(std::path::Path::new(&path))}
#[tauri::command]
fn prepare_gg_import(path:String)->Result<icue_profiles::Prepared,String>{gg_profiles::prepare(std::path::Path::new(&path))}

fn summaries(manager: &Manager) -> Vec<DeviceSummary> {
    manager
        .devices()
        .iter()
        .map(|d| DeviceSummary {
            vendor_id: d.vendor_id,
            id: d.id.clone(),
            name: d.name.clone(),
            product: d.product.clone(),
            product_id: format!("{:04X}", d.product_id),
            kind: d.kind,
            firmware_name: d.firmware_name.clone(),
            model_ids: d.model_ids.iter().map(|id| format!("{id:04X}")).collect(),
            connection: if d.vendor_id == mchose::VID {
                "MCHOSE 2.4 GHz receiver".into()
            } else if d.index == hidpp::DIRECT {
                "Direct (USB)".into()
            } else {
                format!("{} · slot {}", d.product, d.index)
            },
            protocol: d.protocol.clone(),
            features: d.features.values().cloned().collect(),
        })
        .collect()
}

#[tauri::command(async)]
fn device_state(state: State<AppState>,sino:State<sinowealth::Service>, id: String) -> Result<DeviceState, String> {
 if id.starts_with("multi:"){return Ok(DeviceState{battery:None,dpi:None,report_rate:None,onboard_mode:None,errors:vec![]});}
 if id.starts_with("sino:"){let v=sino.read(&id,None)?;return Ok(DeviceState{battery:None,dpi:Some(device::Dpi{feature:0,x:v.plan.dpi[v.plan.active][0],y:Some(v.plan.dpi[v.plan.active][1]),default:None,supported:(100..=16000).step_by(100).collect(),supports_y:true,lod:None,supports_lod:false}),report_rate:Some(device::ReportRate{feature:0,current:match v.plan.rate{125=>1,250=>2,500=>3,_=>4},choices:[125,250,500,1000].into_iter().enumerate().map(|(i,hz)|device::Choice{value:i as u8+1,label:format!("{hz} Hz"),hz}).collect()}),onboard_mode:Some("onboard"),errors:vec![]})}

    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    manager.with_device(&id, |d, t| Ok(d.state(t))).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn set_dpi(state: State<AppState>,sino:State<sinowealth::Service>, id: String, x: u16, y: Option<u16>, lod: Option<u8>) -> Result<(), String> {
 if id.starts_with("sino:"){if lod.is_some(){return Err("此适配器不写入LOD".into())}let mut p=sino.read(&id,None)?.plan;p.dpi[p.active]=[x,y.unwrap_or(x)];p.macros.clear();sino.write(&id,p)?;return Ok(())}

    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    manager.with_device(&id, |d, t| d.set_dpi(t, x, y, lod)).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn set_report_rate(state: State<AppState>,sino:State<sinowealth::Service>, id: String, value: u8) -> Result<(), String> {
 if id.starts_with("sino:"){let rate=[125,250,500,1000].get(value.checked_sub(1).ok_or("回报率无效")? as usize).copied().ok_or("回报率无效")?;let mut p=sino.read(&id,None)?.plan;p.rate=rate;p.macros.clear();sino.write(&id,p)?;return Ok(())}

    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    manager.with_device(&id, |d, t| d.set_report_rate(t, value)).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn set_onboard_mode(state: State<AppState>, id: String, host: bool) -> Result<(), String> {
    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    manager.with_device(&id, |d, t| d.set_onboard_mode(t, host)).map_err(|e| e.to_string())
}

fn with_mchose<R>(state: &State<AppState>, id:&str, f: impl FnOnce(&mchose::Transport)->hidpp::Result<R>)->Result<R,String>{
    let guard=manager(state)?;
    guard.as_ref().expect("manager initialised").with_device(id,|_,t|f(t.mchose().ok_or_else(||hidpp::Error::Other("This is not a verified MCHOSE device".into()))?)).map_err(|e|e.to_string())
}
#[tauri::command(async)]
fn mchose_hardware(state:State<AppState>,id:String)->Result<mchose_config::HardwareState,String>{with_mchose(&state,&id,|t|t.hardware_state())}
#[tauri::command(async)]
fn mchose_macro(state:State<AppState>,id:String,slot:u8)->Result<Option<mchose_config::MouseMacro>,String>{with_mchose(&state,&id,|t|t.read_macro(slot))}
#[tauri::command(async)]
fn mchose_macro_file(path:String,slot:u8,name:String,mode:u8,events:Vec<mchose_config::MacroEvent>)->Result<(),String>{
    if slot>=18{return Err("宏槽无效".into())}
    let raw=mchose_config::encode_macro(&name,mode,&events).map_err(|e|e.to_string())?;
    let record=mchose_config::MouseMacro{slot,name,mode,events,raw};
    std::fs::write(path,serde_json::to_vec_pretty(&record).map_err(|e|e.to_string())?).map_err(|e|e.to_string())
}
#[tauri::command(async)]
fn mchose_apply(app:AppHandle,state:State<AppState>,id:String,expected_profile:u8,action:mchose_config::Action)->Result<mchose_config::HardwareState,String>{
    let result=with_mchose(&state,&id,|t|{
        if t.settings()?[0]!=expected_profile{return Err(hidpp::Error::Other("板载配置已经改变，请刷新后重试".into()))}
        t.apply_hardware(action)
    })?;
    let _=app.emit("device-changed",id);Ok(result)
}
#[tauri::command(async)]
fn mchose_export(state:State<AppState>,id:String,path:String)->Result<(),String>{
    let snapshot=with_mchose(&state,&id,|t|t.full_snapshot())?;
    std::fs::write(path,serde_json::to_vec_pretty(&snapshot).map_err(|e|e.to_string())?).map_err(|e|e.to_string())
}
#[tauri::command(async)]
fn mchose_restore(app:AppHandle,state:State<AppState>,id:String,path:String)->Result<mchose_config::HardwareState,String>{
    let meta=std::fs::metadata(&path).map_err(|e|e.to_string())?;
    if meta.len()>1024*1024{return Err("备份文件过大".into())}
    let snapshot=serde_json::from_slice(&std::fs::read(path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    let result=with_mchose(&state,&id,|t|t.restore_snapshot(snapshot))?;
    let _=app.emit("device-changed",id);Ok(result)
}
#[tauri::command]
fn open_mhub(app:AppHandle)->Result<(),String>{
    let root=std::env::var_os("ProgramFiles").ok_or("ProgramFiles is unavailable")?;
    let exe=std::path::PathBuf::from(root).join("MCHOSE HUB").join("MCHOSE HUB Launcher.exe");
    std::process::Command::new(exe).spawn().map_err(|e|e.to_string())?;
    app.exit(0);Ok(())
}


#[tauri::command(async)]
fn magdock_state(c:State<magdock::Controller>)->Result<magdock::View,String>{c.view()}
#[tauri::command(async)]
fn magdock_apply(c:State<magdock::Controller>,engine:State<effects::Engine>,light:magdock::Light)->Result<magdock::View,String>{engine.stop()?;c.apply(light)}
#[tauri::command(async)]
fn magdock_music(c:State<magdock::Controller>,enabled:bool,light:magdock::Light,gain:f32)->Result<magdock::View,String>{if enabled{c.music(light,gain)}else{c.stop()?;c.view()}}
#[tauri::command(async)]
fn magdock_reset(c:State<magdock::Controller>,engine:State<effects::Engine>)->Result<magdock::View,String>{engine.stop()?;c.reset()}


#[tauri::command(async)]
async fn rgb_devices(app:tauri::AppHandle)->Result<Vec<rgb::Device>,String>{
 tauri::async_runtime::spawn_blocking(move||app.state::<rgb::Server>().scan()).await.map_err(|e|format!("灯光识别任务失败：{e}"))?
}
#[tauri::command(async)]
fn effects_status(engine:State<effects::Engine>)->Result<effects::Status,String>{engine.status()}
#[tauri::command(async)]
fn effects_start(engine:State<effects::Engine>,dock:State<magdock::Controller>,config:effects::Config)->Result<effects::Status,String>{dock.stop()?;engine.start(config,dock.inner().clone())}
#[tauri::command(async)]
fn effects_stop(engine:State<effects::Engine>)->Result<effects::Status,String>{engine.stop()}

#[tauri::command(async)]
fn effects_preset(path:String,mut config:effects::Config,target_keys:Option<Vec<String>>)->Result<(),String>{let keys=target_keys.unwrap_or_default();if keys.len()>64||keys.iter().any(|s|s.len()>4096){return Err("设备标识过长".into())}config.rgb.clear();let mut check=config.clone();check.dock=true;check.validate()?;std::fs::write(path,serde_json::to_vec_pretty(&serde_json::json!({"format":"open-hub-lighting/1","config":config,"target_keys":keys})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())}

#[tauri::command(async)]
fn import_mhub_profiles(app:AppHandle,state:State<ProfilesState>,path:Option<String>)->Result<serde_json::Value,String>{let path=path.map(PathBuf::from).map(Ok).unwrap_or_else(mhub_import::default_path)?;let doc=mhub_import::read(&path)?;let mut guard=lock_store(&state)?;let mut updated=guard.clone();let summary=mhub_import::import(&mut updated,&doc)?;updated.save(&state.path).map_err(|e|e.to_string())?;*guard=updated;let result=serde_json::json!({"summary":summary,"view":view(&guard)});drop(guard);let _=app.emit("profiles-changed",());tray::refresh(&app);Ok(result)}

// ---------- profiles ----------

struct ProfilesState {
    store: Mutex<ProfileStore>,
    path: PathBuf,
}

fn lock_store<'a>(state: &'a ProfilesState) -> Result<MutexGuard<'a, ProfileStore>, String> {
    state.store.lock().map_err(|_| "profile store lock poisoned".to_string())
}

#[derive(Serialize)]
struct ProfilesView {
    profiles: Vec<Profile>,
    auto_switch: bool,
    active_id: String,
}

fn view(store: &ProfileStore) -> ProfilesView {
    ProfilesView { profiles: store.profiles.clone(), auto_switch: store.auto_switch, active_id: store.active_id.clone() }
}

/// Mutates the store, persists it, and returns the new view.
fn update_store(state: &ProfilesState, f: impl FnOnce(&mut ProfileStore)) -> Result<ProfilesView, String> {
    let mut store = lock_store(state)?;
    f(&mut store);
    store.save(&state.path).map_err(|e| format!("could not save profiles: {e}"))?;
    Ok(view(&store))
}

#[derive(Clone, Serialize)]
struct Activation {
    id: String,
    name: String,
    /// "manual" or "auto"
    reason: &'static str,
    errors: Vec<String>,
}

/// Activates a profile; native multi-brand plans require one exact target.
/// Lock order is always profiles, then devices.
fn activate(app: &AppHandle, id: &str, reason: &'static str) -> Result<Activation, String> {activate_target(app,id,reason,None)}
fn activate_target(app: &AppHandle, id: &str, reason: &'static str, device_id:Option<&str>) -> Result<Activation, String> {
    let profiles = app.state::<ProfilesState>();
    let profile = {
        let mut store = lock_store(&profiles)?;
        let profile = store.get(id).cloned().ok_or_else(|| "profile not found".to_string())?;
        if profile.sino.is_some()||profile.multi.is_some(){profile_transfer::validate(&profile)?;}
        if profile.sino.is_none()&&profile.multi.is_none(){store.active_id = profile.id.clone();store.save(&profiles.path).map_err(|e| format!("could not save profiles: {e}"))?;}
        profile
    };

    let devices = app.state::<AppState>();
    let mut guard = manager(&devices)?;
    let manager = guard.as_mut().expect("manager initialised");
    if manager.devices().is_empty() {
        manager.scan().map_err(|e| e.to_string())?;
    }
    let mut errors = Vec::new();
    for d in manager.devices() {
        match manager.with_device(&d.id, |d, t| Ok(profiles::apply_to_device(&profile, d, t))) {
            Ok(errs) => errors.extend(errs),
            Err(e) => errors.push(format!("{}: {e}", d.name)),
        }
    }
    if let Ok(mut applied) = app.state::<AppliedDevices>().0.lock() {
        applied.extend(manager.devices().iter().map(|d| d.id.clone()));
    }
    drop(guard);

    if let Some(plan)=&profile.multi{let service=app.state::<multibrand::Service>();let ids=service.peers().into_iter().filter(|d|d.vendor_id==plan.vendor&&d.product_id==plan.product).map(|d|d.id).collect::<Vec<_>>();match multibrand::choose_profile_device(&ids,device_id.map(|id|id.strip_prefix("multi:").unwrap_or(id))){Ok(id)=>if let Err(e)=service.apply(&id,plan,&app.state::<macro_runtime::Runtime>()){errors.push(e);},Err(e)=>errors.push(e)}}
    if let Some(plan)=&profile.sino{let service=app.state::<sinowealth::Service>();let ids=service.peers().into_iter().filter(|d|d.model==plan.model).map(|d|d.id).collect::<Vec<_>>();match multibrand::choose_profile_device(&ids,device_id.map(|id|id.strip_prefix("sino:").unwrap_or(id))){Ok(id)=>if let Err(e)=service.activate(&id,plan.clone()){errors.push(e);},Err(e)=>errors.push(e)}}

    if profile.multi.is_none()&&profile.sino.is_none()&&profile.target_vendor.is_none()&&profile.target_model.is_none()&&profile.ghub_id.is_none()&&profile.mchose_board.is_none(){let service=app.state::<sinowealth::Service>();for d in service.peers(){if let Err(e)=service.apply_generic(&d.id,&profile){errors.push(format!("{}: {e}",d.model.name));}}}
    if (profile.sino.is_some()||profile.multi.is_some())&&errors.is_empty(){let mut store=lock_store(&profiles)?;store.active_id=profile.id.clone();store.save(&profiles.path).map_err(|e|e.to_string())?;}
    let activation = Activation { id: profile.id, name: profile.name, reason, errors };
    tray::refresh(app);
    let _ = app.emit("profile-activated", activation.clone());
    Ok(activation)
}

/// Devices the active profile has been applied to since they connected.
/// A device that disconnects drops out, so it gets the profile again on return.
#[derive(Default)]
struct AppliedDevices(Mutex<HashSet<String>>);

/// Applies the active profile to devices that connected (or reconnected)
/// since it was last applied: after power-cycling the mouse, waking it, or
/// starting Open HUB. Does nothing until the user has saved settings, so a
/// first launch never changes the mouse unasked.
/// Lock order: profiles, then devices, then the applied set.
fn apply_to_new_devices(app: &AppHandle) {
    let profiles = app.state::<ProfilesState>();
    if !profiles.path.exists() {
        return;
    }
    let (profile,auto_switch) = match lock_store(&profiles) {
        Ok(store) => match store.get(&store.active_id) {
            Some(p) => (p.clone(),store.auto_switch),
            None => return,
        },
        Err(_) => return,
    };

    let devices = app.state::<AppState>();
    let Ok(guard) = manager(&devices) else { return };
    let Some(manager) = guard.as_ref() else { return };
    let present: Vec<String> = manager.devices().iter().map(|d| d.id.clone()).collect();
    let applied_state = app.state::<AppliedDevices>();
    let Ok(mut applied) = applied_state.0.lock() else { return };
    applied.retain(|id| present.contains(id));
    let new: Vec<String> = present.into_iter().filter(|id| !applied.contains(id)).collect();
    for id in &new {
        if !auto_switch && manager.devices().iter().any(|d|d.id==*id&&d.vendor_id==mchose::VID){
            applied.insert(id.clone());continue;
        }
        match manager.with_device(id, |d, t| Ok(profiles::apply_to_device(&profile, d, t))) {
            Ok(errors) => errors.iter().for_each(|e| eprintln!("reapply {}: {e}", profile.name)),
            Err(e) => eprintln!("reapply {}: {e}", profile.name),
        }
        if events::logging() {
            eprintln!("[event] applied profile '{}' to {id}", profile.name);
        }
        applied.insert(id.clone());
    }
    drop(applied);
    drop(guard);
    for id in new {
        let _ = app.emit("device-changed", id);
    }
}

/// Switches profiles when another application comes to the foreground,
/// like G HUB's process watcher. Driven by a Windows event hook, not polling.
fn spawn_profile_watcher(app: AppHandle) {
    let own_exe = std::env::current_exe().ok().map(|p| p.to_string_lossy().to_lowercase());
    let last_exe = Mutex::new(String::new());
    foreground::watch(move || {
        let Some(exe) = foreground::foreground_exe() else { return };
        if events::logging() {
            eprintln!("[event] foreground: {exe}");
        }
        {
            let Ok(mut last) = last_exe.lock() else { return };
            if *last == exe {
                return;
            }
            *last = exe.clone();
        }
        if own_exe.as_deref() == Some(exe.to_lowercase().as_str()) {
            return;
        }
        let target = {
            let state = app.state::<ProfilesState>();
            let Ok(store) = lock_store(&state) else { return };
            if !store.auto_switch {
                return;
            }
            let profile = store.match_exe(&exe);
            if profile.id == store.active_id {
                return;
            }
            profile.id.clone()
        };
        if let Err(e) = activate(&app, &target, "auto") {
            eprintln!("auto-switch failed: {e}");
        }
    });
}

/// Reads the mouse's on-board profile memory. Read-only.
#[tauri::command(async)]
fn onboard_snapshot(state: State<AppState>, id: String) -> Result<onboard::Snapshot, String> {
    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    manager.with_device(&id, |d, t| d.onboard_snapshot(t).map(|(s, _)| s)).map_err(|e| e.to_string())
}

/// Writes a backup file: decoded snapshot plus raw sectors (hex).
fn write_onboard_backup(
    app_version: &str,
    path: &std::path::Path,
    device: &device::Device,
    snapshot: &onboard::Snapshot,
    raw: &[onboard::RawSector],
) -> Result<(), String> {
    let backup = serde_json::json!({
        "format": "open-ghub-onboard-backup/1",
        "app_version": app_version,
        "created": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        "device": device.name,
        "model_ids": device.model_ids.iter().map(|id| format!("{id:04X}")).collect::<Vec<_>>(),
        "snapshot": snapshot,
        "sectors": raw,
    });
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(&backup).map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| format!("could not write {}: {e}", path.display()))
}

/// Automatic backup taken before every on-board write.
fn auto_backup_path(app: &AppHandle, device: &device::Device) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let name: String = device.name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("onboard-backups");
    Ok(dir.join(format!("{name}-{stamp}.json")))
}

/// G HUB's agent also writes on-board memory; two writers at once could
/// corrupt a sector, so writes are refused while it runs.
fn ghub_agent_running() -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq lghub_agent.exe", "/NH"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).to_ascii_lowercase().contains("lghub_agent.exe"))
    }
    #[cfg(not(windows))]
    {
        false
    }
}

const GHUB_RUNNING: &str = "G HUB is running. Quit G HUB (tray icon → Quit) before changing on-board memory, so the two don't write at the same time.";

/// Reads the on-board memory and writes a backup to `path`. Read-only towards the device.
#[tauri::command(async)]
fn save_onboard_backup(app: AppHandle, state: State<AppState>, id: String, path: String) -> Result<(), String> {
    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    manager
        .with_device(&id, |d, t| {
            let (snapshot, raw) = d.onboard_snapshot(t)?;
            write_onboard_backup(&app.package_info().version.to_string(), std::path::Path::new(&path), d, &snapshot, &raw).map_err(hidpp::Error::Other)
        })
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct OnboardWriteResult {
    report: onboard::WriteReport,
    backup_path: String,
    snapshot: onboard::Snapshot,
}

/// Backs up the on-board memory, then writes new DPI levels, starting level
/// and report rate into one user slot, verified by reading back.
#[tauri::command(async)]
fn write_onboard_slot(
    app: AppHandle,
    state: State<AppState>,
    id: String,
    settings: onboard::SlotSettings,
) -> Result<OnboardWriteResult, String> {
    if ghub_agent_running() {
        return Err(GHUB_RUNNING.into());
    }
    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    manager
        .with_device(&id, |d, t| {
            let (before, raw) = d.onboard_snapshot(t)?;
            let backup = auto_backup_path(&app, d).map_err(hidpp::Error::Other)?;
            write_onboard_backup(&app.package_info().version.to_string(), &backup, d, &before, &raw).map_err(hidpp::Error::Other)?;
            let report = d.write_slot_settings(t, &settings)?;
            let (snapshot, _) = d.onboard_snapshot(t)?;
            Ok(OnboardWriteResult { report, backup_path: backup.display().to_string(), snapshot })
        })
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct OnboardRestoreResult {
    sectors_written: usize,
    backup_path: String,
    snapshot: onboard::Snapshot,
}

/// Restores the directory and user profiles from a backup file made for the
/// same mouse model. The current state is backed up first.
#[tauri::command(async)]
fn restore_onboard_backup(app: AppHandle, state: State<AppState>, id: String, path: String) -> Result<OnboardRestoreResult, String> {
    if ghub_agent_running() {
        return Err(GHUB_RUNNING.into());
    }
    let file: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).map_err(|e| format!("could not read {path}: {e}"))?)
        .map_err(|e| format!("not a backup file: {e}"))?;
    if file.get("format").and_then(|f| f.as_str()) != Some("open-ghub-onboard-backup/1") {
        return Err("not an Open HUB on-board backup".into());
    }
    let raw: Vec<onboard::RawSector> =
        serde_json::from_value(file.get("sectors").cloned().unwrap_or_default()).map_err(|e| format!("backup sectors are unreadable: {e}"))?;
    let sectors = onboard::restorable_sectors(&raw)?;
    let backup_models: Vec<String> = serde_json::from_value(file.get("model_ids").cloned().unwrap_or_default()).unwrap_or_default();

    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    manager
        .with_device(&id, |d, t| {
            let models: Vec<String> = d.model_ids.iter().map(|m| format!("{m:04X}")).collect();
            if !backup_models.is_empty() && !backup_models.iter().any(|m| models.contains(m)) {
                return Err(hidpp::Error::Other(format!("this backup is for a different mouse ({})", backup_models.join(", "))));
            }
            let (before, current_raw) = d.onboard_snapshot(t)?;
            let backup = auto_backup_path(&app, d).map_err(hidpp::Error::Other)?;
            write_onboard_backup(&app.package_info().version.to_string(), &backup, d, &before, &current_raw).map_err(hidpp::Error::Other)?;
            let sectors_written = d.restore_sectors(t, &sectors)?;
            let (snapshot, _) = d.onboard_snapshot(t)?;
            Ok(OnboardRestoreResult { sectors_written, backup_path: backup.display().to_string(), snapshot })
        })
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn get_profiles(state: State<ProfilesState>) -> Result<ProfilesView, String> {
    Ok(view(&*lock_store(&state)?))
}

#[tauri::command(async)]
fn save_profile(app: AppHandle, state: State<ProfilesState>, mut profile: Profile) -> Result<ProfilesView, String> {
    if profile.sino.is_some()||profile.multi.is_some(){profile_transfer::validate(&profile)?;}
    if profile.mchose_board.is_some()&&profile.target_vendor.is_some_and(|v|v!=mchose::VID){return Err("板载引用与品牌不一致".into())}
    if profile.mchose_board.is_some_and(|b|b>2){return Err("迈从板载配置必须为 1–3".into())}
    if profile.id.is_empty() {
        profile.id = profiles::new_id();
    }
    if profile.name.trim().is_empty() || (profile.levels.is_empty()&&profile.mchose_board.is_none()) {
        return Err("a profile needs a name and at least one DPI level".into());
    }
    let view = update_store(&state, |store| store.upsert(profile))?;
    tray::refresh(&app);
    Ok(view)
}

#[tauri::command(async)]
fn delete_profile(app: AppHandle, state: State<ProfilesState>, id: String) -> Result<ProfilesView, String> {
    let view = update_store(&state, |store| store.delete(&id))?;
    tray::refresh(&app);
    Ok(view)
}

#[tauri::command(async)]
fn set_auto_switch(app: AppHandle, state: State<ProfilesState>, enabled: bool) -> Result<ProfilesView, String> {
    let view = update_store(&state, |store| store.auto_switch = enabled)?;
    tray::refresh(&app);
    Ok(view)
}

#[tauri::command(async)]
fn activate_profile(app: AppHandle, id: String,device_id:Option<String>) -> Result<Activation, String> {
    activate_target(&app, &id, "manual",device_id.as_deref())
}

#[derive(Serialize)]
struct ImportResult {
    summary: ImportSummary,
    view: ProfilesView,
}

#[tauri::command(async)]
fn import_ghub_profiles(app: AppHandle, state: State<ProfilesState>) -> Result<ImportResult, String> {
    let path = profiles::ghub_settings_path()
        .filter(|p| p.exists())
        .ok_or_else(|| "G HUB settings not found (expected %LOCALAPPDATA%\\LGHUB\\settings.db)".to_string())?;
    let mut summary = None;
    let view = update_store(&state, |store| summary = Some(profiles::import_ghub(store, &path)))?;
    let summary = summary.expect("import ran")?;
    tray::refresh(&app);
    Ok(ImportResult { summary, view })
}

#[tauri::command(async)]
fn preview_profile_import(source:String,path:Option<String>)->Result<profile_transfer::Preview,String>{profile_transfer::preview(&source,path)}
#[tauri::command(async)]
fn commit_profile_import(app:AppHandle,state:State<ProfilesState>,profiles:Vec<Profile>)->Result<ProfilesView,String>{
 let mut guard=lock_store(&state)?;let mut updated=guard.clone();profile_transfer::merge(&mut updated,profiles)?;updated.save(&state.path).map_err(|e|e.to_string())?;*guard=updated;let result=view(&guard);drop(guard);let _=app.emit("profiles-changed",());tray::refresh(&app);Ok(result)
}
#[tauri::command(async)]
fn export_profile_pack(path:String,profiles:Vec<Profile>)->Result<(),String>{
 if profiles.len()>512{return Err("最多导出512组配置".into())}for p in &profiles{profile_transfer::validate(p)?;}
 std::fs::write(path,serde_json::to_vec_pretty(&serde_json::json!({"format":"open-hub-profiles/1","profiles":profiles})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())
}

// ---------- app settings and updates ----------

#[derive(Serialize)]
struct AppInfo {
    version: String,
    settings: settings::AppSettings,
    update: Option<updater::UpdateInfo>,
}

#[tauri::command(async)]
fn get_app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        settings: app.state::<settings::SettingsState>().get(),
        update: app.state::<updater::UpdateState>().available(),
    }
}

#[tauri::command(async)]
fn set_app_settings(app: AppHandle, settings: settings::AppSettings) -> Result<(), String> {
    app.state::<settings::SettingsState>().set(settings)
}

#[tauri::command]
async fn check_for_update(app: AppHandle) -> Result<Option<updater::UpdateInfo>, String> {
    updater::check(&app).await
}

#[tauri::command]
async fn install_update(app: AppHandle, expected_version:String) -> Result<(), String> {
    updater::install(&app,&expected_version)
}
#[tauri::command]
async fn download_update(app:AppHandle,expected_version:String)->Result<updater::UpdateInfo,String>{
    updater::download(&app,&expected_version).await
}

/// Runs `f` on the first device with on-board profiles (CLI diagnostics).
/// Refuses while G HUB's agent runs, like the app.
fn onboard_cli(f: impl FnOnce(&device::Device, &hidpp::Transport) -> Result<String, String>) {
    if ghub_agent_running() {
        return eprintln!("{GHUB_RUNNING}");
    }
    let mut manager = match Manager::new(events::sink()) {
        Ok(m) => m,
        Err(e) => return eprintln!("HID init failed: {e}"),
    };
    if let Err(e) = manager.scan() {
        return eprintln!("scan failed: {e}");
    }
    let Some(id) = manager.devices().iter().find(|d| d.has(device::feature::ONBOARD_PROFILES)).map(|d| d.id.clone()) else {
        return eprintln!("no device with on-board profiles found");
    };
    let mut f = Some(f);
    match manager.with_device(&id, |d, t| Ok((f.take().expect("called once"))(d, t))) {
        Ok(Ok(out)) => println!("{out}"),
        Ok(Err(e)) => eprintln!("{e}"),
        Err(e) => eprintln!("{e}"),
    }
}

/// `open-ghub --onboard-dump [backup.json]`: read-only. Prints the decoded
/// on-board profiles; with a path, also writes the raw sectors there.
fn onboard_dump(backup_path: Option<String>) {
    let mut manager = match Manager::new(events::sink()) {
        Ok(m) => m,
        Err(e) => return eprintln!("HID init failed: {e}"),
    };
    if let Err(e) = manager.scan() {
        return eprintln!("scan failed: {e}");
    }
    for d in manager.devices() {
        if !d.has(device::feature::ONBOARD_PROFILES) {
            continue;
        }
        match manager.with_device(&d.id, |d, t| d.onboard_snapshot(t)) {
            Ok((snapshot, raw)) => {
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "device": d.name, "snapshot": snapshot })).unwrap_or_default());
                if let Some(path) = &backup_path {
                    let backup = serde_json::json!({
                        "device": d.name, "model_ids": d.model_ids, "info": snapshot.info, "sectors": raw,
                    });
                    match std::fs::write(path, serde_json::to_vec_pretty(&backup).unwrap_or_default()) {
                        Ok(()) => eprintln!("raw sectors written to {path}"),
                        Err(e) => eprintln!("could not write {path}: {e}"),
                    }
                }
            }
            Err(e) => eprintln!("{}: {e}", d.name),
        }
    }
}

/// `open-ghub --probe`: read-only diagnostic that prints every detected
/// device and its current settings as JSON, without opening a window.
fn probe() {
    let mut manager = match Manager::new(events::sink()) {
        Ok(m) => m,
        Err(e) => return eprintln!("HID init failed: {e}"),
    };
    match manager.scan() {
        Ok(warnings) => warnings.iter().for_each(|w| eprintln!("warning: {w}")),
        Err(e) => return eprintln!("scan failed: {e}"),
    }
    for d in manager.devices() {
        let state = manager.with_device(&d.id, |d, t| Ok(d.state(t))).ok();
        let features: Vec<String> = d.features.values().map(|f| format!("{:04X}", f.id)).collect();
        let report = serde_json::json!({
            "name": d.name, "firmware_name": d.firmware_name,
            "model_ids": d.model_ids.iter().map(|id| format!("{id:04X}")).collect::<Vec<_>>(), "kind": d.kind, "product_id": format!("{:04X}", d.product_id),
            "index": d.index, "protocol": d.protocol, "features": features, "state": state,
        });
        println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
    }
    if manager.devices().is_empty() {
        println!("no HID++ devices found");
    }
    println!("foreground app: {}", foreground::foreground_exe().unwrap_or_else(|| "(none)".into()));
}

// The Windows updater exits directly; share cleanup with ordinary app exit.
pub(crate) fn prepare_shutdown(app:&AppHandle){
 let _=app.state::<multibrand::Service>().stop_software(&app.state::<macro_runtime::Runtime>());
 app.state::<macro_runtime::Runtime>().shutdown();let _=app.state::<effects::Engine>().stop();let _=app.state::<magdock::Controller>().stop();app.state::<rgb::Server>().shutdown();
}

fn main() {
    if let Some(pos)=std::env::args().position(|a|a=="--verify-release-update"){
        let output=std::env::args().nth(pos+1).expect("verification report path required");
        let app=tauri::Builder::default().plugin(tauri_plugin_updater::Builder::new().build()).build(tauri::generate_context!()).expect("verification host");
        let result=tauri::async_runtime::block_on(updater::verify_published(app.handle()));
        let report=match result{Ok(v)=>v,Err(e)=>serde_json::json!({"passed":false,"error":e,"installed":false,"hardware_accessed":false})};
        std::fs::write(output,serde_json::to_vec_pretty(&report).unwrap()).expect("write verification report");
        std::process::exit(if report["passed"]==true{0}else{1});
    }

    if std::env::args().any(|v|v=="--three-brand-self-test"){let result=(||->Result<serde_json::Value,String>{let templates=multibrand::templates()?;let mut results=Vec::new();for v in templates{let p=multibrand::template_plan(&v)?;results.push(serde_json::json!({"model":v.descriptor.name,"result":multibrand::simulate(&p)?}));}Ok(serde_json::json!({"simulated":true,"hardware_tested":false,"devices":results}))})();match result{Ok(v)=>println!("{}",serde_json::to_string_pretty(&v).unwrap()),Err(e)=>{eprintln!("{e}");std::process::exit(1)}}return;}

    let args:Vec<String>=std::env::args().collect();
    if let Some(i)=args.iter().position(|v|v=="--synapse-import-probe"){let result=args.get(i+1).ok_or_else(||"缺少配置文件路径".to_string()).and_then(|p|synapse_import::preview(std::path::Path::new(p)));match result{Ok(v)=>println!("{}",serde_json::to_string_pretty(&v).unwrap()),Err(e)=>{eprintln!("{e}");std::process::exit(1)}}return;}

    if std::env::args().any(|a|a=="--mhub-import-probe"){let result=(||->Result<serde_json::Value,String>{let doc=mhub_import::read(&mhub_import::default_path()?)?;let mut store=ProfileStore::default();let summary=mhub_import::import(&mut store,&doc)?;Ok(serde_json::json!({"summary":summary,"profiles":store.profiles}))})();match result{Ok(v)=>println!("{}",serde_json::to_string_pretty(&v).unwrap()),Err(e)=>{eprintln!("{e}");std::process::exit(1)}}return;}

    if std::env::args().any(|a|a=="--lighting-self-test"){let result=effects::hardware_test();match result{Ok(v)=>println!("{}",serde_json::to_string_pretty(&v).unwrap()),Err(e)=>{eprintln!("{e}");std::process::exit(1)}}return;}

    if std::env::args().any(|a|a=="--lighting-probe"){let result=(||->Result<serde_json::Value,String>{let rgb=rgb::Client::connect()?.devices;let screen=screen::sample()?;let meter=audio_meter::Meter::open()?;Ok(serde_json::json!({"rgb":rgb,"screen":screen,"audio_peak":meter.peak()?}))})();match result{Ok(v)=>println!("{}",serde_json::to_string_pretty(&v).unwrap()),Err(e)=>{eprintln!("{e}");std::process::exit(1)}}return;}

    if std::env::args().any(|a|a=="--magdock-probe"||a=="--magdock-self-test"||a=="--magdock-music-test") {
        let c=magdock::Controller::default();let result=if std::env::args().any(|a|a=="--magdock-self-test"){c.self_test()}else if std::env::args().any(|a|a=="--magdock-music-test"){( c.view()).and_then(|v|{c.music(v.light,2.0)?;std::thread::sleep(std::time::Duration::from_secs(3));let during=c.view()?;c.stop()?;Ok(serde_json::json!({"during":during,"after":c.view()?}))})}else{c.view().and_then(|v|serde_json::to_value(v).map_err(|e|e.to_string()))};match result{Ok(v)=>println!("{}",serde_json::to_string_pretty(&v).unwrap()),Err(e)=>{eprintln!("{e}");std::process::exit(1)}}return;
    }
    if std::env::args().any(|a|a=="--mchose-hardware-probe"||a=="--mchose-hardware-self-test") {
        let test=std::env::args().any(|a|a=="--mchose-hardware-self-test");
        let result=(||->Result<serde_json::Value,String>{
            let mut m=Manager::new(events::sink()).map_err(|e|e.to_string())?;m.scan().map_err(|e|e.to_string())?;
            let d=m.devices().iter().find(|d|d.vendor_id==mchose::VID).ok_or("No MCHOSE mouse found")?;
            m.with_device(&d.id,|_,t|{
                let t=t.mchose().ok_or_else(||hidpp::Error::Other("Wrong backend".into()))?;
                if test{t.hardware_self_test()}else{serde_json::to_value(t.full_snapshot()?).map_err(|e|hidpp::Error::Other(e.to_string()))}
            }).map_err(|e|e.to_string())
        })();
        match result{Ok(v)=>println!("{}",serde_json::to_string_pretty(&v).unwrap()),Err(e)=>{eprintln!("{e}");std::process::exit(1)}}return;
    }
    // Reversible real-hardware validation, never performed at normal startup.
    if std::env::args().any(|a| a == "--mchose-self-test") {
        let result=(|| -> Result<serde_json::Value,String> {
            let mut manager=Manager::new(events::sink()).map_err(|e|e.to_string())?;
            manager.scan().map_err(|e|e.to_string())?;
            let d=manager.devices().iter().find(|d|d.vendor_id==mchose::VID).ok_or("No verified MCHOSE mouse connected")?;
            manager.with_device(&d.id,|_,t| {
                let t=t.mchose().ok_or_else(||hidpp::Error::Other("Wrong transport".into()))?;
                let original=t.dpi()?.ok_or_else(||hidpp::Error::Other("No DPI state".into()))?;
                let rate=t.report_rate()?.ok_or_else(||hidpp::Error::Other("No report-rate state".into()))?;
                if original.y.is_some(){return Err(hidpp::Error::Other("Self-test requires square DPI".into()))}
                let test_dpi=if original.x<50000 {original.x+50} else {original.x-50};
                let test_rate=if rate.current==2 {3} else {2};
                let trial=(|| -> hidpp::Result<()> {t.set_dpi(test_dpi,None,None)?;t.set_report_rate(test_rate)?;Ok(())})();
                // Restore both settings even when a trial command failed.
                let dpi_restore=t.set_dpi(original.x,None,None);
                let rate_restore=t.set_report_rate(rate.current);
                dpi_restore?; rate_restore?; trial?;
                let dpi=t.dpi()?.ok_or_else(||hidpp::Error::Other("Missing DPI after restore".into()))?;
                let final_rate=t.report_rate()?.ok_or_else(||hidpp::Error::Other("Missing rate after restore".into()))?;
                if dpi.x!=original.x || final_rate.current!=rate.current {return Err(hidpp::Error::Other("Restored state did not match".into()))}
                Ok(serde_json::json!({"passed":true,"original_dpi":original.x,"tested_dpi":test_dpi,"original_rate":rate.current,"tested_rate":test_rate,"restored":true}))
            }).map_err(|e|e.to_string())
        })();
        match result {Ok(r)=>println!("{}",r),Err(e)=>{eprintln!("MCHOSE self-test failed: {e}");std::process::exit(1)}}
        return;
    }
    if std::env::args().any(|a| a == "--probe") {
        return probe();
    }
    // Read-only: dump on-board profile memory as JSON (and raw sectors to a file).
    if let Some(pos) = std::env::args().position(|a| a == "--onboard-dump") {
        return onboard_dump(std::env::args().nth(pos + 1));
    }
    // Writes one on-board slot (backup first): --onboard-write <slot> <dpi,dpi,..> <start> <hz> <backup.json>
    if let Some(pos) = std::env::args().position(|a| a == "--onboard-write") {
        let a: Vec<String> = std::env::args().skip(pos + 1).take(5).collect();
        return onboard_cli(|d, t| {
            let [slot, dpis, start, hz, backup] = a.as_slice() else { return Err("usage: --onboard-write <slot> <dpi,..> <start> <hz> <backup.json>".into()) };
            let settings = onboard::SlotSettings {
                slot: slot.parse().map_err(|_| "bad slot")?,
                dpi_levels: dpis.split(',').map(|v| v.parse().map_err(|_| "bad dpi")).collect::<Result<_, _>>()?,
                default_index: start.parse().map_err(|_| "bad start index")?,
                report_rate_hz: hz.parse().map_err(|_| "bad rate")?,
            };
            let (snapshot, raw) = d.onboard_snapshot(t).map_err(|e| e.to_string())?;
            write_onboard_backup(env!("CARGO_PKG_VERSION"), std::path::Path::new(backup), d, &snapshot, &raw)?;
            let report = d.write_slot_settings(t, &settings).map_err(|e| e.to_string())?;
            Ok(serde_json::to_string_pretty(&report).unwrap_or_default())
        });
    }
    // Restores user sectors from a backup: --onboard-restore <backup.json>
    if let Some(pos) = std::env::args().position(|a| a == "--onboard-restore") {
        let path = std::env::args().nth(pos + 1).unwrap_or_default();
        return onboard_cli(move |d, t| {
            let file: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            let raw: Vec<onboard::RawSector> = serde_json::from_value(file["sectors"].clone()).map_err(|e| e.to_string())?;
            let sectors = onboard::restorable_sectors(&raw)?;
            let written = d.restore_sectors(t, &sectors).map_err(|e| e.to_string())?;
            Ok(format!("restored {written} sector(s)"))
        });
    }
    // Read-only: print what an import from G HUB would produce, without saving.
    if std::env::args().any(|a| a == "--ghub-profiles") {
        let mut store = ProfileStore::default();
        let result = profiles::ghub_settings_path()
            .ok_or_else(|| "LOCALAPPDATA not set".to_string())
            .and_then(|p| profiles::import_ghub(&mut store, &p));
        match result {
            Ok(s) => println!(
                "{}\nimported {}, updated {}, skipped {}",
                serde_json::to_string_pretty(&store.profiles).unwrap_or_default(),
                s.imported,
                s.updated,
                s.skipped
            ),
            Err(e) => eprintln!("{e}"),
        }
        return;
    }
    tauri::Builder::default()
        // Must be first: a second launch focuses this window instead of
        // starting another instance that would also talk to the mouse.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {if args.iter().any(|a|a=="--prepare-update"){let _=app.state::<effects::Engine>().stop();let _=app.state::<magdock::Controller>().stop();app.state::<rgb::Server>().shutdown();app.exit(0);}else{tray::show_main(app)}}))
        .plugin(tauri_plugin_autostart::Builder::new().args(["--minimized"]).build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(effects::Engine::default())
        .manage(rgb::Server::default())
        .manage(magdock::Controller::default())
        .manage(sinowealth::Service::default())
        .manage(multibrand::Service::default())
        .manage(macro_runtime::Runtime::default())
        .manage(updater::UpdateState::default())
        .manage(tray::BatteryCache::default())
        .manage(AppliedDevices::default())
        .manage(AppState(Mutex::new(None)))
        .setup(|app| {
            if std::env::args().any(|a|a=="--prepare-update"){app.handle().exit(0);return Ok(());}

            let path = app.path().app_data_dir()?.join("profiles.json");
            let store = ProfileStore::load(&path);
            app.manage(ProfilesState { store: Mutex::new(store), path });
            let settings_path = app.path().app_data_dir()?.join("settings.json");
            app.manage(settings::SettingsState::load(settings_path));
            // Before anything creates the device manager, so HID readers get a live sink.
            events::start(app.handle().clone());
            spawn_profile_watcher(app.handle().clone());
            tray::create(app.handle())?;
            // Only this fork's signed GitHub feed is used; background discovery never downloads.
            updater::spawn_background(app.handle().clone());
            // Started by "Start with Windows": stay in the tray.
            if !std::env::args().any(|a| a == "--minimized") {
                tray::show_main(app.handle());
            }
            Ok(())
        })
        // Closing the window destroys it (freeing WebView2); the app keeps
        // running in the tray, see the ExitRequested handler below.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                tray::tray_hint_once(window.app_handle());
            }
        })
        .invoke_handler(tauri::generate_handler![
            scan_devices,
            list_devices,
            device_state,
            set_dpi,
            set_report_rate,
            set_onboard_mode,
            rgb_devices,
            effects_preset,
            effects_status,
            effects_start,
            effects_stop,
            magdock_state,
            magdock_apply,
            magdock_music,
            magdock_reset,
            mchose_hardware,
            mchose_macro,
            mchose_macro_file,
            mchose_apply,
            mchose_export,
            mchose_restore,
            open_mhub,
            get_profiles,
            save_profile,
            delete_profile,
            set_auto_switch,
            activate_profile,
            import_ghub_profiles,
            import_mhub_profiles,
            sino_read,sino_apply,sino_models,sino_simulate,multi_read,multi_apply,multi_templates,multi_simulate,multi_macro_simulate,multi_input_sources,multi_macro_status,multi_stop_macros,multi_stop_device_macros,multi_recovery_list,multi_recovery_observe,multi_recovery_restore,multi_reconnect_list,multi_reconnect_candidates,multi_reconnect_resume,multi_reconnect_retry,multi_reconnect_discard,preview_synapse_import,prepare_synapse_import,prepare_icue_import,prepare_gg_import,
            preview_profile_import,commit_profile_import,export_profile_pack,
            onboard_snapshot,
            save_onboard_backup,
            write_onboard_slot,
            restore_onboard_backup,
            get_app_info,
            set_app_settings,
            appearance::export_ui_skin,
            check_for_update,
            install_update,
            download_update
        ])
        .build(tauri::generate_context!())
        .expect("error while building Open HUB")
        .run(|_app, event| {
            if matches!(event,tauri::RunEvent::ExitRequested{code:Some(_),..}|tauri::RunEvent::Exit){prepare_shutdown(_app);}
            // The last window closing would normally quit; stay in the tray
            // instead. Quit from the tray menu passes an exit code and goes through.
            if let tauri::RunEvent::ExitRequested { api, code: None, .. } = event {
                api.prevent_exit();
            }
        });
}
