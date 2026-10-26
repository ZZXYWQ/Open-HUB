//! Read-only iCUE cereal XML import. Schema facts were checked against public
//! iCUE exports; physical layouts remain restricted to the exact M65 PRO RGB.
use crate::{multibrand::Plan, profiles::{Profile,Level}};
use gearwise_core::{Setting,model::{ButtonAction,DpiState,DpiStage,DpiFunction,Modifiers,MouseButton},profile::{Macro,MacroMode,MacroStep}};
use serde::Serialize;
use std::{collections::{BTreeMap,HashMap},io::Read};

#[derive(Default,Debug)] struct Node { name:String,text:String,children:Vec<Node> }
impl Node {
 fn c(&self,name:&str)->Option<&Node>{self.children.iter().find(|n|n.name==name)}
 fn at(&self,path:&[&str])->Option<&Node>{let mut n=self;for p in path{n=n.c(p)?;}Some(n)}
 fn t(&self,name:&str)->Option<&str>{self.c(name).map(|n|n.text.trim())}
 fn need(&self,name:&str)->Result<&str,String>{self.t(name).ok_or_else(||format!("缺少 iCUE 字段 {name}"))}
 fn flag(&self,name:&str)->Result<bool,String>{match self.need(name)?{"true"=>Ok(true),"false"=>Ok(false),_=>Err(format!("{name} 不是有效布尔值"))}}
 fn number(&self,name:&str)->Result<u16,String>{self.need(name)?.parse().map_err(|_|format!("{name} 超出整数范围"))}
}
fn push(stack:&mut [Node],n:Node)->Result<(),String>{let parent=stack.last_mut().ok_or("XML 根节点无效")?;if n.name!="base"&&parent.children.iter().any(|c|c.name==n.name){return Err(format!("XML 字段 {} 重复，不能确定采用哪一个值",n.name));}parent.children.push(n);Ok(())}
fn xml(text:&str)->Result<Node,String>{
 use quick_xml::{Reader,events::Event};
 if text.len()>16*1024*1024{return Err("iCUE 文件超过 16 MiB".into());}
 let mut reader=Reader::from_str(text);let mut stack=vec![Node::default()];let mut count=0;
 loop{match reader.read_event().map_err(|e|format!("iCUE XML 损坏：{e}"))?{
  Event::Start(e)=>{count+=1;if count>150000||stack.len()>=128{return Err("iCUE XML 节点或嵌套过多".into());}stack.push(Node{name:std::str::from_utf8(e.name().as_ref()).map_err(|e|e.to_string())?.into(),..Default::default()});},
  Event::Empty(e)=>{count+=1;if count>150000{return Err("iCUE XML 节点过多".into());}push(&mut stack,Node{name:std::str::from_utf8(e.name().as_ref()).map_err(|e|e.to_string())?.into(),..Default::default()})?;},
  Event::End(_)=>{if stack.len()<2{return Err("XML 结束节点无效".into());}let n=stack.pop().unwrap();push(&mut stack,n)?;},
  Event::Text(e)=>{let decoded=e.decode().map_err(|e|e.to_string())?;stack.last_mut().unwrap().text.push_str(&decoded);},
  Event::GeneralRef(e)=>{let name=e.decode().map_err(|e|e.to_string())?;let entity=format!("&{name};");stack.last_mut().unwrap().text.push_str(&quick_xml::escape::unescape(&entity).map_err(|e|e.to_string())?);},
  Event::CData(e)=>stack.last_mut().unwrap().text.push_str(&e.decode().map_err(|e|e.to_string())?),
  Event::DocType(_)=>return Err("iCUE 导入不允许 DTD 或外部实体".into()),
  Event::Eof=>break,_=>{}
 }}
 if stack.len()!=1{return Err("iCUE XML 未闭合".into());}let root=stack.pop().unwrap();if root.children.len()!=1||root.children[0].name!="cereal"{return Err("文件不是 iCUE cereal XML 导出".into());}Ok(root)
}
struct Context<'a>{types:HashMap<u32,&'a str>,pointers:HashMap<u32,&'a Node>}
impl<'a> Context<'a>{
 fn new(root:&'a Node)->Result<Self,String>{
  fn walk<'a>(n:&'a Node,c:&mut Context<'a>)->Result<(),String>{
   if let(Some(id),Some(name))=(n.t("polymorphic_id").and_then(|v|v.parse::<u32>().ok()),n.t("polymorphic_name")){if let Some(old)=c.types.insert(id&0x7fffffff,name){if old!=name{return Err("iCUE 多态类型标识冲突".into());}}}
   if n.name=="ptr_wrapper"{if let(Some(id),Some(data))=(n.t("id").and_then(|v|v.parse::<u32>().ok()),n.c("data")){if c.pointers.insert(id&0x7fffffff,data).is_some(){return Err("iCUE 共享对象标识重复".into());}}}
   for child in &n.children{walk(child,c)?;}Ok(())
  }
  let mut c=Self{types:HashMap::new(),pointers:HashMap::new()};walk(root,&mut c)?;Ok(c)
 }
 fn kind(&self,n:&Node)->Result<&str,String>{n.t("polymorphic_id").and_then(|v|v.parse::<u32>().ok()).and_then(|id|self.types.get(&(id&0x7fffffff)).copied()).ok_or("iCUE 动作的类型引用未定义".into())}
 fn data(&self,n:&'a Node)->Result<&'a Node,String>{let wrapper=n.c("ptr_wrapper").ok_or("缺少 iCUE 对象包装")?;if let Some(d)=wrapper.c("data"){return Ok(d);}wrapper.t("id").and_then(|v|v.parse::<u32>().ok()).and_then(|id|self.pointers.get(&(id&0x7fffffff)).copied()).ok_or("iCUE 共享对象引用未定义".into())}
}
#[derive(Serialize)]pub struct Candidate{pub model:String,pub profile:Option<Profile>,pub issues:Vec<String>,pub notes:Vec<String>}
#[derive(Serialize)]pub struct Prepared{pub name:String,pub source_kind:&'static str,pub candidates:Vec<Candidate>}
pub fn prepare(path:&std::path::Path)->Result<Prepared,String>{let mut text=String::new();std::fs::File::open(path).map_err(|e|e.to_string())?.take(16*1024*1024+1).read_to_string(&mut text).map_err(|e|e.to_string())?;convert(&text)}
pub fn convert(text:&str)->Result<Prepared,String>{
 let root=xml(text)?;let ctx=Context::new(&root)?;let archive=root.at(&["cereal","value0"]).ok_or("缺少 iCUE 导出数据")?;let profile=archive.c("profile").ok_or("缺少 iCUE 配置")?;let name=profile.need("name")?.to_string();let source_id=profile.need("id")?;
 let mouse=profile.c("properties").and_then(|p|p.children.iter().find(|p|p.t("key")==Some("Mouse"))).and_then(|p|p.at(&["value","properties"])).ok_or("此 iCUE 导出不包含鼠标配置")?;
 let dpi=mouse.children.iter().find(|p|ctx.kind(p).ok()==Some("DPIProperty::Proxy")).ok_or("此导出没有 DPI 型号关联；需要包含鼠标 DPI 的完整导出")?;
 let links=ctx.data(dpi)?.c("properties").ok_or("缺少 DPI 型号关联表")?;let mut candidates=Vec::new();
 for entry in &links.children{
  let model=entry.at(&["key","modelId"]).ok_or("DPI 关联缺少型号")?.text.trim().to_string();let reference=ctx.data(entry.c("value").ok_or("缺少 DPI 引用")?)?.need("id")?;
  let mut candidate=Candidate{model:model.clone(),profile:None,issues:vec![],notes:vec!["此导入转换 DPI、按键和宏；灯效、档位指示色、表面校准尚未自动转换。".into(),"导出未提供回报率时保留当前设备值。软件动作需要在编辑方案中选择这只鼠标的输入来源。".into()]};
  let result=(||->Result<Profile,String>{
   if model.to_ascii_uppercase().chars().filter(|c|c.is_ascii_alphanumeric()).collect::<String>()!="M65PRORGB"{return Err(format!("尚未实现 {model} 的物理按键布局；当前可执行导入仅适配 M65 PRO RGB (1B1C:1B2E)"));}
   if profile.c("linkedProgramsPaths").is_some_and(|p|!p.children.is_empty()){return Err("此导出的程序关联列表尚未转换，不能静默丢失自动切换条件".into());}
   let modes=archive.c("dpiModes").ok_or("导出未包含引用的 DPI 数据")?;let matches=modes.children.iter().filter(|e|e.t("key")==Some(model.as_str())).filter_map(|e|e.c("value")).filter_map(|e|ctx.data(e).ok()).filter(|e|e.t("id")==Some(reference)).collect::<Vec<_>>();if matches.len()!=1{return Err("DPI 引用缺失或存在歧义".into());}
   let(mode,sniper)=dpi_mode(&ctx,matches[0])?;let mut settings=vec![Setting::Dpi(mode.clone())];let mut macros=Vec::new();let mut bindings=BTreeMap::new();
   let actions=software_actions(&ctx,mouse,&model)?;
   for item in &actions.children{
    let target=item.c("second").ok_or("缺少按键分配目标")?;let key=target.t("key").or_else(||target.t("first")).ok_or("缺少物理按键名称")?;if target.t("layer").is_some_and(|s|s!="StandardLayer"){return Err(format!("{key} 的层行为尚未转换"));}if target.t("event").or_else(||target.t("second"))!=Some("Click"){return Err(format!("{key} 的触发事件尚未转换"));}
    let id=key.strip_prefix("Mouse").and_then(|n|n.parse::<u16>().ok()).filter(|n|(1..=8).contains(n)).ok_or_else(||format!("{key} 不属于已验证的 M65 PRO 按键布局"))?;
    let node=item.c("first").ok_or("缺少按键动作")?;let(action,body)=action(&ctx,node)?;if id==1&&action!=ButtonAction::Mouse(MouseButton::Left){return Err("保留原始左键点击，源配置的左键修改尚不能导入".into());}if id==1{continue;}
    if bindings.insert(id,action).is_some(){return Err(format!("按键 {id} 的源配置重复"));}if let Some(body)=body{if let Some(previous)=macros.iter().find(|m:&&Macro|m.id==body.id){if previous!=&body{return Err("源宏使用相同标识但内容冲突，不能选取其中一个".into());}}else{macros.push(body);}}
   }
   if !bindings.contains_key(&8){if let Some(value)=sniper{bindings.insert(8,ButtonAction::Dpi(DpiFunction::Shift(value)));}}
   settings.extend(bindings.into_iter().map(|(id,action)|Setting::Button{id,action}));let plan=Plan{vendor:0x1b1c,product:0x1b2e,settings,macros,source:None};crate::multibrand::validate(&plan)?;
   let p=Profile{id:format!("icue:1b1c:1b2e:{source_id}"),name:name.clone(),target_vendor:Some(0x1b1c),target_model:Some(0x1b2e),mchose: None, mchose_board:None,sino:None,multi:Some(plan),app_path:profile.t("linkedProgramPath").filter(|s|!s.is_empty()).map(str::to_owned),levels:mode.stages.iter().map(|s|Level{x:s.x,y:Some(s.y)}).collect(),active_level:usize::from(mode.active),report_rate_hz:None,ghub_id:None};crate::profile_transfer::validate(&p)?;Ok(p)
  })();match result{Ok(p)=>candidate.profile=Some(p),Err(e)=>candidate.issues.push(e)}candidates.push(candidate);
 }
 if candidates.is_empty(){return Err("没有可转换的鼠标 DPI 型号关联".into());}Ok(Prepared{name,source_kind:"iCUE cereal XML",candidates})
}
fn software_actions<'a>(ctx:&Context<'a>,mouse:&'a Node,model:&str)->Result<&'a Node,String>{
 let tables=mouse.children.iter().filter(|p|matches!(ctx.kind(p).ok(),Some("ActionsProperty"|"ActionsProperty::Proxy"))).collect::<Vec<_>>();if tables.len()!=1{return Err("需要唯一的软件按键动作表；不把板载槽位表猜作软件配置".into());}
 let mut data=ctx.data(tables[0])?;if ctx.kind(tables[0])?=="ActionsProperty::Proxy"{let entries=data.c("properties").ok_or("缺少型号按键表")?.children.iter().filter(|p|p.t("key")==Some(model)).collect::<Vec<_>>();if entries.len()!=1{return Err("软件按键表与 DPI 型号不匹配或重复".into());}data=ctx.data(entries[0].c("value").ok_or("缺少型号按键内容")?)?;}
 data.c("actions").ok_or("缺少软件按键动作列表".into())
}
fn dpi_mode<'a>(ctx:&Context<'a>,n:&'a Node)->Result<(DpiState,Option<u16>),String>{
 if n.t("defaultModeIndex").is_some_and(|v|v!="0"){return Err("此 DPI 配置包含额外模式选择，尚未转换".into());}
 let mut all=BTreeMap::new();let mut sniper=None;let settings=n.c("settings").ok_or("缺少 DPI 档位")?;
 for entry in &settings.children{let key=entry.need("key")?;let value=ctx.data(entry.c("value").ok_or("缺少 DPI 档位内容")?)?;let x=value.number("dpiX")?;let y=value.number("dpiY")?;let independent=value.flag("independent")?;if !independent&&x!=y{return Err("DPI 数据的 XY 与独立轴标志不一致".into());}let enabled=value.flag("enabled")?;
  if key=="SniperDpi"{if !enabled{return Err("来源禁用了狙击 DPI；其原始行为尚未确认，不能保留默认狙击键后保存为已转换配置".into());}if x!=y{return Err("M65 软件狙击 DPI 暂不支持不同 XY".into());}if sniper.replace(x).is_some(){return Err("狙击 DPI 档位重复".into());}continue;}
  let id=key.strip_prefix("Dpi").and_then(|v|v.parse::<u8>().ok()).filter(|v|(1..=5).contains(v)).ok_or_else(||format!("未知 DPI 档位 {key}"))?;if all.insert(id,(enabled,DpiStage{x,y})).is_some(){return Err("DPI 档位重复".into());}
 }
 let selected=n.need("defaultIndex")?.strip_prefix("Dpi").and_then(|v|v.parse::<u8>().ok()).ok_or("未知默认 DPI 档位")?;let stages=all.iter().filter(|(_,v)|v.0).map(|(id,(_,stage))|(*id,*stage)).collect::<Vec<_>>();let active=stages.iter().position(|(id,_)|*id==selected).ok_or("默认 DPI 档位已禁用或缺失")?;Ok((DpiState{stages:stages.into_iter().map(|(_,s)|s).collect(),active:active as u8},sniper))
}

