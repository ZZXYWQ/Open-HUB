//! Naga V2 HyperSpeed only (1532:00B4), feature report 3, transaction 1F.
//! Command-class button protocol independently implemented from the MIT
//! Sidearm source/captured record fixtures; no assumption about other Razers.
use std::{collections::BTreeMap,sync::{Arc,Mutex},time::Duration};
use gearwise_core::{Device,Driver,Capabilities,DeviceDescriptor,DeviceState,Setting,Error,Result,HidBackend,HidInterfaceInfo,HidTransport};
use gearwise_core::model::{Vendor,DeviceKind,Connection,DpiCaps,DpiState,DpiStage,ButtonCaps,ButtonAction,MouseButton,Modifiers};
const IDS:[u8;19]=[1,2,3,0x34,0x35,0x0b,0x0c,0x40,0x41,0x42,0x43,0x44,0x45,0x46,0x47,0x48,0x49,0x4a,0x4b];
pub fn known_button(id:u16)->bool{IDS.contains(&(id as u8))&&id<=255}
fn rate_code(hz:u16)->Result<u8>{match hz{1000=>Ok(1),500=>Ok(2),125=>Ok(8),_=>Err(Error::invalid("Naga polling rate"))}}
fn crc(p:&[u8])->u8{p[2..88].iter().fold(0,|a,b|a^b)}
pub fn packet(class:u8,command:u8,args:&[u8])->Result<Vec<u8>>{if args.len()>80{return Err(Error::invalid("Naga argument length"));}let mut p=vec![0;91];p[0]=3;p[2]=0x1f;p[6]=args.len() as u8;p[7]=class;p[8]=command;p[9..9+args.len()].copy_from_slice(args);p[89]=crc(&p[1..]);Ok(p)}
struct Naga{io:Box<dyn HidTransport>,desc:DeviceDescriptor,caps:Capabilities,state:DeviceState,mode:u8}
impl Naga{
 fn exchange(&mut self,class:u8,cmd:u8,args:&[u8])->Result<Vec<u8>>{
  let req=packet(class,cmd,args)?;
  for _ in 0..5{self.io.send_feature_report(&req)?;std::thread::sleep(Duration::from_millis(2));let mut reply=vec![0;91];reply[0]=3;let n=self.io.get_feature_report(&mut reply)?;
   if n!=91||reply[0]!=3||reply[2]!=0x1f||reply[3..6]!=req[3..6]||reply[7..9]!=req[7..9]||reply[6]>80||reply[89]!=crc(&reply[1..]){return Err(Error::invalid("Naga response CRC/identity/length"));}
   match reply[1]{2=>return Ok(reply[9..9+reply[6] as usize].to_vec()),1=>std::thread::sleep(Duration::from_millis(4)),s=>return Err(Error::invalid(format!("Naga command {class:02X}/{cmd:02X} status {s:02X}")))}
  }Err(Error::Timeout)
 }
 fn open(io:Box<dyn HidTransport>,iface:&HidInterfaceInfo)->Result<Self>{
  let unique=iface.serial.as_deref().filter(|s|!s.is_empty()).unwrap_or(&iface.path);
  let mut this=Self{io,desc:DeviceDescriptor{id:format!("razer:00b4:{unique}"),model_key:"razer:00b4".into(),vendor:Vendor::Razer,kind:DeviceKind::Mouse,name:"Razer Naga V2 HyperSpeed".into(),vendor_id:0x1532,product_id:0x00b4,connection:Connection::Wireless,firmware:None,serial:iface.serial.clone(),simulated:false},caps:Capabilities{dpi:Some(DpiCaps{min:100,max:30000,step:1,max_stages:1,separate_xy:true}),report_rates:vec![125,500,1000],buttons:IDS.iter().map(|id|ButtonCaps{id:u16::from(*id),name:match id{1=>"左键".into(),2=>"右键".into(),3=>"中键".into(),0x34=>"后退".into(),0x35=>"前进".into(),0x0b=>"顶部辅助 1".into(),0x0c=>"顶部辅助 2".into(),_=>format!("侧键 {}",id-0x40+1)},remappable:*id!=1,default_action:ButtonAction::Default,hotspot:None}).collect(),..Default::default()},state:DeviceState::default(),mode:0};
  this.refresh()?;Ok(this)
 }
 fn button(&mut self,id:u8)->Result<Vec<u8>>{let mut args=[0;10];args[0]=1;args[1]=id;let r=self.exchange(2,0x8c,&args)?;if r.len()!=10||r[..3]!=args[..3]||r[4]>5{return Err(Error::invalid("Naga button identity/length"));}Ok(r)}
 fn refresh(&mut self)->Result<()>{
  self.mode=*self.exchange(0,0x84,&[0,0])?.first().ok_or(Error::invalid("Naga mode"))?;
  let r=self.exchange(4,0x85,&[1,0,0,0,0,0,0])?;if r.len()<5{return Err(Error::invalid("Naga DPI length"));}let x=u16::from_be_bytes([r[1],r[2]]);let y=u16::from_be_bytes([r[3],r[4]]);if !(100..=30000).contains(&x)||!(100..=30000).contains(&y){return Err(Error::invalid("Naga DPI readback"));}self.state.dpi=Some(DpiState{stages:vec![DpiStage{x,y}],active:0});
  let r=self.exchange(0,0x85,&[0])?;self.state.report_rate=Some(match r.first(){Some(1)=>1000,Some(2)=>500,Some(8)=>125,_=>return Err(Error::invalid("Naga rate code"))});
  self.state.buttons.clear();
  for id in IDS{if let Ok(r)=self.button(id){let a=match(r[3],r[4]){(1,1)=>match r[5]{1=>Some(ButtonAction::Mouse(MouseButton::Left)),2=>Some(ButtonAction::Mouse(MouseButton::Right)),3=>Some(ButtonAction::Mouse(MouseButton::Middle)),4=>Some(ButtonAction::Mouse(MouseButton::Back)),5=>Some(ButtonAction::Mouse(MouseButton::Forward)),_=>None},(2,2)=>Some(ButtonAction::Key{usage:r[6],mods:Modifiers(r[5])}),_=>None};if let Some(a)=a{self.state.buttons.insert(u16::from(id),a);}}}
  Ok(())
 }
}
impl Device for Naga{
 fn descriptor(&self)->&DeviceDescriptor{&self.desc}fn capabilities(&self)->&Capabilities{&self.caps}fn read_state(&mut self)->Result<DeviceState>{self.refresh()?;Ok(self.state.clone())}
 fn apply(&mut self,s:&Setting)->Result<()>{
  if self.mode!=0{return Err(Error::invalid("Naga is in Synapse driver mode; close Synapse and restore normal device mode before applying"));}
  match s{
   Setting::Dpi(d)=>{if d.stages.len()!=1||d.active!=0{return Err(Error::invalid("Naga supports one current DPI value"));}let p=d.stages[0];if !(100..=30000).contains(&p.x)||!(100..=30000).contains(&p.y){return Err(Error::invalid("Naga DPI range"));}let[xh,xl]=p.x.to_be_bytes();let[yh,yl]=p.y.to_be_bytes();self.exchange(4,5,&[1,xh,xl,yh,yl,0,0])?;let r=self.exchange(4,0x85,&[1,0,0,0,0,0,0])?;if r.len()<5||r[1..5]!=[xh,xl,yh,yl]{return Err(Error::invalid("Naga DPI write did not read back"));}}
   Setting::ReportRate(hz)=>{let code=rate_code(*hz)?;self.exchange(0,5,&[code])?;if self.exchange(0,0x85,&[0])?.first()!=Some(&code){return Err(Error::invalid("Naga rate did not read back"));}}
   Setting::Button{id,action}=>{
    if !known_button(*id)||*id==1{return Err(Error::invalid("Unknown Naga button or protected left click"));}
    let bytes=match action{ButtonAction::Default=>return Ok(()),ButtonAction::Key{usage,mods} if (4..=0xa4).contains(usage)=>vec![2,2,mods.0,*usage],ButtonAction::Mouse(b)=>vec![1,1,match b{MouseButton::Left=>1,MouseButton::Right=>2,MouseButton::Middle=>3,MouseButton::Back=>4,MouseButton::Forward=>5,_=>return Err(Error::Unsupported("Naga wheel remap"))}],_=>return Err(Error::Unsupported("Naga action"))};
    let before=self.button(*id as u8)?;let mut args=[0;10];args[..3].copy_from_slice(&[1,*id as u8,0]);args[3..3+bytes.len()].copy_from_slice(&bytes);
    self.exchange(2,0x0c,&args)?;let actual=self.button(*id as u8)?;if actual[3..3+bytes.len()]!=bytes{let _=self.exchange(2,0x0c,&before);return Err(Error::invalid("Naga button did not read back; original record restore attempted"));}
   }
   _=>return Err(Error::Unsupported("Naga setting")),
  }s.apply_to(&mut self.state);Ok(())
 }
}
#[derive(Default)]pub struct NagaDriver;
impl Driver for NagaDriver{
 fn name(&self)->&'static str{"Razer Naga V2 HyperSpeed"}fn vendor(&self)->Vendor{Vendor::Razer}
 fn probe(&self,i:&HidInterfaceInfo)->bool{i.vendor_id==0x1532&&i.product_id==0x00b4&&(i.usage_page>=0xff00||(i.usage_page==1&&i.usage==2))}
 fn open(&self,i:&HidInterfaceInfo,b:&dyn HidBackend)->Result<Vec<Box<dyn Device>>>{if !self.probe(i){return Err(Error::Unsupported("Naga control collection"));}Ok(vec![Box::new(Naga::open(b.open(&i.path)?,i)?)])}
}
pub fn simulated()->(Box<dyn Device>,Arc<Mutex<Vec<gearwise_core::transport::MockRecord>>>){
 use gearwise_core::transport::{MockTransport,MockOp,MockReply};
 let mut dpi=[1,3,0x20,3,0x20,0,0];let mut rate=1;let mut buttons=BTreeMap::<u8,Vec<u8>>::new();
 let io=MockTransport::new(move|op|{let MockOp::SetFeature(req)=op else{return MockReply::none()};let mut r=req.to_vec();r[1]=2;let args=&req[9..9+req[6] as usize];let out=match(req[7],req[8]){(0,0x84)=>vec![0,0],(0,0x85)=>vec![rate],(0,5)=>{rate=args[0];args.to_vec()},(4,0x85)=>dpi.to_vec(),(4,5)=>{dpi.copy_from_slice(args);dpi.to_vec()},(2,0x8c)=>{let id=args[1];buttons.get(&id).cloned().unwrap_or_else(||vec![1,id,0,2,2,0,0x1e,0,0,0])},(2,0x0c)=>{buttons.insert(args[1],args.to_vec());args.to_vec()},_=>{r[1]=5;args.to_vec()}};r[6]=out.len() as u8;r[9..89].fill(0);r[9..9+out.len()].copy_from_slice(&out);r[89]=crc(&r[1..]);MockReply::feature(r)});
 let log=io.log();let info=HidInterfaceInfo{path:"simulation:naga".into(),vendor_id:0x1532,product_id:0x00b4,usage_page:0xff00,..Default::default()};(Box::new(Naga::open(Box::new(io),&info).unwrap()),log)
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn naga_packet_has_report_id_and_checksum(){let p=packet(2,0x0c,&[1,0x40,0,2,2,7,0x68,0,0,0]).unwrap();assert_eq!(p.len(),91);assert_eq!(&p[..9],&[3,0,0x1f,0,0,0,10,2,0x0c]);assert_eq!(p[89],0x2a);let captured_layer1=packet(2,0x0c,&[1,0x40,1,2,2,7,0x68,0,0,0]).unwrap();assert_eq!(captured_layer1[89],0x2b);}
 #[test]fn direct_key_binding_is_confirmed_by_actual_get_button_path(){let(mut d,log)=simulated();let s=Setting::Button{id:0x40,action:ButtonAction::Key{usage:0x68,mods:Modifiers(7)}};d.apply(&s).unwrap();assert_eq!(d.read_state().unwrap().buttons[&0x40],ButtonAction::Key{usage:0x68,mods:Modifiers(7)});assert!(log.lock().unwrap().iter().any(|r|matches!(r,gearwise_core::transport::MockRecord::SetFeature(p) if p[7]==2&&p[8]==0x0c)));}
 #[test]fn naga_dpi_and_rate_roundtrip(){let(mut d,_)=simulated();d.apply(&Setting::Dpi(DpiState{stages:vec![DpiStage{x:1600,y:2400}],active:0})).unwrap();d.apply(&Setting::ReportRate(500)).unwrap();let s=d.read_state().unwrap();assert_eq!(s.dpi.unwrap().stages[0],DpiStage{x:1600,y:2400});assert_eq!(s.report_rate,Some(500));}
 #[test]fn unrelated_devices_and_protected_left_rejected(){assert!(!NagaDriver.probe(&HidInterfaceInfo{vendor_id:0x046d,product_id:0x00b4,usage_page:0xff00,..Default::default()}));let(mut d,_)=simulated();assert!(d.apply(&Setting::Button{id:1,action:ButtonAction::Key{usage:4,mods:Modifiers(0)}}).is_err());}
}


