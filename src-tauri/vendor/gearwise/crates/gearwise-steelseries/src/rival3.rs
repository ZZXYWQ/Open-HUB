//! Original Rival 3 only. Four independent colours, one global firmware mode.
//! Protocol facts: pinned rivalcfg rival3.py; no vendor application execution.
use crate::link::Packet;
use gearwise_core::{DeviceState,Error,Result,Setting};
use gearwise_core::model::{LightingEffect,Rgb,ZoneState};
use serde::{Deserialize,Serialize};
pub const SCHEMA:&str="steelseries-rival3-lighting/1";
/// Exact inverse of the reviewed TrueMove Core encoding; unknown codes stay invalid.
pub fn dpi_from_code(code:u8)->Option<u16>{(200..=8500).step_by(100).find(|dpi|crate::cpi::CORE.resolve(*dpi).is_ok_and(|(_,c)|c==u16::from(code)))}
pub fn supported(pid:u16)->bool{matches!(pid,0x1824|0x184c)}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Zone {pub color:Rgb,pub brightness:u8}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {pub enabled:bool,pub mode:u8,pub zones:[Zone;4]}
impl Default for Profile {
 fn default()->Self{Self{enabled:true,mode:4,zones:[[255,0,0],[0,255,0],[0,0,255],[128,0,128]].map(|[r,g,b]|Zone{color:Rgb{r,g,b},brightness:100})}}
}
pub fn parse(schema:&str,data:&serde_json::Value)->Result<Profile>{
 if schema!=SCHEMA{return Err(Error::Unsupported("Rival3 lighting schema"));}
 let p:Profile=serde_json::from_value(data.clone()).map_err(|e|Error::invalid(format!("Rival3 complete lighting: {e}")))?;
 if p.mode>6||p.zones.iter().any(|z|z.brightness>100){return Err(Error::invalid("Rival3 mode0..6 and brightness0..100 required"));}
 Ok(p)
}
pub fn setting(p:&Profile)->Setting{Setting::VendorProfile{schema:SCHEMA.into(),data:serde_json::to_value(p).expect("bounded Rival3 lighting")}}
pub fn plan(pid:u16,schema:&str,data:&serde_json::Value)->Result<(Setting,Vec<Packet>)>{
 if !supported(pid){return Err(Error::Unsupported("exact Rival3 lighting model"));}
 let p=parse(schema,data)?;
 let mut packets=p.zones.iter().enumerate().map(|(i,z)|{
   let c=if p.enabled{z.color}else{Rgb::BLACK};
   Packet::output(vec![5,0,(i+1)as u8,c.r,c.g,c.b,if p.enabled{z.brightness}else{0}])
 }).collect::<Vec<_>>();
 packets.push(Packet::output(vec![6,0,if p.enabled{p.mode}else{4}]));
 Ok((setting(&p),packets))
}
/// GG122 original1824 set_all_colors, admitted only after this connection has
/// successfully established full-brightness steady state. The command has no
/// brightness field, so templates and dimmed/disabled palettes cannot prime it.
pub(crate) fn plan_after_write(pid:u16,schema:&str,data:&serde_json::Value,previous:Option<&serde_json::Value>)->Result<(Setting,Vec<Packet>)>{
 let full=plan(pid,schema,data)?;
 let eligible=|p:&Profile|p.enabled&&p.mode==4&&p.zones.iter().all(|z|z.brightness==100);
 let want=parse(schema,data)?;
 if pid!=0x1824||!eligible(&want)||!previous.and_then(|v|parse(SCHEMA,v).ok()).is_some_and(|p|eligible(&p)){return Ok(full);}
 let mut bytes=vec![0x0a,0,15];for z in &want.zones{bytes.extend([z.color.r,z.color.g,z.color.b]);}bytes.resize(32,0);
 let mut mode=vec![6,0,4];mode.resize(32,0);
 Ok((full.0,vec![Packet::output(bytes),Packet::output(mode)]))
}
/// Screen/music synchronisation may choose one static zone. This switches the
/// shared mode to steady; the complete desired palette is sent explicitly.
pub fn static_zone(current:&serde_json::Value,zone:u8,want:&ZoneState)->Result<Setting>{
 let mut p=parse(SCHEMA,current)?;
 let target=p.zones.get_mut(usize::from(zone)).ok_or(Error::invalid("Rival3 lighting zone"))?;
 let color=match want.effect{LightingEffect::Static{color}=>color,LightingEffect::Off=>Rgb::BLACK,_=>return Err(Error::Unsupported("Rival3 native patterns use the shared lighting editor"))};
 if want.brightness>100{return Err(Error::invalid("Rival3 brightness"));}
 target.color=color;target.brightness=want.brightness;p.enabled=true;p.mode=4;Ok(setting(&p))
}
pub fn cache(state:&mut DeviceState,data:&serde_json::Value)->Result<()>{
 let p=parse(SCHEMA,data)?;state.vendor_profile=Some(data.clone());state.lighting.clear();
 // Dynamic firmware modes have no proven millisecond period in the source.
 // Keep their exact mode in vendor_profile instead of inventing a generic one.
 if !p.enabled||p.mode==4{for(i,z)in p.zones.iter().enumerate(){state.lighting.insert(i as u8,ZoneState{effect:if p.enabled{LightingEffect::Static{color:z.color}}else{LightingEffect::Off},brightness:z.brightness});}}
 Ok(())
}

