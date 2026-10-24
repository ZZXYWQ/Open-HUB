//! Optional background discovery from this fork's GitHub feed. Download and
//! installation are separate, explicit user actions. Tauri verifies signatures.
use std::{sync::{Mutex,atomic::{AtomicBool,Ordering}},time::Duration,path::PathBuf};
use serde::Serialize;
use tauri::{AppHandle,Emitter,Manager};
use tauri_plugin_updater::UpdaterExt as _;
use crate::{settings::SettingsState,tray};
const FIRST_CHECK:Duration=Duration::from_secs(30);
const CHECK_EVERY:Duration=Duration::from_secs(6*60*60);
const DOWNLOAD_PREFIX:&str="https://github.com/ZZXYWQ/Open-HUB/releases/download/";
#[derive(Clone,Serialize)]
pub struct UpdateInfo{
 pub version:String,pub current_version:String,pub notes:Option<String>,pub date:Option<String>,
 pub release_url:String,pub experimental:bool,pub downloaded:bool,
}
#[derive(Clone,Serialize)]
struct Progress{downloaded:u64,total:Option<u64>}
struct Pending{update:tauri_plugin_updater::Update,path:PathBuf}
#[derive(Default)]
pub struct UpdateState{
 available:Mutex<Option<UpdateInfo>>,notified:Mutex<Option<String>>,
 pending:Mutex<Option<Pending>>,busy:AtomicBool,
}
impl UpdateState{pub fn available(&self)->Option<UpdateInfo>{self.available.lock().ok().and_then(|v|v.clone())}}
struct Busy<'a>(&'a AtomicBool);
impl Drop for Busy<'_>{fn drop(&mut self){self.0.store(false,Ordering::SeqCst);}}
fn begin(busy:&AtomicBool)->Result<Busy<'_>,String>{
 busy.compare_exchange(false,true,Ordering::SeqCst,Ordering::SeqCst).map_err(|_|"更新操作正在进行，请稍候".to_string())?;Ok(Busy(busy))
}
fn release_url(url:&str,version:&str)->Result<(String,bool),String>{
 if version.is_empty()||version.len()>64||!version.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'.'||b==b'-'){
  return Err("更新版本号无效".into());
 }
 let tail=url.strip_prefix(DOWNLOAD_PREFIX).ok_or("更新下载地址不属于 ZZXYWQ/Open-HUB")?;
 let parts:Vec<_>=tail.split('/').collect();
 if parts.len()!=2||parts[1]!=format!("Open-HUB-{version}-x64-setup.exe")||
   !(parts[0]==format!("v{version}")||parts[0].strip_prefix(&format!("v{version}-experimental.")).is_some_and(|n|!n.is_empty()&&n.bytes().all(|b|b.is_ascii_digit()))){
  return Err("更新标签、版本和安装文件不一致".into());
 }
 Ok((format!("https://github.com/ZZXYWQ/Open-HUB/releases/tag/{}",parts[0]),parts[0].contains("-experimental.")))
}
fn cache_path(app:&AppHandle,version:&str)->Result<PathBuf,String>{
 // release_url validated version before it is used as a filename.
 Ok(app.path().app_cache_dir().map_err(|e|e.to_string())?.join("updates").join(format!("Open-HUB-{version}-x64-setup.exe")))
}
fn info(update:&tauri_plugin_updater::Update,downloaded:bool)->Result<UpdateInfo,String>{
 let (release_url,experimental)=release_url(update.download_url.as_str(),&update.version)?;
 if update.signature.trim().is_empty(){return Err("更新缺少签名".into());}
 Ok(UpdateInfo{version:update.version.clone(),current_version:update.current_version.clone(),notes:update.body.clone(),date:update.date.map(|d|d.to_string()),release_url,experimental,downloaded})
}
pub async fn check(app:&AppHandle)->Result<Option<UpdateInfo>,String>{
 let cleanup=app.clone();
 let update=app.updater_builder().on_before_exit(move||crate::prepare_shutdown(&cleanup)).timeout(Duration::from_secs(25)).build().map_err(|e|e.to_string())?.check().await.map_err(|e|format!("检查 GitHub 更新失败：{e}"))?;
 let found=if let Some(update)=update{
  let mut found=info(&update,false)?;let path=cache_path(app,&update.version)?;
  if path.is_file(){found.downloaded=true;*app.state::<UpdateState>().pending.lock().map_err(|_|"更新缓存锁失败")?=Some(Pending{update,path});}
  Some(found)
 }else{None};
 *app.state::<UpdateState>().available.lock().map_err(|_|"更新状态锁失败")?=found.clone();
 tray::refresh(app);
 if let Some(found)=&found{let _=app.emit("update-available",found.clone());}
 Ok(found)
}
pub async fn download(app:&AppHandle,expected_version:&str)->Result<UpdateInfo,String>{
 let state=app.state::<UpdateState>();let _busy=begin(&state.busy)?;
 if !state.available().is_some_and(|v|v.version==expected_version){return Err("请先检查更新并选择明确的版本".into());}
 let cleanup=app.clone();
 let update=app.updater_builder().on_before_exit(move||crate::prepare_shutdown(&cleanup)).timeout(Duration::from_secs(60)).build().map_err(|e|e.to_string())?.check().await.map_err(|e|format!("获取更新失败：{e}"))?.ok_or("没有可下载的新版本")?;
 if update.version!=expected_version{return Err("GitHub 上的新版本已变化，请重新检查后选择下载".into());}
 let mut found=info(&update,false)?;let mut downloaded=0;
 let bytes=update.download(|chunk,total|{downloaded+=chunk as u64;let _=app.emit("update-progress",Progress{downloaded,total});},||{}).await.map_err(|e|format!("下载或签名验证失败：{e}"))?;
 // download() already verified the exact installer; install() verifies it again.
 let path=cache_path(app,&update.version)?;let parent=path.parent().ok_or("更新缓存路径无效")?;
 std::fs::create_dir_all(parent).map_err(|e|e.to_string())?;
 let temp=path.with_extension("exe.download");std::fs::write(&temp,&bytes).map_err(|e|e.to_string())?;
 if path.exists(){std::fs::remove_file(&path).map_err(|e|e.to_string())?;}
 std::fs::rename(&temp,&path).map_err(|e|e.to_string())?;
 let previous=state.pending.lock().map_err(|_|"更新缓存锁失败")?.replace(Pending{update,path:path.clone()});
 if let Some(previous)=previous{if previous.path!=path&&previous.path.parent()==Some(parent){let _=std::fs::remove_file(previous.path);}}
 found.downloaded=true;*state.available.lock().map_err(|_|"更新状态锁失败")?=Some(found.clone());
 tray::refresh(app);let _=app.emit("update-downloaded",found.clone());Ok(found)
}
pub fn install(app:&AppHandle,expected_version:&str)->Result<(),String>{
 let state=app.state::<UpdateState>();let _busy=begin(&state.busy)?;
 let pending=state.pending.lock().map_err(|_|"更新缓存锁失败")?;
 let pending=pending.as_ref().ok_or("请先选择下载更新，下载完成后再安装")?;
 if pending.update.version!=expected_version{return Err("下载的版本与所选版本不同，请重新检查".into());}
 info(&pending.update,true)?;
 let bytes=std::fs::read(&pending.path).map_err(|e|format!("无法读取已下载更新：{e}"))?;
 pending.update.install(&bytes).map_err(|e|format!("签名验证或安装失败：{e}"))
}
fn announce(app:&AppHandle,info:&UpdateInfo){
 let state=app.state::<UpdateState>();let Ok(mut notified)=state.notified.lock()else{return};
 if notified.as_deref()==Some(&info.version){return;}
 *notified=Some(info.version.clone());
 tray::notify(app,&format!("Open HUB {}{} 可用",info.version,if info.experimental{" 实验版"}else{""}),"进入外观与设置查看说明，自行选择下载、稍后或安装。");
}
pub fn spawn_background(app:AppHandle){
 std::thread::Builder::new().name("updater".into()).spawn(move||{
  std::thread::sleep(FIRST_CHECK);
  loop{if app.state::<SettingsState>().get().check_updates{
   match tauri::async_runtime::block_on(check(&app)){Ok(Some(found))=>announce(&app,&found),Ok(None)=>{},Err(e)=>eprintln!("{e}")}
  }std::thread::sleep(CHECK_EVERY);}
 }).expect("could not start updater thread");
}
pub fn check_from_tray(app:&AppHandle){let app=app.clone();std::thread::spawn(move||match tauri::async_runtime::block_on(check(&app)){
 Ok(Some(found))=>announce(&app,&found),Ok(None)=>tray::notify(&app,"Open HUB 已是最新版本",&format!("当前 {}",app.package_info().version)),Err(e)=>tray::notify(&app,"检查更新失败",&e),
});}
pub fn install_from_tray(app:&AppHandle){let app=app.clone();std::thread::spawn(move||{
 let Some(found)=app.state::<UpdateState>().available()else{return};
 let result=if found.downloaded{install(&app,&found.version)}else{
  tauri::async_runtime::block_on(download(&app,&found.version)).map(|_|tray::notify(&app,"更新已下载并验证","选择安装更新以重启，或稍后从设置安装。"))
 };
 if let Err(e)=result{tray::notify(&app,"更新操作失败",&e);}
});}
#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn updater_accepts_only_own_versioned_release_assets(){
  for tag in ["v0.3.2","v0.3.2-experimental.1"]{assert!(release_url(&format!("{DOWNLOAD_PREFIX}{tag}/Open-HUB-0.3.2-x64-setup.exe"),"0.3.2").is_ok());}
  for url in ["https://github.com/abidr/open-ghub/releases/download/v0.3.2/Open-HUB-0.3.2-x64-setup.exe", "https://github.com/ZZXYWQ/Open-HUB/releases/download/v9.9.9/Open-HUB-0.3.2-x64-setup.exe","https://github.com/ZZXYWQ/Open-HUB/releases/download/v0.3.2/Open-HUB-0.3.2-x64-setup.exe?x=1","http://github.com/ZZXYWQ/Open-HUB/releases/download/v0.3.2/Open-HUB-0.3.2-x64-setup.exe"]{assert!(release_url(url,"0.3.2").is_err());}
  assert!(release_url("","../bad").is_err());
 }
 #[test]fn updater_serializes_actions_and_resets_after_failure(){let flag=AtomicBool::new(false);{let _guard=begin(&flag).unwrap();assert!(begin(&flag).is_err());}assert!(begin(&flag).is_ok());}
 #[test]fn updater_config_uses_own_feed_and_dedicated_public_key(){
  let c:serde_json::Value=serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
  assert_eq!(c["plugins"]["updater"]["endpoints"][0],"https://raw.githubusercontent.com/ZZXYWQ/Open-HUB/main/updates/latest.json");
  assert!(!c["plugins"]["updater"]["pubkey"].as_str().unwrap().is_empty());
  assert_eq!(c["plugins"]["updater"]["requireSignedVersion"],true);
 }
}

