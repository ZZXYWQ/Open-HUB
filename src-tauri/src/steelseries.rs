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

/// Version-2 recovery stores every non-primary key, because the native command
/// always writes the complete table and a reopened driver only has templates.
pub fn complete_table(product:u16,settings:&[Setting])->bool{
 if !PRODUCTS.contains(&product){return false;}let count=if product==0x1722{8}else{6};
 let ids=settings.iter().filter_map(|s|if let Setting::Button{id,..}=s{Some(*id)}else{None}).collect::<std::collections::BTreeSet<_>>();
 settings.len()==usize::from(count-1)&&ids==(2..=count).collect()
}
pub fn restore_table(device:&mut dyn gearwise_core::Device,settings:&[Setting])->Result<(),String>{
 let d=device.descriptor();if !supported(d.vendor_id,d.product_id)||!complete_table(d.product_id,settings){return Err("完整按键恢复表与目标鼠标不一致".into());}
 for s in settings{validate_setting(d.product_id,s)?;}
 let actions=settings.iter().map(|s|if let Setting::Button{id,action}=s{(*id,action.clone())}else{unreachable!()}).collect::<Vec<_>>();
 device.apply_button_table(&actions).map_err(|e|format!("完整原始按键恢复失败，记录已保留：{e}"))
}
