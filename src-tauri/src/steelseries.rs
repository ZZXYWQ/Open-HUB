//! Exact wired SteelSeries bridge scope, independently checked against fixed
//! rivalcfg native profiles and byte vectors. Settings are write-only cached.
use gearwise_core::{Setting,model::{ButtonAction,Modifiers,MouseButton}};
pub const FIXED_PRODUCTS:[u16;3]=[0x1710,0x1720,0x1722];
pub const PRODUCTS:[u16;8]=[0x1710,0x1720,0x1722,0x1832,0x1724,0x1824,0x184c,0x182e];
pub fn button_count(product:u16)->Option<u16>{match product{0x1710|0x1720=>Some(6),0x1824|0x184c|0x182e=>Some(8),0x1722|0x1832=>Some(8),0x1724=>Some(7),_=>None}}
pub fn fixed_dpi_stages(product:u16)->Option<usize>{matches!(product,0x1710|0x1720|0x1722|0x1724).then_some(2)}
pub fn zone_count(product:u16)->Option<u8>{match product{0x182e=>Some(1),0x1824|0x184c=>Some(4),0x1724=>Some(8),0x1710|0x1720|0x1722|0x1832=>Some(2),_=>None}}
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
 if !PRODUCTS.contains(&product){return false;}let count=button_count(product).unwrap();
 let ids=settings.iter().filter_map(|s|if let Setting::Button{id,..}=s{Some(*id)}else{None}).collect::<std::collections::BTreeSet<_>>();
 settings.len()==if product==0x182e{6}else{usize::from(count-1)}&&ids==if product==0x182e{[2,3,4,5,7,8].into_iter().collect()}else{(2..=count).collect()}
}
pub fn restore_table(device:&mut dyn gearwise_core::Device,settings:&[Setting])->Result<(),String>{
 let d=device.descriptor();if !supported(d.vendor_id,d.product_id)||!complete_table(d.product_id,settings){return Err("完整按键恢复表与目标鼠标不一致".into());}
 for s in settings{validate_setting(d.product_id,s)?;}
 let actions=settings.iter().map(|s|if let Setting::Button{id,action}=s{(*id,action.clone())}else{unreachable!()}).collect::<Vec<_>>();
 device.apply_button_table(&actions).map_err(|e|format!("完整原始按键恢复失败，记录已保留：{e}"))
}

pub fn validate_lighting_plan(p:&crate::multibrand::Plan)->Result<(),String>{
 if p.vendor==0x1038&&p.product==0x182e&&p.settings.iter().any(|s|matches!(s,Setting::VendorProfile{..}))&&p.settings.iter().any(|s|matches!(s,Setting::Lighting{..})){return Err("Prime 完整滚轮灯效不能混入单独灯区设置".into());}
 if p.vendor==0x1038&&gearwise_steelseries::rival3::supported(p.product)&&p.settings.iter().any(|s|matches!(s,Setting::VendorProfile{..}))&&p.settings.iter().any(|s|matches!(s,Setting::Lighting{..})){return Err("Rival3 全局灯效不能与单独灯区设置混合，请在同一灯效编辑器核对".into());}Ok(())
}
