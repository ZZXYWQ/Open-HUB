//! Feature-only Windows HID handle, opened with zero requested access.
//! Razer protected mouse collections reject read/write handles but permit
//! HidD_GetFeature/SetFeature, as documented by the MIT Sidearm implementation.
#[cfg(windows)]
mod win{
 use gearwise_core::{HidTransport,Result,Error};
 use windows_sys::Win32::{Foundation::{HANDLE,INVALID_HANDLE_VALUE,CloseHandle},Storage::FileSystem::CreateFileW,Devices::HumanInterfaceDevice::{HidD_GetFeature,HidD_SetFeature}};
 pub struct FeatureOnly(HANDLE);
 // A HID handle can move between threads; &mut self serializes all exchanges.
 unsafe impl Send for FeatureOnly{}
 impl FeatureOnly{pub fn open(path:&str)->Result<Self>{let wide=path.encode_utf16().chain([0]).collect::<Vec<_>>();let handle=unsafe{CreateFileW(wide.as_ptr(),0,3,std::ptr::null(),3,0x80,std::ptr::null_mut())};if handle==INVALID_HANDLE_VALUE{return Err(Error::transport(format!("Feature-only HID open: {}",std::io::Error::last_os_error())));}Ok(Self(handle))}}
 impl HidTransport for FeatureOnly{
  fn write(&mut self,_:&[u8])->Result<usize>{Err(Error::Unsupported("output report on feature-only handle"))}
  fn read_timeout(&mut self,_:&mut[u8],_:i32)->Result<usize>{Err(Error::Unsupported("input read on feature-only handle"))}
  fn send_feature_report(&mut self,data:&[u8])->Result<()>{if unsafe{HidD_SetFeature(self.0,data.as_ptr().cast(),data.len() as u32)}{Ok(())}else{Err(Error::transport(format!("HidD_SetFeature: {}",std::io::Error::last_os_error())))}}
  fn get_feature_report(&mut self,data:&mut[u8])->Result<usize>{if unsafe{HidD_GetFeature(self.0,data.as_mut_ptr().cast(),data.len() as u32)}{Ok(data.len())}else{Err(Error::transport(format!("HidD_GetFeature: {}",std::io::Error::last_os_error())))}}
 }
 impl Drop for FeatureOnly{fn drop(&mut self){unsafe{CloseHandle(self.0);}}}
}
#[cfg(windows)]pub use win::FeatureOnly;