#[cfg(test)]pub(crate) mod tests{
 use super::*;
 fn object(id:u32,kind:&str,data:&str)->String{format!("<polymorphic_id>{}</polymorphic_id><polymorphic_name>{kind}</polymorphic_name><ptr_wrapper><valid>1</valid><data>{data}</data></ptr_wrapper>",id|0x80000000)}
 fn base(id:&str,hint:&str)->String{format!("<base><name>{id}</name><id>{id}</id><repeatOptions><repeatCount>1</repeatCount><repeatMode>NoRepeat</repeatMode><delay>0</delay></repeatOptions><executionHints><terminateOnSecondExec>false</terminateOnSecondExec><restartOnSecondExec>false</restartOnSecondExec><retainOriginalKeyOutput>false</retainOriginalKeyOutput><execHint>{hint}</execHint></executionHints><attachedActions/></base>")}
 pub(crate) fn fixture()->String{sample(false)}
 fn sample(proxy:bool)->String{
  // Fact-derived synthetic M65 sample. This is not a capture from M65 hardware.
  let link=object(1,"DPIProperty::Proxy","<properties><value0><key><modelId>M65 PRO RGB</modelId></key><value><ptr_wrapper><data><id>dpi-body</id></data></ptr_wrapper></value></value0></properties>");
  let shortcut=object(3,"KeyRemapAction",&format!("{}<keyName/><keyStroke><value0>LeftCtrl</value0><value1>W</value1></keyStroke><holdingKeyEnabled>false</holdingKeyEnabled>",base("shortcut","OnBoth")));
  let events=[("KeyPress","A"),("KeyPress","B"),("Delay","23"),("KeyRelease","A"),("KeyRelease","B")].into_iter().enumerate().map(|(i,(sub,key))|{let obj=if sub=="Delay"{object(6,"DelayMacroActionEvent","<useRandom>false</useRandom><delay>23</delay>")}else{object(5,"KeyboardMacroActionEvent",&format!("<keys><value0>{key}</value0></keys><sub>{sub}</sub>"))};format!("<value{i}>{obj}</value{i}>")}).collect::<String>();
  let macro_action=object(4,"MacroAction",&format!("{}<terminationMethod>Interrupt</terminationMethod><highPriority>false</highPriority><events>{events}</events>",base("overlap","OnPress")));
  let actions_data=format!("<actions><value0><first>{shortcut}</first><second><key>Mouse4</key><layer>StandardLayer</layer><event>Click</event></second></value0><value1><first>{macro_action}</first><second><first>Mouse5</first><second>Click</second></second></value1></actions>");
  let actions=if proxy{object(2,"ActionsProperty::Proxy",&format!("<properties><value0><key>M65 PRO RGB</key><value><ptr_wrapper><data>{actions_data}</data></ptr_wrapper></value></value0></properties>"))}else{object(2,"ActionsProperty",&actions_data)};
  let mut stages=String::new();for(i,key,x,y) in [(0,"Dpi1",800,1000),(1,"Dpi2",1600,1800),(2,"SniperDpi",400,400)]{stages.push_str(&format!("<value{i}><key>{key}</key><value><ptr_wrapper><data><enabled>true</enabled><independent>{}</independent><dpiX>{x}</dpiX><dpiY>{y}</dpiY></data></ptr_wrapper></value></value{i}>",x!=y));}
  format!("<cereal><value0><profile><name>Test &amp; profile</name><id>source-guid</id><linkedProgramPath>C:/Games/game.exe</linkedProgramPath><properties><value0><key>Mouse</key><value><properties><value0>{link}</value0><value1>{actions}</value1></properties></value></value0></properties></profile><dpiModes><value0><key>M65 PRO RGB</key><value><ptr_wrapper><data><id>dpi-body</id><settings>{stages}</settings><defaultIndex>Dpi2</defaultIndex><defaultModeIndex>0</defaultModeIndex></data></ptr_wrapper></value></value0></dpiModes></value0></cereal>")
 }
 #[test]fn icue_dpi_keys_and_overlapping_macro_save_and_execute_real_m65_driver(){
  let prepared=convert(&fixture()).unwrap();assert_eq!(prepared.name,"Test & profile");let p=prepared.candidates[0].profile.as_ref().unwrap_or_else(||panic!("{:?}",prepared.candidates[0].issues));assert_eq!(p.levels,vec![Level{x:800,y:Some(1000)},Level{x:1600,y:Some(1800)}]);assert_eq!(p.active_level,1);assert_eq!(p.app_path.as_deref(),Some("C:/Games/game.exe"));assert_eq!(p.multi.as_ref().unwrap().macros[0].steps,vec![MacroStep::KeyDown(4),MacroStep::KeyDown(5),MacroStep::Delay(23),MacroStep::KeyUp(4),MacroStep::KeyUp(5)]);
  let mut store=crate::profiles::ProfileStore::default();for (id,vendor,product) in [("logitech",0x046d,0xc539),("mchose",0x3837,0x4026)]{let mut other=Profile::desktop();other.id=id.into();other.target_vendor=Some(vendor);other.target_model=Some(product);crate::profile_transfer::merge(&mut store,vec![other]).unwrap();}crate::profile_transfer::merge(&mut store,vec![p.clone()]).unwrap();
  let file=std::env::temp_dir().join(format!("icue-store-{}.json",crate::profiles::new_id()));store.save(&file).unwrap();let loaded:crate::profiles::ProfileStore=serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();std::fs::remove_file(file).unwrap();assert_eq!(loaded.profiles.len(),4);let restored=loaded.get(&p.id).unwrap();assert_eq!(restored.multi,p.multi);assert!(!restored.targets(0x046d,&[0x1b2e]));assert!(!restored.targets(0x3837,&[0x1b2e]));
  let result=crate::multibrand::simulate(restored.multi.as_ref().unwrap()).unwrap();assert_eq!(result["hardware_tested"],false);assert!(result["packet_count"].as_u64().unwrap()>0);assert_eq!(result["software_macros"][1]["virtual_delay_ms"],23);assert_eq!(result["software_macros"][1]["events"],serde_json::json!(["key_down:4","key_down:5","key_up:4","key_up:5"]));assert_eq!(convert(&sample(true)).unwrap().candidates.remove(0).profile.unwrap().multi,p.multi);
 }
 #[test]fn foreign_layout_unresolved_dpi_and_unsupported_repeat_never_save_partial_profiles(){
  for source in [fixture().replace("M65 PRO RGB","SCIMITAR RGB ELITE"),fixture().replace("<id>dpi-body</id><settings>","<id>other-body</id><settings>"),fixture().replace("NoRepeat","RepeatConstantly"),fixture().replace("<defaultIndex>Dpi2</defaultIndex>","<defaultIndex>Dpi5</defaultIndex>"),fixture().replace("StandardLayer","HypershiftLayer")]{let result=convert(&source).unwrap();assert!(result.candidates.iter().all(|c|c.profile.is_none()&&!c.issues.is_empty()));}
 }
 #[test]fn malformed_xml_entities_dtd_duplicate_fields_and_stuck_keys_are_rejected(){
  assert!(convert("<!DOCTYPE cereal [<!ENTITY x SYSTEM 'file:///secret'>]><cereal/>").is_err());assert!(convert(&fixture().replace("Test &amp; profile","Test &unknown; profile")).is_err());assert!(convert(&fixture().replace("<name>Test &amp; profile</name>","<name>A</name><name>B</name>")).is_err());let source=fixture().replace("<sub>KeyRelease</sub>","<sub>KeyPress</sub>");assert!(convert(&source).unwrap().candidates[0].profile.is_none());
 }
 #[test]fn legacy_and_modern_assignments_and_type_references_have_the_same_meaning(){
  let text=fixture();let old=convert(&text).unwrap().candidates.remove(0).profile.unwrap();let text=text.replace("<polymorphic_id>2147483653</polymorphic_id><polymorphic_name>KeyboardMacroActionEvent</polymorphic_name>","<polymorphic_id>5</polymorphic_id>");let declaration="<unused><polymorphic_id>2147483653</polymorphic_id><polymorphic_name>KeyboardMacroActionEvent</polymorphic_name></unused>";let text=text.replace("<dpiModes>",&format!("{declaration}<dpiModes>"));let new=convert(&text).unwrap().candidates.remove(0).profile.unwrap();assert_eq!(old.multi,new.multi);
 }
 #[test]#[ignore="requires explicit OPEN_HUB_ICUE_SAMPLE public-source path"]fn public_icue_export_can_be_inspected_without_inventing_an_m65_layout(){
  let path=std::env::var_os("OPEN_HUB_ICUE_SAMPLE").expect("set public-source sample path");let p=prepare(std::path::Path::new(&path)).unwrap();assert_eq!(p.name,"Default");assert!(p.candidates.iter().any(|c|c.model=="SCIMITAR RGB ELITE"));assert!(p.candidates.iter().all(|c|c.profile.is_none()));let text=std::fs::read_to_string(path).unwrap();let root=xml(&text).unwrap();let ctx=Context::new(&root).unwrap();let archive=root.at(&["cereal","value0"]).unwrap();let n=ctx.data(archive.at(&["dpiModes","value0","value"]).unwrap()).unwrap();let(dpi,sniper)=dpi_mode(&ctx,n).unwrap();assert_eq!(dpi.stages.len(),5);assert_eq!(dpi.active,1);assert_eq!(dpi.stages[0],DpiStage::uniform(800));assert_eq!(sniper,Some(400));let mouse=archive.at(&["profile","properties"]).unwrap().children.iter().find(|p|p.t("key")==Some("Mouse")).unwrap().at(&["value","properties"]).unwrap();let actions=software_actions(&ctx,mouse,"SCIMITAR RGB ELITE").unwrap();let first=actions.children.first().unwrap();let(key,body)=action(&ctx,first.c("first").unwrap()).unwrap();assert_eq!(key,ButtonAction::Key{usage:0x73,mods:Modifiers(0)});assert!(body.is_none());
 }
}
fn basic(n:&Node)->Result<(&Node,&Node),String>{
 let bases=n.children.iter().filter(|b|b.name=="base"&&b.c("repeatOptions").is_some()&&b.c("executionHints").is_some()).collect::<Vec<_>>();if bases.len()!=1{return Err("动作的执行参数 base 缺失或存在歧义".into());}let base=bases[0];let repeat=base.c("repeatOptions").unwrap();let hints=base.c("executionHints").unwrap();
 if hints.flag("retainOriginalKeyOutput")?||hints.flag("restartOnSecondExec")?||hints.flag("terminateOnSecondExec")?{return Err("此动作含保留原键、重启动作或二次终止参数，尚不能无损转换".into());}
 if repeat.need("repeatMode")?!="NoRepeat"||repeat.number("repeatCount")?!=1||repeat.number("delay")?!=0{return Err("此来源的循环/重复方式尚未转换，请保留原配置".into());}
 if base.c("attachedActions").is_some_and(|a|!a.children.is_empty())||base.t("actionSoundPath").is_some_and(|s|!s.is_empty())||base.t("actionLighting").is_some_and(|s|!s.is_empty()&&s!="{00000000-0000-0000-0000-000000000000}"){return Err("此动作含附加动作、音效或关联灯效，尚未完整转换".into());}Ok((base,hints))
}
fn action<'a>(ctx:&Context<'a>,node:&'a Node)->Result<(ButtonAction,Option<Macro>),String>{
 let kind=ctx.kind(node)?;let data=ctx.data(node)?;let(base,hints)=basic(data)?;
 match kind{
  "DpiAction"=>{
   if hints.need("execHint")?!="OnPress"{return Err("DPI 动作不是按下触发，尚未转换".into());}
   let behavior=data.c("behavior").ok_or("缺少原始 DPI 动作行为")?;if ctx.kind(behavior)?!="DirectionDpiBehavior"{return Err("此 DPI 动作行为尚未转换".into());}let d=ctx.data(behavior)?;
   if d.flag("loob")?||d.flag("applyToAllModes")?{return Err("此 DPI 动作含循环或跨模式参数，尚未转换".into());}
   let forward=d.flag("forward")?;let direction=d.at(&["base","m_typeBehavior"]).map(|n|n.text.trim()).ok_or("缺少 DPI 动作方向")?;
   let f=match(direction,forward){("NextDpi",true)=>DpiFunction::Up,("PrevDpi",false)=>DpiFunction::Down,_=>return Err("DPI 动作方向字段相互矛盾或未知".into())};Ok((ButtonAction::Dpi(f),None))
  },
  "KeyRemapAction"=>{
   if data.flag("holdingKeyEnabled")?||hints.need("execHint")?!="OnBoth"{return Err("此键盘重映射含额外保持/触发方式，尚未转换".into());}
   if let Some(name)=data.t("keyName").filter(|s|!s.is_empty()){if data.c("keyStroke").is_some_and(|n|!n.children.is_empty()){return Err("动作同时提供单键和快捷键，无法判断实际行为".into());}let mouse=match name{"Mouse1"=>Some(MouseButton::Left),"Mouse2"=>Some(MouseButton::Right),"Mouse3"=>Some(MouseButton::Middle),"Mouse4"=>Some(MouseButton::Back),"Mouse5"=>Some(MouseButton::Forward),_=>None};return match mouse{Some(button)=>Ok((ButtonAction::Mouse(button),None)),None=>keyboard_action(base,&[crate::import_keys::usage(name)?])};}
   let keys=data.c("keyStroke").ok_or("缺少快捷键内容")?.children.iter().map(|key|crate::import_keys::usage(key.text.trim())).collect::<Result<Vec<_>,_>>()?;keyboard_action(base,&keys)
  },
  "MacroAction"=>{
   if hints.need("execHint")?!="OnPress"{return Err("来源宏的触发方式尚未转换".into());}if data.t("terminationMethod")!=Some("Interrupt")||data.flag("highPriority")?{return Err("来源宏的中止/优先级行为尚未转换".into());}
   let mut steps=Vec::new();for event in &data.c("events").ok_or("缺少原始宏事件")?.children{let d=ctx.data(event)?;match ctx.kind(event)?{
    "KeyboardMacroActionEvent"=>{let down=match d.need("sub")?{"KeyPress"=>true,"KeyRelease"=>false,_=>return Err("未知宏按键状态".into())};for key in &d.c("keys").ok_or("缺少宏按键")?.children{let u=crate::import_keys::usage(key.text.trim())?;steps.push(if down{MacroStep::KeyDown(u)}else{MacroStep::KeyUp(u)});}},
    "DelayMacroActionEvent"=>{if d.flag("useRandom")?{return Err("随机宏延迟尚未转换".into());}let ms=d.need("delay")?.parse::<u32>().map_err(|_|"宏延迟超出范围")?;steps.push(MacroStep::Delay(ms));},
    other=>return Err(format!("宏事件 {other} 尚未转换")),
   }}
   let body=Macro{id:format!("icue:{}",base.need("id")?),name:base.need("name")?.into(),mode:MacroMode::Once,steps};crate::host_macros::validate(&body)?;Ok((ButtonAction::Macro(body.id.clone()),Some(body)))
  },other=>Err(format!("iCUE 动作 {other} 尚未转换"))
 }
}
fn keyboard_action(base:&Node,keys:&[u8])->Result<(ButtonAction,Option<Macro>),String>{
 let mut mods=0;let mut usage=None;for &u in keys{if (0xe0..=0xe7).contains(&u){let bit=1<<(u-0xe0);if mods&bit!=0{return Err("重复修饰键".into());}mods|=bit;}else if usage.replace(u).is_some(){return Err("多个普通键不能压成一个快捷键".into());}}
 if let Some(usage)=usage{return Ok((ButtonAction::Key{usage,mods:Modifiers(mods)},None));}if mods==0{return Err("快捷键没有按键内容".into());}
 let held=(0..8).filter(|n|mods&(1<<n)!=0).map(|n|0xe0+n).collect::<Vec<u8>>();let mut steps=held.iter().copied().map(MacroStep::KeyDown).collect::<Vec<_>>();steps.push(MacroStep::WaitForRelease);steps.extend(held.iter().rev().copied().map(MacroStep::KeyUp));let body=Macro{id:format!("icue-remap:{}",base.need("id")?),name:base.need("name")?.into(),mode:MacroMode::WhileHeld,steps};crate::host_macros::validate(&body)?;Ok((ButtonAction::Macro(body.id.clone()),Some(body)))
}