#[cfg(test)]mod tests{
 use super::*;
 #[test]fn seven_modes_and_off_have_independent_complete_packets(){
  for mode in 0..=6{let p=Profile{mode,zones:[Zone{color:Rgb{r:17,g:34,b:51},brightness:37},Zone{color:Rgb{r:4,g:5,b:6},brightness:0},Zone{color:Rgb{r:7,g:8,b:9},brightness:75},Zone{color:Rgb{r:10,g:11,b:12},brightness:100}],..Default::default()};for pid in[0x1824,0x184c]{let(_,packets)=plan(pid,SCHEMA,&serde_json::to_value(&p).unwrap()).unwrap();let expected=[vec![5,0,1,17,34,51,37],vec![5,0,2,4,5,6,0],vec![5,0,3,7,8,9,75],vec![5,0,4,10,11,12,100],vec![6,0,mode]];assert_eq!(packets.iter().map(|p|p.bytes.clone()).collect::<Vec<_>>(),expected);}}
  let mut p=Profile::default();p.enabled=false;let(_,packets)=plan(0x1824,SCHEMA,&serde_json::to_value(&p).unwrap()).unwrap();for (i,packet)in packets[..4].iter().enumerate(){assert_eq!(packet.bytes,vec![5,0,(i+1)as u8,0,0,0,0]);}assert_eq!(packets[4].bytes,[6,0,4]);
 }
 #[test]fn malformed_or_foreign_profiles_never_become_packets(){
  let base=serde_json::to_value(Profile::default()).unwrap();for n in 0..7{let mut d=base.clone();match n{0=>d["mode"]=7.into(),1=>d["enabled"]=1.into(),2=>d["zones"][0]["brightness"]=101.into(),3=>d["zones"][0]["color"]["r"]=256.into(),4=>d["zones"]=serde_json::json!([]),5=>d["unknown"]=1.into(),_=>d["zones"][0]["unknown"]=1.into()};assert!(plan(0x1824,SCHEMA,&d).is_err());}for pid in[0x1830,0x1720,0x1724]{assert!(plan(pid,SCHEMA,&base).is_err());}assert!(plan(0x1824,"wrong",&base).is_err());
 }
 #[test]fn dynamic_mode_cache_and_static_zone_switch_keep_exact_palette(){
  let mut p=Profile::default();p.mode=6;let mut s=DeviceState::default();cache(&mut s,&serde_json::to_value(&p).unwrap()).unwrap();assert!(s.lighting.is_empty());let Setting::VendorProfile{data,..}=static_zone(s.vendor_profile.as_ref().unwrap(),2,&ZoneState{effect:LightingEffect::Static{color:Rgb{r:1,g:2,b:3}},brightness:39}).unwrap()else{panic!()};cache(&mut s,&data).unwrap();assert_eq!(s.lighting.len(),4);assert_eq!(s.lighting[&2].brightness,39);let applied=parse(SCHEMA,&data).unwrap();assert_eq!(applied.mode,4);assert_eq!(applied.zones[0],p.zones[0]);assert_eq!(applied.zones[3],p.zones[3]);
 }
}
