//! Read-only SteelSeries GG SQLite conversion. Schema/codec facts were checked
//! against GG122 migrations and inert factories 98871/86737/55489/58576.
//! Exact Rival310/Sensei310 schemas; model tags and six/eight physical records
//! are checked separately, never inferred from a brand or similar settings.
use crate::{icue_profiles::{Candidate,Prepared},multibrand::Plan,profiles::{Level,Profile}};
use gearwise_core::{Setting,model::{ButtonAction,DpiFunction,DpiStage,DpiState,Modifiers,MouseButton},profile::{Macro,MacroMode,MacroStep}};
use rusqlite::{Connection,OpenFlags};
use serde_json::Value;
use std::{collections::{BTreeMap,HashSet},path::Path};

const LIMIT:usize=1024*1024;
fn number(v:&Value)->Result<u32,String>{v.as_u64().and_then(|v|v.try_into().ok()).ok_or("GG 字段不是有效的非负整数".into())}
fn integer_text(v:&Value)->Result<u32,String>{if let Some(s)=v.as_str(){s.parse().map_err(|_|"GG 宏播放数值无效".into())}else{number(v)}}
fn field(v:&Value,k:&str)->Result<u32,String>{number(v.get(k).ok_or_else(||format!("GG 缺少字段 {k}"))?)}
fn json(s:&str)->Result<Value,String>{if s.len()>LIMIT{return Err("GG 单项配置超过 1 MiB".into());}serde_json::from_str(s).map_err(|e|format!("GG JSON 损坏：{e}"))}
fn mouse(code:u32)->Result<MouseButton,String>{match code{1=>Ok(MouseButton::Left),2=>Ok(MouseButton::Right),3=>Ok(MouseButton::Middle),4=>Ok(MouseButton::Back),5=>Ok(MouseButton::Forward),_=>Err("此 GG 鼠标输出按钮尚未转换".into())}}