#[cfg(test)]mod regression_tests{
 use super::*;
 #[test]fn source_dpi_direction_actions_execute_without_guessing_loop_or_cross_mode_behavior(){
  let behavior="<behavior><polymorphic_id>2147483655</polymorphic_id><polymorphic_name>DirectionDpiBehavior</polymorphic_name><ptr_wrapper><data><base><m_typeBehavior>NextDpi</m_typeBehavior></base><forward>true</forward><loob>false</loob><applyToAllModes>false</applyToAllModes></data></ptr_wrapper></behavior>";
  let source=tests::fixture().replace("KeyRemapAction","DpiAction").replace("<keyName/><keyStroke><value0>LeftCtrl</value0><value1>W</value1></keyStroke><holdingKeyEnabled>false</holdingKeyEnabled>",behavior).replace("<execHint>OnBoth</execHint>","<execHint>OnPress</execHint>");
  for (source,expected) in [(source.clone(),DpiFunction::Up),(source.replace("NextDpi","PrevDpi").replace("<forward>true</forward>","<forward>false</forward>"),DpiFunction::Down)]{let p=convert(&source).unwrap().candidates.remove(0).profile.expect("convert supported source DPI behavior");let plan=p.multi.unwrap();assert!(plan.settings.contains(&Setting::Button{id:4,action:ButtonAction::Dpi(expected)}));assert!(crate::multibrand::simulate(&plan).unwrap()["packet_count"].as_u64().unwrap()>0);}
  for invalid in [source.replace("<loob>false</loob>","<loob>true</loob>"),source.replace("<applyToAllModes>false</applyToAllModes>","<applyToAllModes>true</applyToAllModes>"),source.replace("<forward>true</forward>","<forward>false</forward>"),source.replace("NextDpi","UnknownDirection"),source.replace("DirectionDpiBehavior","UnknownBehavior")]{assert!(convert(&invalid).unwrap().candidates[0].profile.is_none());}
 }
 #[test]fn disabled_or_duplicate_sniper_dpi_never_silently_keeps_the_target_default(){let source=tests::fixture();let disabled=source.replace("<enabled>true</enabled><independent>false</independent><dpiX>400</dpiX>","<enabled>false</enabled><independent>false</independent><dpiX>400</dpiX>");assert_ne!(disabled,source);let p=convert(&disabled).unwrap();assert!(p.candidates[0].profile.is_none());assert!(p.candidates[0].issues.iter().any(|e|e.contains("禁用了狙击")));let duplicate=source.replace("</settings>","<value3><key>SniperDpi</key><value><ptr_wrapper><data><enabled>true</enabled><independent>false</independent><dpiX>800</dpiX><dpiY>800</dpiY></data></ptr_wrapper></value></value3></settings>");let p=convert(&duplicate).unwrap();assert!(p.candidates[0].profile.is_none());assert!(p.candidates[0].issues.iter().any(|e|e.contains("狙击 DPI 档位重复")));}
 #[test]fn conflicting_source_macro_identifiers_cannot_silently_use_the_first_body(){let source=tests::fixture();let start=source.find("<value1><first><polymorphic_id>2147483652").unwrap();let end=start+source[start..].find("</actions>").unwrap();let row=&source[start..end];let row=row.strip_suffix("</value1>").unwrap().replacen("<value1>","<value2>",1).replace("<first>Mouse5</first>","<first>Mouse6</first>").replace("<value0>A</value0>","<value0>C</value0>")+"</value2>";let source=source.replace("</actions>",&format!("{row}</actions>"));let result=convert(&source).unwrap();assert!(result.candidates[0].profile.is_none());assert!(result.candidates[0].issues.iter().any(|e|e.contains("内容冲突")));}
 #[test]fn modifier_only_remap_keeps_right_left_identity_and_release_semantics(){let root=xml("<cereal><base><name>modifiers</name><id>m</id></base></cereal>").unwrap();let base=root.at(&["cereal","base"]).unwrap();let(action,body)=keyboard_action(base,&[0xe0,0xe5]).unwrap();let body=body.unwrap();assert_eq!(action,ButtonAction::Macro(body.id.clone()));assert_eq!(body.mode,MacroMode::WhileHeld);assert_eq!(body.steps,vec![MacroStep::KeyDown(224),MacroStep::KeyDown(229),MacroStep::WaitForRelease,MacroStep::KeyUp(229),MacroStep::KeyUp(224)]);let result=crate::multibrand::simulate(&Plan{vendor:0x1b1c,product:0x1b2e,settings:vec![Setting::Button{id:4,action}],macros:vec![body],source:None}).unwrap();assert_eq!(result["software_macros"][0]["events"],serde_json::json!(["key_down:224","key_down:229","key_up:229","key_up:224"]));}
 #[test]fn cereal_empty_multiple_base_metadata_is_supported_but_execution_ambiguity_is_rejected(){let root=xml("<cereal><action><base/><base><name>a</name><id>b</id><repeatOptions><repeatMode>NoRepeat</repeatMode><repeatCount>1</repeatCount><delay>0</delay></repeatOptions><executionHints><retainOriginalKeyOutput>false</retainOriginalKeyOutput><restartOnSecondExec>false</restartOnSecondExec><terminateOnSecondExec>false</terminateOnSecondExec><execHint>OnBoth</execHint></executionHints></base></action></cereal>").unwrap();let node=root.at(&["cereal","action"]).unwrap();assert_eq!(basic(node).unwrap().0.t("id"),Some("b"));let ambiguous=xml("<cereal><action><base><repeatOptions/><executionHints/></base><base><repeatOptions/><executionHints/></base></action></cereal>").unwrap();assert!(basic(ambiguous.at(&["cereal","action"]).unwrap()).is_err());}
}