#[cfg(test)]mod discovery_tests{
 use super::*;
 #[test]fn exact_mouse_control_collection_can_be_probed_without_claiming_keyboard_or_other_brands(){let mut i=HidInterfaceInfo{vendor_id:0x1532,product_id:0x00b4,usage_page:1,usage:2,..Default::default()};assert!(NagaDriver.probe(&i));i.usage=6;assert!(!NagaDriver.probe(&i));i.usage_page=0xff00;assert!(NagaDriver.probe(&i));i.vendor_id=0x046d;assert!(!NagaDriver.probe(&i));}
}
#[cfg(test)]mod fresh_state_tests{
 use super::*;use std::sync::atomic::{AtomicBool,Ordering};use gearwise_core::transport::{MockTransport,MockOp,MockReply};
 #[test]fn unreadable_or_unknown_action_does_not_reuse_a_stale_owned_key(){let unknown=Arc::new(AtomicBool::new(false));let state=unknown.clone();let io=MockTransport::new(move|op|{let MockOp::SetFeature(req)=op else{return MockReply::none()};let mut response=req.to_vec();response[1]=2;let args=&req[9..9+req[6] as usize];let out=match(req[7],req[8]){(0,0x84)=>vec![0,0],(0,0x85)=>vec![1],(4,0x85)=>vec![1,3,0x20,3,0x20,0,0],(2,0x8c)=>if state.load(Ordering::SeqCst){vec![1,args[1],0,0x0c,0,0,0,0,0,0]}else{vec![1,args[1],0,2,2,0,0x68,0,0,0]},_=>return MockReply::none()};response[6]=out.len() as u8;response[9..89].fill(0);response[9..9+out.len()].copy_from_slice(&out);response[89]=crc(&response[1..]);MockReply::feature(response)});let mut device=Naga::open(Box::new(io),&HidInterfaceInfo{path:"simulation:fresh".into(),vendor_id:0x1532,product_id:0x00b4,..Default::default()}).unwrap();assert_eq!(device.read_state().unwrap().buttons.len(),19);unknown.store(true,Ordering::SeqCst);assert!(device.read_state().unwrap().buttons.is_empty());}
}