pub fn prepare(path:&Path)->Result<Prepared,String>{
 let meta=std::fs::metadata(path).map_err(|e|e.to_string())?;if !meta.is_file()||meta.len()>256*1024*1024{return Err("请选择不超过 256 MiB 的 GG SQLite 配置数据库".into());}
 let conn=Connection::open_with_flags(path,OpenFlags::SQLITE_OPEN_READ_ONLY|OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(|e|e.to_string())?;
 conn.busy_timeout(std::time::Duration::from_secs(1)).map_err(|e|e.to_string())?;
 conn.execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF;").map_err(|e|e.to_string())?;
 let tx=conn.unchecked_transaction().map_err(|e|e.to_string())?;
 // One read transaction keeps profile, binding and macro references consistent.
 let result=read(&tx)?;tx.commit().map_err(|e|e.to_string())?;Ok(result)
}
fn read(conn:&Connection)->Result<Prepared,String>{
 let mut stmt=conn.prepare("SELECT c.id,c.name,d.product_id,d.name,CASE WHEN length(c.settings)<=1048576 THEN c.settings END FROM configurations c JOIN devices d ON c.device_id=d.id WHERE d.type=1 ORDER BY c.id LIMIT 501").map_err(|e|format!("GG 数据库格式不受支持：{e}"))?;
 let rows=stmt.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,u32>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|format!("GG 配置数据无效或超过大小限制：{e}"))?;
 if rows.is_empty(){return Err("GG 数据库没有鼠标配置".into());}if rows.len()>500{return Err("GG 鼠标配置超过 500 个，请先导出较小的数据库副本".into());}
 let mut ids=HashSet::new();let mut candidates=Vec::new();
 for(id,name,product,model,text)in rows{
  if !ids.insert(id.clone()){return Err("GG 配置标识重复".into());}
  let mut c=Candidate{model:format!("{model} · {name}"),profile:None,issues:vec![],notes:vec!["按原始事件转换 DPI、按钮和软件宏。灯效、加速度等未转换项目会单独提示；尚未实机验证。".into(),"导入与保存只处理文件；应用软件动作前请在编辑器选择此鼠标的输入来源。".into()]};
  let p=(||{
   let pid=match(product,model.as_str()){(0x10381720,"rival_310")=>0x1720,(0x10381722,"sensei_310")=>0x1722,_=>return Err("此配置不是已核对的 Rival 310 / Sensei 310，不能借用其他型号的物理布局".into())};
   if id.is_empty()||id.len()>80||name.is_empty()||name.len()>256{return Err("GG 配置名称或标识无效".into());}
   let source=json(&text)?;let(plan,levels,active,rate)=convert(conn,&id,pid,&source,&mut c.notes)?;
   let app_path=associated_program(conn,&id)?;
   let p=Profile{id:format!("gg:1038:{pid:04x}:{id}"),name,target_vendor:Some(0x1038),target_model:Some(pid),mchose_board:None,sino:None,multi:Some(plan),app_path,levels,active_level:active,report_rate_hz:Some(u32::from(rate)),ghub_id:None};crate::profile_transfer::validate(&p)?;Ok(p)
  })();match p{Ok(p)=>c.profile=Some(p),Err(e)=>c.issues.push(e)}candidates.push(c);
 }
 Ok(Prepared{name:"SteelSeries GG 鼠标配置".into(),source_kind:"GG SQLite",candidates})
}
fn associated_program(conn:&Connection,id:&str)->Result<Option<String>,String>{
 let mut stmt=conn.prepare("SELECT ac.device_id=c.device_id,a.path FROM apps_configs ac JOIN configurations c ON c.id=ac.config_id LEFT JOIN applications a ON a.id=ac.app_id WHERE ac.config_id=?1 LIMIT 2").map_err(|e|format!("无法核对 GG 应用关联：{e}"))?;
 let rows=stmt.query_map([id],|r|Ok((r.get::<_,bool>(0)?,r.get::<_,Option<String>>(1)?))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
 if rows.is_empty(){return Ok(None);}if rows.len()!=1{return Err("此 GG 配置关联多个程序，当前单路径配置格式尚不能完整保存这些条件".into());}
 let(same,path)=&rows[0];if !same{return Err("GG 应用关联与配置所属设备不一致".into());}let path=path.as_ref().filter(|p|!p.trim().is_empty()&&p.len()<=4096&&!p.chars().any(char::is_control)).ok_or("GG 应用路径缺失或无效，不能静默丢失自动切换条件")?;Ok(Some(path.clone()))
}
fn convert(conn:&Connection,id:&str,pid:u16,s:&Value,notes:&mut Vec<String>)->Result<(Plan,Vec<Level>,usize,u16),String>{
 let physical_buttons=match pid{0x1720=>6,0x1722=>8,_=>return Err("此 GG 型号没有已核对的按键布局".into())};
 let allowed=["resolution_1","resolution_2","raw_cpi","resolution_level","polling_rate","exact_accel","exact_aim","free_move","button_mappings","wheel_mappings","logo_lighting","wheel_lighting","logo_element","wheel_element"];
 for key in s.as_object().ok_or("GG 配置不是对象")?.keys(){if !allowed.contains(&key.as_str()){return Err(format!("GG 设置 {key} 尚未转换，不能保存为完整的鼠标参数方案"));}}
 for name in ["exact_accel","exact_aim","free_move"]{if s.get(name).is_some_and(|v|v.get("level").and_then(Value::as_u64)!=Some(0)){return Err(format!("{name} 的非默认性能行为尚未转换"));}}
 let mut stages=Vec::new();for index in 1..=2{let source=s.get(format!("resolution_{index}")).ok_or("缺少 GG CPI 档位")?;if field(source,"dcpi")?!=0{return Err("GG 额外 CPI 修正标志尚未转换".into());}let resolution=field(source,"resolution")?;if resolution>119{return Err("Rival 310 CPI 编码超出范围".into());}let value=((resolution+1)*100)as u16;
  if let Some(raw)=s.get("raw_cpi").and_then(|v|v.get(format!("level{index}"))){let raw=number(raw)?;if raw!=0&&raw!=u32::from(value){return Err("GG 显示 CPI 与设备 CPI 编码不一致，请先在 GG 核对原始值".into());}}
  stages.push(DpiStage::uniform(value));
 }
 let active=match s.get("resolution_level"){Some(v)=>match field(v,"level")?{1=>0,2=>1,_=>return Err("GG 当前 CPI 档位无效".into())},None=>{notes.push("来源没有记录当前 CPI 档位；预览选中档位 1，可在编辑器更改后应用。".into());0}};
 let rate=match field(s.get("polling_rate").ok_or("缺少 GG 回报率")?,"level")?{1=>1000,2=>500,3=>250,4=>125,_=>return Err("此 Rival 310 回报率编码尚未验证".into())};
 let source_buttons=s.pointer("/button_mappings/buttons").and_then(Value::as_array).ok_or("缺少 GG 物理按键映射")?;if source_buttons.len()!=physical_buttons{return Err(format!("GG 按键表不匹配当前型号的 {physical_buttons} 键布局"));}
 let wheel=s.pointer("/wheel_mappings/wheels").and_then(Value::as_array).ok_or("缺少 GG 滚轮映射")?;if wheel.len()!=2{return Err("GG 滚轮映射数量无效".into());}for(i,v)in wheel.iter().enumerate(){if field(v,"function")?!=49+i as u32||!zero_codes(v)?{return Err("GG 自定义滚轮动作尚未转换".into());}}
 let mut buttons=BTreeMap::new();let mut macros=Vec::new();
 for(i,v)in source_buttons.iter().enumerate(){let button=i as u16+1;let action=match field(v,"function")?{
  0=>{if !zero_codes(v)?{return Err("禁用按钮含未知附加键码".into());}ButtonAction::Disabled},
  code@1..=5=>{if !zero_codes(v)?{return Err("鼠标动作含未知附加键码".into());}ButtonAction::Mouse(mouse(code)?)},
  48=>{if !zero_codes(v)?{return Err("CPI 切换含未知附加键码".into());}ButtonAction::Dpi(DpiFunction::Cycle)},
  81=>{let keys=codes(v)?.into_iter().filter(|k|*k!=0).collect::<Vec<_>>();let mut seen=HashSet::new();if keys.is_empty()||keys.iter().any(|k|!seen.insert(*k)){return Err("GG 快捷键为空或重复".into());}
   let normal=keys.iter().copied().filter(|k|!(0xe0..=0xe7).contains(k)).collect::<Vec<_>>();if normal.len()==1{let mods=keys.iter().copied().filter(|k|(0xe0..=0xe7).contains(k)).fold(0,|m,k|m|1<<(k-0xe0));ButtonAction::Key{usage:normal[0],mods:Modifiers(mods)}}else{
    let mut steps=keys.iter().copied().map(MacroStep::KeyDown).collect::<Vec<_>>();steps.push(MacroStep::WaitForRelease);steps.extend(keys.iter().rev().copied().map(MacroStep::KeyUp));let body=Macro{id:format!("gg-key:{id}:{button}"),name:format!("按键 {button} 的原始按住组合"),mode:MacroMode::WhileHeld,steps};crate::host_macros::validate(&body)?;let action=ButtonAction::Macro(body.id.clone());macros.push(body);action
   }
  },other=>return Err(format!("GG 按键功能 {other} 尚未转换，未保存部分方案"))
 };buttons.insert(button,action);}
 let mut stmt=conn.prepare("SELECT type,trigger_data,macro_id,playback_options_data,is_passthrough FROM bindings WHERE config_id=?1 LIMIT 33").map_err(|e|format!("GG 绑定表格式不受支持：{e}"))?;
 let bindings=stmt.query_map([id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?,r.get::<_,String>(3)?,r.get::<_,i64>(4)?))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
 if bindings.len()>32{return Err("GG 自定义绑定过多".into());}let mut seen=HashSet::new();
 for(kind,trigger,macro_id,playback,passthrough)in bindings{
  let trigger=json(&trigger)?;let button=field(&trigger,"hidCode")?;if field(&trigger,"page")?!=1||field(&trigger,"type")?!=0||field(&trigger,"direction")?!=1||!(1..=physical_buttons as u32).contains(&button){return Err("GG 绑定不是已支持的物理鼠标按下触发".into());}
  if !seen.insert(button)||passthrough!=0{return Err("GG 绑定重复或含保留原键输出，尚不能无损转换".into());}
  if kind!="macro"&& !kind.is_empty(){return Err(format!("GG 自定义绑定 {kind} 尚未转换"));}let reference=macro_id.filter(|v|!v.is_empty()).ok_or("GG 宏引用缺失")?;
  let mut q=conn.prepare("SELECT name,CASE WHEN length(events)<=1048576 THEN events END FROM macros WHERE id=?1 LIMIT 2").map_err(|e|e.to_string())?;
  let rows=q.query_map([reference],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;if rows.len()!=1{return Err("GG 宏引用缺失或不唯一".into());}
  let body=macro_body(format!("gg-macro:{id}:{button}"),rows[0].0.clone(),&rows[0].1,&json(&playback)?)?;buttons.insert(button as u16,ButtonAction::Macro(body.id.clone()));macros.push(body);
 }
 if buttons.get(&1)!=Some(&ButtonAction::Mouse(MouseButton::Left)){return Err("GG 原始配置修改了主左键，当前导入保留主左键点击".into());}buttons.remove(&1);
 // A custom binding replaces a direct held chord; omit its now-unused body.
 macros.retain(|m|buttons.values().any(|a|a==&ButtonAction::Macro(m.id.clone())));
 if s.get("logo_lighting").is_some()||s.get("wheel_lighting").is_some(){notes.push("GG 原始灯效和指示色尚未自动转换；保留原数据库，可在灯光同步中另行设置。".into());}
 let levels=stages.iter().map(|s|Level{x:s.x,y:Some(s.y)}).collect();let mut settings=vec![Setting::Dpi(DpiState{stages,active:active as u8}),Setting::ReportRate(rate)];settings.extend(buttons.into_iter().map(|(id,action)|Setting::Button{id,action}));let plan=Plan{vendor:0x1038,product:pid,settings,macros,source:None};crate::multibrand::validate(&plan)?;Ok((plan,levels,active,rate))
}
fn codes(v:&Value)->Result<Vec<u8>,String>{let list=v.get("key_codes").and_then(Value::as_array).ok_or("缺少 GG 四字节键码")?;if list.len()!=4{return Err("GG 键码长度不是已验证的四字节布局".into());}list.iter().map(|v|number(v)?.try_into().map_err(|_|"GG HID 键码超出范围".into())).collect()}
fn zero_codes(v:&Value)->Result<bool,String>{Ok(codes(v)?.iter().all(|v|*v==0))}
fn macro_body(id:String,name:String,text:&str,playback:&Value)->Result<Macro,String>{
 let events=json(text)?;let events=events.as_array().ok_or("GG 原始宏事件不是数组")?;if events.is_empty()||events.len()>512{return Err("GG 宏事件数量无效".into());}let mut steps=Vec::new();
 for event in events{let code=field(event,"code")?;let page=field(event,"page")?;let extra=field(event,"extraData")?;match field(event,"type")?{
  2=>{if ![1,7].contains(&page)||extra>1{return Err("GG 宏键盘页面或按下/松开标志不受支持".into());}let key=code.try_into().map_err(|_|"GG 宏键码超出范围")?;steps.push(if extra==1{MacroStep::KeyDown(key)}else{MacroStep::KeyUp(key)});},
  0=>{if ![0,1].contains(&page)||extra>1{return Err("GG 宏鼠标页面或按下/松开标志不受支持".into());}let button=mouse(code)?;steps.push(if extra==1{MacroStep::MouseDown(button)}else{MacroStep::MouseUp(button)});},
  4=>{if code!=0||page!=0{return Err("GG 延迟事件包含未知参数".into());}steps.push(MacroStep::Delay(extra));},other=>return Err(format!("GG 宏事件 {other} 尚未转换"))
 }}
 // Validate the original iteration before adding repeats; repeated stuck input
 // must never become an accidentally balanced combined sequence.
 let original=Macro{id:id.clone(),name:name.clone(),mode:MacroMode::Once,steps};crate::host_macros::validate(&original)?;
 let kind=field(playback,"type")?;let count=integer_text(playback.get("num_repeat").ok_or("缺少 GG 重复次数")?)?;let gap=integer_text(playback.get("repeat_delay").ok_or("缺少 GG 重复间隔")?)?;
 if playback.get("direction").is_some_and(|v|v.as_u64()!=Some(1)){return Err("GG 播放方向不是按下触发".into());}
 let(mut steps,mode)=(original.steps.clone(),match kind{0|1=>MacroMode::Once,2=>MacroMode::WhileHeld,3=>MacroMode::Toggle,_=>return Err("GG 切换保持或未知播放方式尚未转换".into())});
 if kind==1{if count==0||count>512{return Err("GG 有限重复次数超出转换范围".into());}for _ in 1..count{if gap>0{steps.push(MacroStep::Delay(gap));}steps.extend(original.steps.clone());if steps.len()>512{return Err("GG 重复后的原始事件超过 512 步".into());}}}else if matches!(kind,2|3)&&gap>0{steps.push(MacroStep::Delay(gap));}
 let body=Macro{id,name,mode,steps};crate::host_macros::validate(&body)?;Ok(body)
}

#[cfg(test)]pub(crate) mod tests{
 use super::*;use serde_json::json;
 fn settings()->Value{json!({"resolution_1":{"resolution":7,"dcpi":0},"resolution_2":{"resolution":15,"dcpi":0},"raw_cpi":{"level1":800,"level2":1600},"polling_rate":{"level":3},"exact_accel":{"level":0},"exact_aim":{"level":0},"free_move":{"level":0},"button_mappings":{"buttons":[{"function":1,"key_codes":[0,0,0,0]},{"function":2,"key_codes":[0,0,0,0]},{"function":3,"key_codes":[0,0,0,0]},{"function":81,"key_codes":[229,26,0,0]},{"function":5,"key_codes":[0,0,0,0]},{"function":48,"key_codes":[0,0,0,0]}]},"wheel_mappings":{"wheels":[{"function":49,"key_codes":[0,0,0,0]},{"function":50,"key_codes":[0,0,0,0]}]}})}
 fn events()->String{json!([{"type":2,"page":1,"code":4,"extraData":1,"timestamp":12},{"type":0,"page":1,"code":2,"extraData":1,"timestamp":13},{"type":4,"page":0,"code":0,"extraData":23,"timestamp":0},{"type":2,"page":1,"code":4,"extraData":0,"timestamp":14},{"type":0,"page":1,"code":2,"extraData":0,"timestamp":15}]).to_string()}
 pub(crate) fn fixture()->Connection{let c=Connection::open_in_memory().unwrap();c.execute_batch("CREATE TABLE devices(id INTEGER PRIMARY KEY,product_id INTEGER,name TEXT,type INTEGER);CREATE TABLE configurations(id TEXT PRIMARY KEY,name TEXT,device_id INTEGER,settings TEXT);CREATE TABLE bindings(type TEXT,config_id TEXT,trigger_data TEXT,macro_id TEXT,playback_options_data TEXT,is_passthrough INTEGER);CREATE TABLE macros(id TEXT,name TEXT,events TEXT);CREATE TABLE apps_configs(config_id TEXT,app_id INTEGER,device_id INTEGER);CREATE TABLE applications(id INTEGER PRIMARY KEY,path TEXT);INSERT INTO devices VALUES(250,272111392,'rival_310',1);").unwrap();c.execute("INSERT INTO configurations VALUES('source-guid','GG game',250,?1)",[settings().to_string()]).unwrap();c.execute("INSERT INTO macros VALUES('source-macro','original events',?1)",[events()]).unwrap();c.execute("INSERT INTO bindings VALUES('macro','source-guid',?1,'source-macro',?2,0)",rusqlite::params![json!({"hidCode":5,"page":1,"type":0,"direction":1}).to_string(),json!({"type":0,"num_repeat":1,"repeat_delay":15}).to_string()]).unwrap();c}
 #[test]fn gg_database_dpi_right_modifier_mouse_and_keyboard_macro_save_and_real_driver_simulation(){let c=fixture();let prepared=read(&c).unwrap();let p=prepared.candidates[0].profile.as_ref().unwrap_or_else(||panic!("{:?}",prepared.candidates[0].issues));assert_eq!(p.report_rate_hz,Some(250));assert_eq!(p.levels,vec![Level{x:800,y:Some(800)},Level{x:1600,y:Some(1600)}]);let plan=p.multi.as_ref().unwrap();assert!(plan.settings.contains(&Setting::Button{id:4,action:ButtonAction::Key{usage:26,mods:Modifiers(32)}}));assert_eq!(plan.macros[0].steps,vec![MacroStep::KeyDown(4),MacroStep::MouseDown(MouseButton::Right),MacroStep::Delay(23),MacroStep::KeyUp(4),MacroStep::MouseUp(MouseButton::Right)]);
  let mut store=crate::profiles::ProfileStore::default();let mut other=Profile::desktop();other.id="mchose".into();other.target_vendor=Some(0x3837);other.target_model=Some(0x4026);crate::profile_transfer::merge(&mut store,vec![other,p.clone()]).unwrap();let bytes=serde_json::to_vec(&store).unwrap();let restored:crate::profiles::ProfileStore=serde_json::from_slice(&bytes).unwrap();assert_eq!(restored.profiles.len(),3);let restored=restored.get(&p.id).unwrap();assert!(!restored.targets(0x046d,&[0x1720]));assert!(!restored.targets(0x3837,&[0x1720]));assert_eq!(restored.multi,p.multi);let result=crate::multibrand::simulate(restored.multi.as_ref().unwrap()).unwrap();assert!(result["packet_count"].as_u64().unwrap()>0);assert_eq!(result["hardware_tested"],false);assert_eq!(result["software_macros"][0]["virtual_delay_ms"],23);
 }
 #[test]fn unknown_layout_dangling_macro_trigger_and_application_conditions_block_the_whole_candidate(){for sql in ["UPDATE devices SET product_id=272111394,name='sensei_310'","DELETE FROM macros","UPDATE bindings SET is_passthrough=1","INSERT INTO apps_configs VALUES('source-guid',1,250)","UPDATE bindings SET type='sse_command'","INSERT INTO bindings SELECT * FROM bindings"]{let c=fixture();c.execute_batch(sql).unwrap();let p=read(&c).unwrap();assert!(p.candidates[0].profile.is_none(),"{sql}");assert!(!p.candidates[0].issues.is_empty());}}
 #[test]fn source_application_path_is_preserved_and_cross_device_or_multiple_links_are_not_dropped(){let c=fixture();c.execute_batch("INSERT INTO applications VALUES(7,'C:/Games/game.exe');INSERT INTO apps_configs VALUES('source-guid',7,250);").unwrap();let p=read(&c).unwrap().candidates.remove(0).profile.unwrap();assert_eq!(p.app_path.as_deref(),Some("C:/Games/game.exe"));c.execute_batch("UPDATE apps_configs SET device_id=251").unwrap();assert!(read(&c).unwrap().candidates[0].profile.is_none());c.execute_batch("UPDATE apps_configs SET device_id=250;INSERT INTO applications VALUES(8,'C:/Games/other.exe');INSERT INTO apps_configs VALUES('source-guid',8,250);").unwrap();assert!(read(&c).unwrap().candidates[0].profile.is_none());}
 #[test]fn source_repeat_modes_and_gaps_survive_without_reducing_macro_to_a_shortcut(){let text=events();for(kind,mode)in [(0,MacroMode::Once),(2,MacroMode::WhileHeld),(3,MacroMode::Toggle)]{let m=macro_body("test".into(),"test".into(),&text,&json!({"type":kind,"num_repeat":"1","repeat_delay":"15"})).unwrap();assert_eq!(m.mode,mode);if kind!=0{assert_eq!(m.steps.last(),Some(&MacroStep::Delay(15)));}}let m=macro_body("test".into(),"test".into(),&text,&json!({"type":1,"num_repeat":2,"repeat_delay":15})).unwrap();assert_eq!(m.steps.len(),11);assert_eq!(m.steps[5],MacroStep::Delay(15));assert_eq!(m.duration_ms(),61);assert!(macro_body("test".into(),"test".into(),&text,&json!({"type":4,"num_repeat":1,"repeat_delay":15})).is_err());let bad=text.replace("\"extraData\":0","\"extraData\":1");assert!(macro_body("test".into(),"test".into(),&bad,&json!({"type":0,"num_repeat":1,"repeat_delay":0})).is_err());}
 #[test]fn database_import_is_read_only_and_bad_input_never_creates_a_database(){let path=std::env::temp_dir().join(format!("gg-{}.db",crate::profiles::new_id()));assert!(prepare(&path).is_err());assert!(!path.exists());let c=fixture();let target=path.to_str().unwrap();c.execute("VACUUM INTO ?1",[target]).unwrap();let before=std::fs::read(&path).unwrap();let result=prepare(&path).unwrap();assert!(result.candidates[0].profile.is_some());assert_eq!(std::fs::read(&path).unwrap(),before);std::fs::remove_file(path).unwrap();}
 #[test]fn unknown_performance_settings_stale_cpi_and_wheel_overrides_never_save_partial_profiles(){for change in 0..5{let c=fixture();let mut v=settings();match change{0=>v["exact_accel"]["level"]=json!(1),1=>v["resolution_1"]["dcpi"]=json!(1),2=>v["raw_cpi"]["level1"]=json!(850),3=>v["wheel_mappings"]["wheels"][0]["function"]=json!(81),_=>v["unknown_performance_mode"]=json!({"level":1})}c.execute("UPDATE configurations SET settings=?1",[v.to_string()]).unwrap();let prepared=read(&c).unwrap();assert!(prepared.candidates[0].profile.is_none());assert!(!prepared.candidates[0].issues.is_empty());}}
 #[test]#[ignore="requires explicit OPEN_HUB_GG_RIVAL310_SQL official installer source path"]fn official_gg_rival310_defaults_convert_using_the_model_specific_schema(){
  let path=std::env::var_os("OPEN_HUB_GG_RIVAL310_SQL").expect("set official GG SQL research path");let text=std::fs::read_to_string(path).unwrap();let start=text.find("VALUES (250, 272111392, 'rival_310', 'Rival 310', 1,").unwrap();let entry=&text[start..];let first=entry.find("'{").unwrap()+1;let end=first+entry[first..].find("}',").unwrap()+1;
  // Parse only the JSON literal; do not execute a vendor migration script.
  let data=json(&entry[first..end]).unwrap();let c=fixture();c.execute("UPDATE configurations SET settings=?1",[data.to_string()]).unwrap();let prepared=read(&c).unwrap();let p=prepared.candidates[0].profile.as_ref().unwrap_or_else(||panic!("{:?}",prepared.candidates[0].issues));assert_eq!(p.report_rate_hz,Some(1000));assert_eq!(p.levels[0],Level{x:800,y:Some(800)});assert_eq!(p.levels[1],Level{x:1600,y:Some(1600)});assert!(prepared.candidates[0].notes.iter().any(|n|n.contains("原始灯效")));assert!(crate::multibrand::simulate(p.multi.as_ref().unwrap()).unwrap()["packet_count"].as_u64().unwrap()>0);
 }
 fn sensei_fixture()->Connection{let c=fixture();c.execute("UPDATE devices SET product_id=272111394,name='sensei_310'",[]).unwrap();let mut source=settings();let b=source["button_mappings"]["buttons"].as_array_mut().unwrap();b[5]=json!({"function":81,"key_codes":[78,0,0,0]});b.push(json!({"function":81,"key_codes":[75,0,0,0]}));b.push(json!({"function":48,"key_codes":[0,0,0,0]}));c.execute("UPDATE configurations SET settings=?1",[source.to_string()]).unwrap();c}
 #[test]fn gg_sensei_eight_controls_preserve_native_right_side_keys_and_last_button_macro(){
  let c=sensei_fixture();c.execute("UPDATE bindings SET trigger_data=?1",[json!({"hidCode":8,"page":1,"type":0,"direction":1}).to_string()]).unwrap();let prepared=read(&c).unwrap();let p=prepared.candidates[0].profile.as_ref().unwrap_or_else(||panic!("{:?}",prepared.candidates[0].issues));assert_eq!(p.target_model,Some(0x1722));assert!(p.id.starts_with("gg:1038:1722:"));let plan=p.multi.as_ref().unwrap();assert_eq!(plan.settings.iter().filter(|s|matches!(s,Setting::Button{..})).count(),7);for(id,usage)in[(6,78),(7,75)]{assert!(plan.settings.contains(&Setting::Button{id,action:ButtonAction::Key{usage,mods:Modifiers(0)}}));}assert!(plan.settings.iter().any(|s|matches!(s,Setting::Button{id:8,action:ButtonAction::Macro(_)})));assert!(!p.targets(0x1038,&[0x1720]));assert_eq!(crate::multibrand::simulate(plan).unwrap()["software_macros"][0]["virtual_delay_ms"],23);
  if let Some(path)=std::env::var_os("OPEN_HUB_GG_SENSEI_FIXTURE"){std::fs::write(path,serde_json::to_vec_pretty(&prepared).unwrap()).unwrap();}
 }
 #[test]fn gg_sensei_and_rival_require_consistent_ids_and_their_own_physical_tables(){
  for(product,name)in[(272111392,"sensei_310"),(272111394,"rival_310"),(272111396,"sensei_310")]{let c=sensei_fixture();c.execute("UPDATE devices SET product_id=?1,name=?2",rusqlite::params![product,name]).unwrap();assert!(read(&c).unwrap().candidates[0].profile.is_none());}
  let c=fixture();c.execute("UPDATE devices SET product_id=272111394,name='sensei_310'",[]).unwrap();assert!(read(&c).unwrap().candidates[0].profile.is_none());let c=sensei_fixture();c.execute("UPDATE devices SET product_id=272111392,name='rival_310'",[]).unwrap();assert!(read(&c).unwrap().candidates[0].profile.is_none());let c=sensei_fixture();c.execute("UPDATE bindings SET trigger_data=?1",[json!({"hidCode":9,"page":1,"type":0,"direction":1}).to_string()]).unwrap();assert!(read(&c).unwrap().candidates[0].profile.is_none());
 }
 #[test]
 #[ignore="requires the inert GG122 official migration JSON literal"]
 fn official_gg_sensei310_eight_key_defaults_use_their_own_native_model(){
  let path=std::env::var_os("OPEN_HUB_GG_RIVAL310_SQL").unwrap();let text=std::fs::read_to_string(path).unwrap();let start=text.find("VALUES (251, 272111394, 'sensei_310', 'Sensei 310', 1,").unwrap();let entry=&text[start..];let first=entry.find("'{").unwrap()+1;let end=first+entry[first..].find("}',").unwrap()+1;let data=json(&entry[first..end]).unwrap();let c=sensei_fixture();c.execute("UPDATE configurations SET settings=?1",[data.to_string()]).unwrap();c.execute("DELETE FROM bindings",[]).unwrap();let result=read(&c).unwrap();let p=result.candidates[0].profile.as_ref().unwrap_or_else(||panic!("{:?}",result.candidates[0].issues));assert_eq!(p.target_model,Some(0x1722));let plan=p.multi.as_ref().unwrap();assert_eq!(plan.settings.iter().filter(|s|matches!(s,Setting::Button{..})).count(),7);assert!(plan.settings.contains(&Setting::Button{id:6,action:ButtonAction::Key{usage:78,mods:Modifiers(0)}}));assert!(plan.settings.contains(&Setting::Button{id:7,action:ButtonAction::Key{usage:75,mods:Modifiers(0)}}));assert!(plan.settings.contains(&Setting::Button{id:8,action:ButtonAction::Dpi(DpiFunction::Cycle)}));assert_eq!(crate::multibrand::simulate(plan).unwrap()["hardware_tested"],false);
 }

 #[test]
 #[ignore="requires current native Sensei GG editor output"]
 fn browser_sensei_gg_import_saves_reloads_merges_and_executes_the_eighth_key_macro(){
  let path=std::env::var_os("OPEN_HUB_GG_SENSEI_UI_PROFILE").unwrap();let p:Profile=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();assert_eq!(p.target_model,Some(0x1722));assert_eq!(p.name,"Sensei GG 八键方案");let mut store=crate::profiles::ProfileStore::default();crate::profile_transfer::merge(&mut store,vec![p.clone()]).unwrap();let file=std::env::temp_dir().join(format!("open-hub-sensei-{}.json",crate::profiles::new_id()));store.save(&file).unwrap();let loaded=crate::profiles::ProfileStore::load(&file);std::fs::remove_file(file).unwrap();let restored=loaded.get(&p.id).unwrap();let plan=restored.multi.as_ref().unwrap();assert_eq!(plan.settings.iter().filter(|s|matches!(s,Setting::Button{..})).count(),7);assert_eq!(plan.macros[0].name,"Sensei 导入原始宏");assert!(plan.settings.iter().any(|s|matches!(s,Setting::Button{id:8,action:ButtonAction::Macro(id)}if id==&plan.macros[0].id)));let result=crate::multibrand::simulate(plan).unwrap();assert_eq!(result["software_macros"][0]["virtual_delay_ms"],23);assert_eq!(result["hardware_tested"],false);assert!(!restored.targets(0x1038,&[0x1720]));assert!(!restored.targets(0x046d,&[0x1722]));
 }

}