/// Read-only release acceptance. Uses the configured production key/feed and never installs.
pub async fn verify_published(app:&AppHandle)->Result<serde_json::Value,String>{
 let mut update=app.updater_builder().timeout(Duration::from_secs(60)).version_comparator(|_,_|true).build().map_err(|e|e.to_string())?.check().await.map_err(|e|e.to_string())?.ok_or("Release feed is empty")?;
 let metadata=info(&update,false)?;let bytes=update.download(|_,_|{},||{}).await.map_err(|e|e.to_string())?;
 update.verify(&bytes).map_err(|e|e.to_string())?;
 let mut corrupt=bytes.clone();let last=corrupt.last_mut().ok_or("Empty installer")?;*last^=1;
 let tamper_rejected=update.verify(&corrupt).is_err();update.version="999.0.0".into();let version_rejected=update.verify(&bytes).is_err();
 if !tamper_rejected||!version_rejected{return Err("Release verification did not reject a tampered file/version".into());}
 Ok(serde_json::json!({"passed":true,"version":metadata.version,"release_url":metadata.release_url,"bytes":bytes.len(),"download_signature_valid":true,"tamper_rejected":tamper_rejected,"signed_version_mismatch_rejected":version_rejected,"installed":false,"hardware_accessed":false}))
}
