//! Stateful synthetic firmware for the six explicitly enabled Viper USB ids.
//! This exercises the production driver and encoders; it is not a USB capture.
use super::*;
use gearwise_core::{model::{DpiStage,DpiState},transport::{MockTransport,MockReply,MockOp,MockRecord}};
use std::{collections::BTreeMap,sync::{Arc,Mutex},time::Duration};

pub fn mouse(pid:u16)->Result<(Box<dyn Device>,Arc<Mutex<Vec<MockRecord>>>)>{
 if !matches!(pid,0x007a|0x007b|0x00a5|0x00a6|0x00c0|0x00c1){return Err(Error::Unsupported("Viper simulation model"));}
 let model=models::find(pid).unwrap();
 let mut stages=commands::set_dpi_stages(&DpiState{stages:vec![DpiStage::uniform(800)],active:0}).args;
 let mut legacy=1;let mut hyper=8;let mut brightness=BTreeMap::<u8,u8>::new();let mut buttons=BTreeMap::new();
 for id in [1,2,4,5,9,10,0x60]{let mut a=[0u8;80];a[..10].copy_from_slice(&crate::device::viper_default(id).unwrap());buttons.insert(id,a);}
 let io=MockTransport::new(move |op|{
  let MockOp::SetFeature(data)=op else{return MockReply::none()};
  let req=report::Report::from_wire(data).expect("malformed Razer request");let mut response=req.clone();response.status=2;response.args=[0;80];
  match(req.class,req.command){
   (0,0x82)=>response.args[..15].copy_from_slice(b"SIMULATED-VIPER"),
   (0,0x81)=>response.args[..2].copy_from_slice(&[1,12]),
   (4,0x86)=>response.args=stages,
   (4,6)=>stages=req.args,
   (0,0x85)=>response.args[0]=legacy,
   (0,5)=>legacy=req.args[0],
   (0,0xc0)=>response.args[1]=hyper,
   (0,0x40)=>hyper=req.args[1],
   (0x0f,2)=>{},
   (0x0f,4)=>{brightness.insert(req.args[1],req.args[2]);},
   (0x0f,0x84)=>{response.args[..3].copy_from_slice(&[1,req.args[1],*brightness.get(&req.args[1]).unwrap_or(&255)]);},
   (2,0x8c)=>response.args=*buttons.get(&req.args[1]).expect("unknown Viper button"),
   (2,0x0c) if req.transaction_id==0x10=>{buttons.insert(req.args[1],req.args);},
   _=>response.status=5,
  }
  MockReply::feature(response.to_wire().to_vec())
 });
 let log=io.log();let link=Link::new(Box::new(io),model.transaction_id,Timing::new(Duration::ZERO,Duration::ZERO,1));
 let iface=HidInterfaceInfo{path:format!("simulation:viper:{pid:04x}"),vendor_id:0x1532,product_id:pid,interface_number:model.control.interface,usage_page:1,usage:2,..Default::default()};
 Ok((Box::new(RazerDevice::open(model,link,&iface)?),log))
}

#[cfg(test)]mod tests{
 use super::*;
 use crate::{report::Report,testutil::{Fake,INSTANT,sent}};
 use gearwise_core::model::{ButtonAction,MouseButton};
 fn scripted(fake:Fake)->(RazerDevice,Arc<Mutex<Vec<MockRecord>>>){let(io,log)=fake.transport();let model=models::find(0x00c1).unwrap();let link=Link::new(Box::new(io),model.transaction_id,INSTANT);(RazerDevice::open(model,link,&HidInterfaceInfo::default()).unwrap(),log)}
 #[test]fn unreadable_original_button_prevents_every_write(){let(mut device,log)=scripted(Fake::default());log.lock().unwrap().clear();assert!(device.apply(&gearwise_core::Setting::Button{id:4,action:ButtonAction::Disabled}).is_err());assert!(!sent(&log).iter().any(|b|b[7..9]==[2,0x0c]));}
 #[test]fn bad_button_readback_restores_exact_original_with_viper_transaction(){let original=[1,4,0,1,1,5,0,0,0,0];let(mut device,log)=scripted(Fake::default().on(2,0x8c,2,&original));log.lock().unwrap().clear();assert!(device.apply(&gearwise_core::Setting::Button{id:4,action:ButtonAction::Mouse(MouseButton::Right)}).is_err());let writes=sent(&log).into_iter().filter(|b|b[7..9]==[2,0x0c]).collect::<Vec<_>>();assert_eq!(writes.len(),2);assert_eq!(writes[0][2],0x10);assert_eq!(writes[1][2],0x10);assert_eq!(&writes[1][9..19],&original);}
}

#[cfg(test)]mod captured_controls_tests{
 use super::*;use gearwise_core::{Setting,model::{ButtonAction,MouseButton}};
 #[test]fn viper_scroll_and_sensitivity_toggle_match_independent_captures(){for pid in [0x00c0,0x00c1]{let(mut d,log)=mouse(pid).unwrap();for(id,default)in[(9,[1,9,0,1,1,9,0,0,0,0]),(10,[1,10,0,1,1,10,0,0,0,0]),(0x60,[1,0x60,0,6,1,6,0,0,0,0])]{assert_eq!(d.read_state().unwrap().buttons[&id],ButtonAction::Default);d.apply(&Setting::Button{id,action:ButtonAction::Disabled}).unwrap();assert_eq!(d.read_state().unwrap().buttons[&id],ButtonAction::Disabled);log.lock().unwrap().clear();d.apply(&Setting::Button{id,action:ButtonAction::Default}).unwrap();assert_eq!(d.read_state().unwrap().buttons[&id],ButtonAction::Default);let log=log.lock().unwrap();assert!(log.iter().any(|r|matches!(r,MockRecord::SetFeature(p)if p[7..9]==[2,0x0c]&&p[2]==0x10&&p[9..19]==default)));}}}
 #[test]fn unverified_key_middle_and_cross_remaps_are_rejected_before_io(){let(mut d,log)=mouse(0x00c0).unwrap();for(id,action)in[(9,ButtonAction::Mouse(MouseButton::Right)),(10,ButtonAction::Key{usage:4,mods:gearwise_core::model::Modifiers(0)}),(0x60,ButtonAction::Mouse(MouseButton::Back)),(3,ButtonAction::Disabled),(1,ButtonAction::Disabled)]{log.lock().unwrap().clear();assert!(d.apply(&Setting::Button{id,action}).is_err());assert!(log.lock().unwrap().is_empty());}}
 #[test]fn unknown_originals_and_bad_tails_remain_locked(){assert!(crate::device::viper_decode(9,&[1,9,0,2,1,9,0,0,0,0]).is_none());assert!(crate::device::viper_decode(0x60,&[1,0x60,0,6,1,6,0,0,0,1]).is_none());assert!(crate::device::viper_decode(9,&[1,10,0,1,1,10,0,0,0,0]).is_none());}
}
