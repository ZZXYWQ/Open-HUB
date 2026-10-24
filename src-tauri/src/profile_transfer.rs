use crate::profiles::{self, Profile, ProfileStore};
use serde::Serialize;
use std::path::PathBuf;
#[derive(Serialize)]
pub struct Preview {pub profiles:Vec<Profile>,pub note:String}
pub fn validate(p:&Profile)->Result<(),String>{
 if let Some(plan)=&p.multi{crate::multibrand::validate(plan)?;if p.sino.is_some()||p.mchose_board.is_some()||p.ghub_id.is_some()||p.target_vendor!=Some(plan.vendor)||p.target_model!=Some(plan.product){return Err("三品牌配置的来源或型号标记不一致".into());}let dpi=plan.settings.iter().find_map(|s|if let gearwise_core::Setting::Dpi(d)=s{Some(d)}else{None}).ok_or("完整方案缺少 DPI 设置")?;let levels=dpi.stages.iter().map(|v|crate::profiles::Level{x:v.x,y:Some(v.y)}).collect::<Vec<_>>();let rate=plan.settings.iter().find_map(|s|if let gearwise_core::Setting::ReportRate(r)=s{Some(u32::from(*r))}else{None});if p.levels!=levels||p.active_level!=dpi.active as usize||p.report_rate_hz!=rate{return Err("配置摘要与三品牌完整方案不一致".into());}}
 if let Some(s)=&p.sino{crate::sinowealth::validate(s)?;if p.target_vendor!=Some(s.model.vendor)||p.target_model!=Some(s.model.product)||p.mchose_board.is_some()||p.ghub_id.is_some(){return Err("完整设备配置与品牌/型号标记不一致".into())}let levels=s.dpi.iter().map(|xy|crate::profiles::Level{x:xy[0],y:Some(xy[1])}).collect::<Vec<_>>();if p.levels!=levels||p.active_level!=s.active||p.report_rate_hz!=Some(s.rate){return Err("配置摘要与完整硬件方案不一致".into())}}

 if p.name.trim().is_empty()||p.name.len()>512||p.id.len()>256{return Err("配置名称或标识无效".into())}
 if p.levels.len()>32||p.levels.iter().any(|l|l.x==0||l.y==Some(0)){return Err("DPI 档位无效".into())}
 if p.levels.is_empty()&&p.mchose_board.is_none(){return Err("配置缺少 DPI 或板载引用".into())}
 if !p.levels.is_empty()&&p.active_level>=p.levels.len(){return Err("当前 DPI 档位越界".into())}
 if p.mchose_board.is_some_and(|b|b>2)||p.mchose_board.is_some()&&(p.target_vendor!=Some(0x3837)||p.target_model!=Some(0x4026)){return Err("迈从板载引用的品牌或型号不匹配".into())}
 if p.ghub_id.is_some()&&p.target_vendor.is_some_and(|v|v!=0x046d){return Err("G HUB 来源与品牌不匹配".into())}
 if p.report_rate_hz.is_some_and(|v|![125,250,500,1000,2000,4000,8000].contains(&v)){return Err("回报率无效".into())}
 if p.app_path.as_ref().is_some_and(|s|s.len()>4096){return Err("应用路径过长".into())} Ok(())
}
pub fn preview(source:&str,path:Option<String>)->Result<Preview,String>{
 let path=path.map(PathBuf::from).map(Ok).unwrap_or_else(||match source{"ghub"=>profiles::ghub_settings_path().ok_or_else(||"未找到本地 G HUB 配置".into()),"mhub"=>crate::mhub_import::default_path(),_=>Err("请选择 Open HUB 配置包".into())})?;
 if std::fs::metadata(&path).map_err(|e|e.to_string())?.len()>8*1024*1024{return Err("文件超过 8 MB".into())}
 let mut store=ProfileStore::default();
 let note=match source{
 "ghub"=>{profiles::import_ghub(&mut store,&path)?;"读取 DPI、回报率与应用关联；官方灯光、按键及宏尚未转换。"},
 "mhub"=>{let d=crate::mhub_import::read(&path)?;crate::mhub_import::import(&mut store,&d)?;"导入三组板载名称和引用；按键、宏、DPI 仍使用鼠标内已有数据。"},
 "openhub"=>{let d:serde_json::Value=serde_json::from_slice(&std::fs::read(path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
 if d["format"]!="open-hub-profiles/1"{return Err("需要 Open HUB 导出的配置包；其他品牌私有文件不能直接转换".into())}
 store.profiles=serde_json::from_value(d["profiles"].clone()).map_err(|e|format!("配置包字段无效：{e}"))?;"保留配置的品牌、型号、DPI、回报率和应用关联；仅导入，不自动应用。"},
 _=>return Err("尚无此品牌的私有配置解析器".into())};
 if source!="openhub" {store.profiles.retain(|p|p.ghub_id.is_some()||p.mchose_board.is_some());}
 if store.profiles.len()>512{return Err("一次最多导入 512 组配置".into())}
 for p in &mut store.profiles {if p.id==profiles::DESKTOP_ID{p.id=if source=="ghub"{"import-ghub-desktop".into()}else{"import-openhub-desktop".into()};}validate(p)?;}
 if store.profiles.is_empty(){return Err("文件中没有当前支持的配置项".into())}
 Ok(Preview{profiles:store.profiles,note:note.into()})
}
pub fn merge(store:&mut ProfileStore,items:Vec<Profile>)->Result<(),String>{
 if items.is_empty()||items.len()>512{return Err("请选择 1–512 组配置".into())}
 // Validate the complete batch before mutating anything.
 for p in &items{validate(p)?;if p.id==profiles::DESKTOP_ID{return Err("导入不能覆盖默认桌面配置".into())}}
 let mut ids=std::collections::HashSet::new();for p in &items{if !ids.insert(&p.id){return Err("配置包含重复标识".into())}}
 for mut p in items{if let Some(gid)=&p.ghub_id{if let Some(old)=store.profiles.iter().find(|old|old.id!=profiles::DESKTOP_ID&&old.ghub_id.as_ref()==Some(gid)){p.id=old.id.clone();}}if p.id.is_empty(){p.id=profiles::new_id();}store.upsert(p);}Ok(())
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn invalid_batch_never_partially_imports(){let mut s=ProfileStore::default();let mut a=Profile::desktop();a.id="good".into();let mut b=a.clone();b.id="bad".into();b.levels[0].x=0;assert!(merge(&mut s,vec![a,b]).is_err());assert_eq!(s.profiles.len(),1);}
 #[test]fn repeated_ghub_import_updates_existing_without_replacing_desktop(){let mut s=ProfileStore::default();let mut p=Profile::desktop();p.id="first".into();p.ghub_id=Some("official-id".into());p.target_vendor=Some(0x046d);merge(&mut s,vec![p.clone()]).unwrap();p.id="second".into();p.name="updated".into();merge(&mut s,vec![p]).unwrap();assert_eq!(s.profiles.len(),2);assert_eq!(s.get("first").unwrap().name,"updated");assert_eq!(s.active_id,"desktop");}
 #[test]fn openhub_file_roundtrip_preserves_scope(){let mut p=Profile::desktop();p.id="roundtrip".into();p.target_vendor=Some(0x1532);p.target_model=Some(0x0084);let f=std::env::temp_dir().join(format!("open-hub-roundtrip-{}.json",profiles::new_id()));std::fs::write(&f,serde_json::to_vec(&serde_json::json!({"format":"open-hub-profiles/1","profiles":[p]})).unwrap()).unwrap();let result=preview("openhub",Some(f.to_string_lossy().into_owned()));let _=std::fs::remove_file(&f);let imported=result.unwrap().profiles;assert_eq!(imported[0].target_vendor,Some(0x1532));assert!(!imported[0].targets(0x046d,&[0x0084]));}
 #[test]fn other_brand_cannot_claim_mchose_board(){let mut p=Profile::desktop();p.mchose_board=Some(0);p.target_vendor=Some(0x046d);assert!(validate(&p).is_err());}
 #[test]fn imported_profile_keeps_brand_and_model(){let mut s=ProfileStore::default();let mut p=Profile::desktop();p.id="brand-test".into();p.target_vendor=Some(0x1532);p.target_model=Some(1);merge(&mut s,vec![p]).unwrap();let p=s.get("brand-test").unwrap();assert!(!p.targets(0x3837,&[1]));assert!(p.targets(0x1532,&[1]));}
}


#[cfg(test)]mod multi_profile_tests{
 use super::*;use gearwise_core::{Setting,model::{DpiState,DpiStage}};
 #[test]fn multi_payload_preserves_brand_scope_through_pack(){let mut p=Profile::desktop();p.id="multi-roundtrip".into();p.target_vendor=Some(0x1038);p.target_model=Some(0x1720);p.multi=Some(crate::multibrand::Plan { macros:vec![], source:None,vendor:0x1038,product:0x1720,settings:vec![Setting::Dpi(DpiState{stages:vec![DpiStage::uniform(800)],active:0}),Setting::ReportRate(1000)]});p.levels=vec![profiles::Level{x:800,y:Some(800)}];p.active_level=0;p.report_rate_hz=Some(1000);validate(&p).unwrap();let encoded=serde_json::to_vec(&p).unwrap();let decoded:Profile=serde_json::from_slice(&encoded).unwrap();assert_eq!(decoded.multi,p.multi);assert!(!decoded.targets(0x046d,&[0x1720]));assert!(!decoded.targets(0x3837,&[0x1720]));let mut bad=decoded.clone();bad.levels[0].x=1600;assert!(validate(&bad).is_err());bad=decoded;bad.target_vendor=Some(0x1532);assert!(validate(&bad).is_err());}
}
