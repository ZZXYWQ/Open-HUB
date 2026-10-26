//! Read-only per-device Windows Raw Input collection. No low-level/global
//! keyboard hook, no legacy-input suppression and no desktop input injection.
use crate::host_macros::Source;
use serde::Serialize;
use std::sync::Arc;
#[derive(Clone,Debug,Serialize)]pub struct SourceInfo{pub source:Source,pub kind:String,pub serial:Option<String>}
#[derive(Clone,Debug,PartialEq,Eq)]pub enum Event{Key{source:Source,vkey:u16,down:bool},Buttons{source:Source,mask:u32},Removed{path:String},Arrived{source:Source},InventoryUncertain,Stopped}
pub type Callback=Arc<dyn Fn(Event)+Send+Sync>;
#[cfg(any(windows,test))]
#[path="raw_input_inventory.rs"]
mod inventory;
#[cfg(any(windows,test))]
#[path="raw_input_startup.rs"]
mod startup;
pub fn parse_source(path:&str)->Option<Source>{
 if path.len()>4096||path.contains('\0'){return None;}let lower=path.to_ascii_lowercase();
 fn word(s:&str,tag:&str)->Option<u16>{let at=s.find(tag)?+tag.len();let end=at.checked_add(4)?;let value=s.get(at..end)?;if s.as_bytes().get(end).is_some_and(|b|b.is_ascii_hexdigit()){return None;}u16::from_str_radix(value,16).ok()}
 let vendor=word(&lower,"vid_")?;let product=word(&lower,"pid_")?;if !crate::multibrand::supported(vendor,product){return None;}Some(Source{vendor,product,path:lower})
}
pub fn corsair_mask(report:&[u8])->Option<u32>{if report.first()!=Some(&3)||report.len()<4||report.len()>64{return None;}let mut bytes=[0;4];for(i,b)in report[1..].iter().take(4).enumerate(){bytes[i]=*b;}Some(u32::from_le_bytes(bytes)&0x01ff_ffff)}
#[cfg(windows)]mod win{
 use super::*;use std::{mem::{size_of,offset_of},ptr,sync::Mutex};
 use super::{inventory::Inventory,startup::Startup};
 use windows_sys::Win32::{Foundation::{HANDLE,HWND,LPARAM,LRESULT,WPARAM},UI::{Input::*,WindowsAndMessaging::*},System::LibraryLoader::GetModuleHandleW};
 fn name(handle:HANDLE)->Option<String>{let mut count=0;unsafe{GetRawInputDeviceInfoW(handle,RIDI_DEVICENAME,ptr::null_mut(),&mut count)};if count==0||count>4096{return None;}let mut data=vec![0u16;count as usize+1];let n=unsafe{GetRawInputDeviceInfoW(handle,RIDI_DEVICENAME,data.as_mut_ptr().cast(),&mut count)};if n==u32::MAX{return None;}let length=data.iter().position(|v|*v==0).unwrap_or(n as usize);Some(String::from_utf16_lossy(&data[..length]))}
 pub fn sources()->Result<Vec<SourceInfo>,String>{
  let mut count=0;let n=unsafe{GetRawInputDeviceList(ptr::null_mut(),&mut count,size_of::<RAWINPUTDEVICELIST>() as u32)};if n==u32::MAX||count>16384{return Err("Raw Input 设备枚举失败".into());}
  let mut devices=vec![RAWINPUTDEVICELIST::default();count as usize];let n=unsafe{GetRawInputDeviceList(devices.as_mut_ptr(),&mut count,size_of::<RAWINPUTDEVICELIST>() as u32)};if n==u32::MAX{return Err("Raw Input 设备列表已变化，请重试".into());}
  let api=hidapi::HidApi::new().ok();let mut result=Vec::new();for item in devices.into_iter().take(n as usize){let Some(path)=name(item.hDevice)else{continue};let Some(source)=parse_source(&path)else{continue};let serial=api.as_ref().and_then(|api|api.device_list().find(|d|d.path().to_string_lossy().eq_ignore_ascii_case(&path)).and_then(|d|d.serial_number().map(str::to_owned)));result.push(SourceInfo{source,kind:match item.dwType{RIM_TYPEKEYBOARD=>"Keyboard report",RIM_TYPEHID=>"Vendor HID",_=>"Mouse report"}.into(),serial});}Ok(result)
 }
 fn initial_sources()->Result<Inventory,String> {
  let mut count=0;let mut result=Inventory::default();
  if unsafe{GetRawInputDeviceList(ptr::null_mut(),&mut count,size_of::<RAWINPUTDEVICELIST>()as u32)}==u32::MAX||count>16384 { return Err("输入监听初始设备枚举失败".into()); }
  let mut devices=vec![RAWINPUTDEVICELIST::default();count as usize];
  let n=unsafe{GetRawInputDeviceList(devices.as_mut_ptr(),&mut count,size_of::<RAWINPUTDEVICELIST>()as u32)};
  if n==u32::MAX { return Err("输入监听初始设备列表已变化，请重试".into()); }
  for item in devices.into_iter().take(n as usize) {
   let source=name(item.hDevice).map(|path|parse_source(&path)).ok_or(());
   result.seed(item.hDevice as usize,source);
  }
  Ok(result)
 }
 struct Data { callback:Callback,known:Mutex<Inventory> }
 fn dispatch(data:&Data,events:Vec<Event>) { for event in events { (data.callback)(event); } }
 struct ListenerGuard(Callback);
 impl Drop for ListenerGuard { fn drop(&mut self) { (self.0)(Event::Stopped); } }
 unsafe extern "system" fn procedure(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{
  unsafe{
   if msg==WM_NCCREATE{let create=&*(l as *const CREATESTRUCTW);SetWindowLongPtrW(hwnd,GWLP_USERDATA,create.lpCreateParams as isize);return 1;}
   let data=GetWindowLongPtrW(hwnd,GWLP_USERDATA) as *mut Data;
   if !data.is_null(){let data=&*data;
    if msg==WM_INPUT{let mut bytes=0u32;GetRawInputData(l as HRAWINPUT,RID_INPUT,ptr::null_mut(),&mut bytes,size_of::<RAWINPUTHEADER>() as u32);
     if (size_of::<RAWINPUTHEADER>()..=1024*1024).contains(&(bytes as usize)){let mut aligned=vec![0u64;(bytes as usize).max(size_of::<RAWINPUT>()).div_ceil(8)];let n=GetRawInputData(l as HRAWINPUT,RID_INPUT,aligned.as_mut_ptr().cast(),&mut bytes,size_of::<RAWINPUTHEADER>() as u32);
      if n!=u32::MAX&&n as usize>=size_of::<RAWINPUTHEADER>(){let raw=&*(aligned.as_ptr() as *const RAWINPUT);let handle=raw.header.hDevice;
       if !handle.is_null()&&raw.header.dwSize<=n{let source=data.known.lock().ok().and_then(|m|m.get(handle as usize)).or_else(||{
        let source_name=name(handle);let source=source_name.as_deref().and_then(parse_source);
        let events=data.known.lock().map(|mut m|if source_name.is_some(){m.observe(handle as usize,source.clone(),false)}else{m.unresolved(handle as usize,false)}).unwrap_or_else(|_|vec![Event::InventoryUncertain]);
        dispatch(data,events);source
       });
        if let Some(source)=source{
         let payload=offset_of!(RAWINPUT,data);match raw.header.dwType{
          RIM_TYPEKEYBOARD if n as usize>=payload+size_of::<RAWKEYBOARD>()=>{let key=raw.data.keyboard;if (0x7c..=0x87).contains(&key.VKey){(data.callback)(Event::Key{source,vkey:key.VKey,down:key.Flags&1==0});}},
          RIM_TYPEHID if n as usize>=payload+8=>{let hid=raw.data.hid;let total=(hid.dwSizeHid as usize).checked_mul(hid.dwCount as usize);if let Some(total)=total.filter(|t|*t<=n as usize-payload-8){if hid.dwSizeHid>0&&hid.dwSizeHid<=64&&source.vendor==0x1b1c{let p=(aligned.as_ptr() as *const u8).add(payload+8);let reports=std::slice::from_raw_parts(p,total);for report in reports.chunks_exact(hid.dwSizeHid as usize){if let Some(mask)=corsair_mask(report){(data.callback)(Event::Buttons{source:source.clone(),mask});}}}}},_=>{}}
        }
       }
      }
     }
     return DefWindowProcW(hwnd,msg,w,l);
    }
    if msg==WM_INPUT_DEVICE_CHANGE&&w==GIDC_ARRIVAL as usize{
     let path=name(l as HANDLE);
     let events=data.known.lock().map(|mut m|match path{Some(path)=>m.observe(l as usize,parse_source(&path),true),None=>m.unresolved(l as usize,true)}).unwrap_or_else(|_|vec![Event::InventoryUncertain]);
     dispatch(data,events);
    }
    if msg==WM_INPUT_DEVICE_CHANGE&&w==GIDC_REMOVAL as usize{
     let events=data.known.lock().map(|mut m|m.remove(l as usize)).unwrap_or_else(|_|vec![Event::InventoryUncertain]);
     dispatch(data,events);
    }
   }
   if msg==WM_CLOSE{DestroyWindow(hwnd);return 0;}
   if msg==WM_DESTROY{PostQuitMessage(0);return 0;}
   if msg==WM_NCDESTROY&&!data.is_null(){SetWindowLongPtrW(hwnd,GWLP_USERDATA,0);}
   DefWindowProcW(hwnd,msg,w,l)
  }
 }
 pub struct Watcher{hwnd:usize,thread:Option<std::thread::JoinHandle<()>>}
 impl Watcher{pub fn start(callback:Callback)->Result<Self,String>{
  let startup=Arc::new(Startup::default());let worker_startup=startup.clone();
  let(tx,rx)=std::sync::mpsc::sync_channel(1);let thread=std::thread::Builder::new().name("open-hub-device-input".into()).spawn(move||unsafe{
   let class="OpenHubDeviceInput\0".encode_utf16().collect::<Vec<_>>();let instance=GetModuleHandleW(ptr::null());let wc=WNDCLASSW{lpfnWndProc:Some(procedure),hInstance:instance,lpszClassName:class.as_ptr(),..Default::default()};RegisterClassW(&wc);
   let _termination=ListenerGuard(callback.clone());
   let mut data=Box::new(Data{callback,known:Mutex::new(Inventory::default())});let data_ptr=data.as_mut() as *mut Data;let hwnd=CreateWindowExW(0,class.as_ptr(),class.as_ptr(),0,0,0,0,0,HWND_MESSAGE,ptr::null_mut(),instance,data_ptr.cast());if hwnd.is_null(){let _=tx.send(Err("无法创建设备输入窗口".to_string()));return;}
   let mut regs=vec![RAWINPUTDEVICE{usUsagePage:1,usUsage:6,dwFlags:RIDEV_INPUTSINK|RIDEV_DEVNOTIFY,hwndTarget:hwnd}];for page in [0xff00,0xff01,0xff42,0xffc2]{regs.push(RAWINPUTDEVICE{usUsagePage:page,usUsage:0,dwFlags:RIDEV_PAGEONLY|RIDEV_INPUTSINK|RIDEV_DEVNOTIFY,hwndTarget:hwnd});}
   let registered=worker_startup.register(||if RegisterRawInputDevices(regs.as_ptr(),regs.len() as u32,size_of::<RAWINPUTDEVICE>() as u32)==0{Err(format!("注册设备输入失败：{}",std::io::Error::last_os_error()))}else{Ok(())});
   if let Err(error)=registered{let _=tx.send(Err(error));DestroyWindow(hwnd);return;}
   match initial_sources(){Ok(initial)=>{if let Ok(mut known)=data.known.lock(){*known=initial;}else{let _=tx.send(Err("输入设备列表锁异常".into()));DestroyWindow(hwnd);return;}},Err(error)=>{let _=tx.send(Err(error));DestroyWindow(hwnd);return;}}
   if tx.send(Ok(hwnd as usize)).is_err(){DestroyWindow(hwnd);return;}let mut msg=MSG::default();while GetMessageW(&mut msg,ptr::null_mut(),0,0)>0{TranslateMessage(&msg);DispatchMessageW(&msg);}DestroyWindow(hwnd);
  }).map_err(|e|e.to_string())?;
  match rx.recv_timeout(std::time::Duration::from_secs(5)){
   Ok(Ok(hwnd))=>Ok(Self{hwnd,thread:Some(thread)}),
   Ok(Err(error))=>{startup.cancel();let _=thread.join();Err(error)},
   Err(error)=>{startup.cancel();Err(format!("输入监听启动未完成：{error}"))},
  }
 }}
 impl Drop for Watcher{fn drop(&mut self){unsafe{PostMessageW(self.hwnd as HWND,WM_CLOSE,0,0);}if let Some(t)=self.thread.take(){let _=t.join();}}}
}
#[cfg(windows)]pub use win::{sources,Watcher};
#[cfg(not(windows))]pub fn sources()->Result<Vec<SourceInfo>,String>{Err("Raw Input requires Windows".into())}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn source_parser_never_claims_other_brands(){assert!(parse_source(r"\\?\HID#VID_046D&PID_1720#A").is_none());assert!(parse_source(r"\\?\HID#VID_3837&PID_00B4#A").is_none());assert!(parse_source(r"\\?\HID#VID_1532&PID_FFFF#A").is_none());}
 #[test]fn source_identity_keeps_physical_path(){let a=parse_source(r"\\?\HID#VID_1038&PID_1720#DeviceA").unwrap();let b=parse_source(r"\\?\HID#VID_1038&PID_1720#DeviceB").unwrap();assert_ne!(a,b);assert_eq!(a.path,r"\\?\hid#vid_1038&pid_1720#devicea");}
 #[test]fn rejects_truncated_or_extended_ids(){assert!(parse_source("vid_103&pid_1720").is_none());assert!(parse_source("vid_1038f&pid_1720").is_none());}
 #[test]fn corsair_report_decodes_physical_mask_and_rejects_other_reports(){assert_eq!(corsair_mask(&[3,0x40,0x02,0]),Some(0x240));assert_eq!(corsair_mask(&[1,0x40,0x02,0]),None);assert_eq!(corsair_mask(&[3,0x40]),None);}
}

