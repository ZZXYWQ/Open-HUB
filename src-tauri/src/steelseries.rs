//! Exact wired SteelSeries bridge scope, independently checked against fixed
//! rivalcfg native profiles and byte vectors. Settings are write-only cached.
use gearwise_core::{Setting,model::{ButtonAction,Modifiers,MouseButton}};
pub const PRODUCTS:[u16;3]=[0x1710,0x1720,0x1722];
pub fn supported(v:u16,p:u16)->bool{v==0x1038&&PRODUCTS.contains(&p)}
pub fn validate_setting(product:u16,setting:&Setting)->Result<(),String>{
 if !PRODUCTS.contains(&product){return Err("此赛睿具体型号尚未核对".into());}
 let native=if let Setting::Button{id,action}=setting{
  if *id==1&&!matches!(action,ButtonAction::Default|ButtonAction::Mouse(MouseButton::Left)){return Err("保留主左键点击".into());}
  if matches!(action,ButtonAction::Macro(_)){Setting::Button{id:*id,action:ButtonAction::Key{usage:0x68,mods:Modifiers(0)}}}else{setting.clone()}
 }else{setting.clone()};
 gearwise_steelseries::validate_setting(product,&native).map_err(|e|e.to_string())
}
