//! Native macro compilation, whole-bank reference planning and multi-sector
//! verified commits. Never routes a native program through OS input.
use crate::{device::Device,hidpp::{Error,FeatureRequest,Result},onboard::{Info,sector_crc_ok},onboard_macros::{Address,inspect}};
use gearwise_core::{profile::{Macro,MacroMode,MacroStep},model::MouseButton};
use std::collections::{BTreeMap,BTreeSet};
use serde::Serialize;
fn fail(s:&str)->Error{Error::Other(format!("Logitech macro planning: {s}"))}
/// Once-only instruction groups. Atomic shortcuts and media taps are kept
/// together when a planner inserts cross-page links.
fn groups(m:&Macro,sector_size:u16)->Result<Vec<Vec<u8>>>{
 crate::host_macros::validate(m).map_err(Error::Other)?;
 if m.name.trim().is_empty(){return Err(fail("macro needs a name"));}
 if m.mode!=MacroMode::Once{return Err(fail("native repeat-mode layout is not enabled"));}
 if !(210..=4096).contains(&sector_size){return Err(fail("unsupported sector size"));}
 let mut out=Vec::new();let mut held_modifiers=0u8;
 for step in &m.steps{
  if let MacroStep::KeyTap{mods,..}=step{if mods.0&held_modifiers!=0{return Err(fail("atomic shortcut overlaps an explicitly held modifier; release it or use separate key events"));}}
  let item=match step{
  MacroStep::KeyDown(k)|MacroStep::KeyUp(k) if (0xe0..=0xe7).contains(k)=>vec![if matches!(step,MacroStep::KeyDown(_)){0x43}else{0x44},1u8<<(*k-0xe0),0],
  MacroStep::KeyDown(k)|MacroStep::KeyUp(k) if (4..=0xa4).contains(k)=>vec![if matches!(step,MacroStep::KeyDown(_)){0x43}else{0x44},0,*k],
  MacroStep::KeyTap{usage,mods}=>vec![0x43,mods.0,*usage,0x44,mods.0,*usage],
  MacroStep::MouseDown(b)|MacroStep::MouseUp(b)=>{let mask=match b{MouseButton::Left=>1u16,MouseButton::Right=>2,MouseButton::Middle=>4,MouseButton::Back=>8,MouseButton::Forward=>16,_=>return Err(fail("native wheel/mouse operand is not verified"))};let bytes=mask.to_be_bytes();vec![if matches!(step,MacroStep::MouseDown(_)){0x41}else{0x42},bytes[0],bytes[1]]},
  MacroStep::Media(usage)=>{if !(1..=0x3ff).contains(usage){return Err(fail("unsupported consumer usage"));}let b=usage.to_be_bytes();vec![0x45,b[0],b[1],0x46,b[0],b[1]]},
  MacroStep::Delay(ms)=>{if *ms==0{vec![]}else{let b=u16::try_from(*ms).map_err(|_|fail("native delay overflow"))?.to_be_bytes();vec![0x40,b[0],b[1]]}},
  _=>return Err(fail("text and physical-wait are not compiled")),
 };
 match step{MacroStep::KeyDown(k) if (0xe0..=0xe7).contains(k)=>held_modifiers|=1<<(*k-0xe0),MacroStep::KeyUp(k) if (0xe0..=0xe7).contains(k)=>held_modifiers&=!(1<<(*k-0xe0)),_=>{}}
 if !item.is_empty(){out.push(item);}}
 if out.is_empty(){return Err(fail("macro has no input or timed event"));}
 if out.iter().map(|v|v.len()/3).sum::<usize>()+1>crate::onboard_macros::MAX_STEPS{return Err(fail("native instruction budget exceeded"));}
 Ok(out)
}
pub fn compile(m:&Macro,sector_size:u16)->Result<Vec<u8>>{let mut bytes:Vec<_>=groups(m,sector_size)?.into_iter().flatten().collect();bytes.push(0xff);Ok(bytes)}
#[derive(Debug,Serialize)]pub struct Reference{pub slot:u8,pub layer:&'static str,pub button:u8,pub entry:Address,pub complete:bool}
#[derive(Debug,Serialize)]pub struct Inventory{pub references:Vec<Reference>,pub protected_sectors:BTreeSet<u16>,pub erased_unreferenced:Vec<u16>,pub blockers:Vec<String>}
/// All live sectors, including unused macro pages. Unknown/partial reachable
/// programs block allocation; a page full of FF is not free if any slot or
/// layer references it. Unreferenced nonempty factory pages are never erased.
pub fn inventory(info:&Info,pages:&BTreeMap<u16,Vec<u8>>)->Result<Inventory>{
 crate::onboard_bindings::validate_info(info).map_err(Error::Other)?;
 if !matches!(info.sector_size,255|256)||info.sector_count==0||info.sector_count>32||info.profile_count>=info.sector_count{return Err(fail("whole-bank planning requires verified 255/256-byte geometry and at most 32 sectors"));}
 for sector in 0..u16::from(info.sector_count){if pages.get(&sector).is_none_or(|p|p.len()!=usize::from(info.sector_size)){return Err(fail("complete, exact-length live memory capture required"));}}
 let directory=&pages[&0];if !sector_crc_ok(directory){return Err(fail("directory CRC failed"));}
 let mut out=Inventory{references:vec![],protected_sectors:(0..=u16::from(info.profile_count)).collect(),erased_unreferenced:vec![],blockers:vec![]};
 let mut profile_sectors=BTreeSet::new();let mut programs=BTreeMap::<Address,(bool,Vec<u16>,Option<String>)>::new();
 for i in 0..info.profile_count{let at=usize::from(i)*4;let sector=u16::from_be_bytes([directory[at],directory[at+1]]);if sector==0xffff{break;}
  if sector==0||sector>=u16::from(info.sector_count)||!profile_sectors.insert(sector){return Err(fail("unknown or aliased profile directory sector"));}
  let profile=&pages[&sector];if !sector_crc_ok(profile){return Err(fail("a live profile CRC failed"));}out.protected_sectors.insert(sector);
  // Inspect all stored physical records, including disabled slots and records
  // beyond the currently advertised button count. They are not reclaimed.
  for (layer,base) in [("Normal",0x20),("GShift",0x60)]{for button in 0..16{let at=base+button*4;let raw=&profile[at..at+4];match raw[0]{
   0=>{let entry=Address{sector:u16::from(raw[1]),offset:u16::from_be_bytes([raw[2],raw[3]])};if !programs.contains_key(&entry){let trace=inspect(entry,info,|s|pages.get(&s).cloned().ok_or_else(||fail("referenced page missing")))?;programs.insert(entry,(trace.complete,trace.pages.iter().map(|p|p.sector).collect(),trace.note));}
    let (complete,reachable,note)=&programs[&entry];out.protected_sectors.extend(reachable);if !complete{out.blockers.push(format!("slot {} {layer} button {}: {}",i+1,button+1,note.as_deref().unwrap_or("partial macro")));}
    out.references.push(Reference{slot:i+1,layer,button:button as u8+1,entry,complete:*complete});
   }
   0x80 if raw[1]<=3=>{},0x90 if (1..=11).contains(&raw[1])=>{},0xff=>{},
   _=>out.blockers.push(format!("slot {} {layer} button {} has unverified binding type 0x{:02x}",i+1,button+1,raw[0])),
  }}}
 }
 for sector in u16::from(info.profile_count)+1..u16::from(info.sector_count){let bytes=&pages[&sector];let empty=bytes.iter().all(|b|*b==0xff)||(bytes[..bytes.len()-2].iter().all(|b|*b==0xff)&&sector_crc_ok(bytes));if !out.protected_sectors.contains(&sector)&&empty{out.erased_unreferenced.push(sector);}}
 Ok(out)
}
#[derive(Debug)]pub struct Bank{pub device_key:String,pub info:Info,pub pages:BTreeMap<u16,Vec<u8>>,pub inventory:Inventory}
/// Bounded, read-only capture. Re-read every live sector and identity before
/// returning a planning baseline; no write can be reached from this function.
pub fn read_bank(d:&Device,t:&dyn FeatureRequest,expected:&str)->Result<Bank>{
 crate::onboard_bindings::validate_target(d).map_err(Error::Other)?;
 d.verify_native_identity(t,expected)?;
 if !expected.starts_with("unit:"){return Err(fail("new macro planning requires a stable native unit-id"));}
 let info=d.onboard_info(t)?;
 crate::onboard_bindings::validate_info(&info).map_err(Error::Other)?;
 if !matches!(info.sector_size,255|256)||info.sector_count==0||info.sector_count>32{return Err(fail("unverified macro bank geometry"));}
 let mut pages=BTreeMap::new();for sector in 0..u16::from(info.sector_count){d.verify_native_identity(t,expected)?;pages.insert(sector,d.read_sector(t,sector,info.sector_size)?);}
 let inventory=inventory(&info,&pages)?;
 for (sector,bytes) in &pages{d.verify_native_identity(t,expected)?;if &d.read_sector(t,*sector,info.sector_size)?!=bytes{return Err(fail("memory changed during planning; baseline discarded"));}}
 d.verify_native_identity(t,expected)?;Ok(Bank{device_key:expected.into(),info,pages,inventory})
}

#[derive(Clone,Debug,serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request{pub selection:crate::onboard_macros::Selection,pub definition:Macro}
#[derive(Debug,Serialize)]pub struct Preview{pub macro_sector:u16,pub slot:u8,pub profile_sector:u16,pub button:u8,pub byte_count:usize,pub page_count:usize}
pub struct Prepared{bank:Bank,selection:crate::onboard_macros::Selection,macro_sector:u16,macro_pages:BTreeMap<u16,Vec<u8>>,profile:Vec<u8>}
impl Prepared{
 pub fn preview(&self)->Preview{Preview{macro_sector:self.macro_sector,slot:self.selection.slot,profile_sector:self.selection.sector,button:self.selection.button,byte_count:self.macro_pages.iter().map(|(sector,payload)|payload.iter().zip(&self.bank.pages[sector]).filter(|(a,b)|a!=b).count()).sum(),page_count:self.macro_pages.len()}}
 pub fn raw(&self)->Vec<crate::onboard::RawSector>{self.bank.pages.iter().map(|(sector,bytes)|crate::onboard::RawSector{sector:*sector,crc_valid:Some(sector_crc_ok(bytes)),hex:bytes.iter().map(|b|format!("{b:02x}")).collect()}).collect()}
}
pub fn prepare(d:&Device,t:&dyn FeatureRequest,r:&Request)->Result<Prepared>{
 // Validate the supported subset before any device reads.
 compile(&r.definition,256)?;
 let s=&r.selection;d.verify_native_preview(t,&s.expected_device,&s.expected_connection)?;let bank=read_bank(d,t,&s.expected_device)?;
 if s.slot==0||s.slot>bank.info.profile_count||s.button==0||s.button>bank.info.button_count{return Err(fail("unknown profile or physical button"));}
 if s.layer==crate::onboard_bindings::Layer::GShift&&bank.info.mechanical_layout&3!=2{return Err(fail("G-Shift is not reported"));}
 let i=usize::from(s.slot-1)*4;let directory=&bank.pages[&0];if u16::from_be_bytes([directory[i],directory[i+1]])!=s.sector{return Err(fail("reviewed slot no longer owns its sector"));}
 let original=bank.pages.get(&s.sector).ok_or_else(||fail("profile sector missing"))?;let at=(if s.layer==crate::onboard_bindings::Layer::Normal{0x20}else{0x60})+usize::from(s.button-1)*4;
 if original[at..at+4]!=s.expected{return Err(fail("physical binding changed since preview"));}
 let plan=plan(&bank,&r.definition)?;let mut profile=original.clone();profile[at..at+4].copy_from_slice(&[0,plan.macro_sector as u8,0,0]);
 if !crate::onboard_bindings::has_normal_left_click(&profile,&bank.info){return Err(fail("at least one normal physical left click must remain"));}
 let n=profile.len();let crc=crate::onboard::crc_ccitt(&profile[..n-2]);profile[n-2..].copy_from_slice(&crc.to_be_bytes());
 Ok(Prepared{bank,selection:s.clone(),macro_sector:plan.macro_sector,macro_pages:plan.pages,profile})
}
fn capture(d:&Device,t:&dyn FeatureRequest,p:&Prepared)->Result<BTreeMap<u16,Vec<u8>>>{
 d.verify_native_preview(t,&p.bank.device_key,&p.selection.expected_connection)?;if d.onboard_info(t)?!=p.bank.info{return Err(fail("native memory descriptor changed"));}
 let mut pages=BTreeMap::new();for sector in p.bank.pages.keys(){d.verify_native_preview(t,&p.bank.device_key,&p.selection.expected_connection)?;pages.insert(*sector,d.read_sector(t,*sector,p.bank.info.sector_size)?);}
 d.verify_native_preview(t,&p.bank.device_key,&p.selection.expected_connection)?;Ok(pages)
}
fn verify(d:&Device,t:&dyn FeatureRequest,p:&Prepared,expected:&BTreeMap<u16,Vec<u8>>)->Result<()>{if &capture(d,t,p)?!=expected{return Err(fail("live bank differs from reviewed transaction state"));}Ok(())}
#[derive(Debug,Serialize)]pub struct Committed{pub preview:Preview,pub active_reloaded:bool,pub reload_note:Option<String>}
/// Caller must persist p.raw() before entering commit. An erased macro page
/// chain is written and reconciled first; the binding can only point to verified code.
pub fn commit(d:&Device,t:&dyn FeatureRequest,p:&Prepared)->Result<Committed>{
 verify(d,t,p,&p.bank.pages)?;
 let mut stage=p.bank.pages.clone();
 let result=(||->Result<()>{
  for (sector,payload) in &p.macro_pages{d.write_sector(t,*sector,payload)?;stage.insert(*sector,payload.clone());verify(d,t,p,&stage)?;}
  d.write_sector(t,p.selection.sector,&p.profile)?;stage.insert(p.selection.sector,p.profile.clone());verify(d,t,p,&stage)
 })();
 if let Err(error)=result{
  let recovery=(||->Result<()>{let mut actual=capture(d,t,p)?;
   for (sector,bytes) in &actual{let allowed=if let Some(payload)=p.macro_pages.get(sector){bytes==&p.bank.pages[sector]||bytes==payload}else if *sector==p.selection.sector{bytes==&p.bank.pages[sector]||bytes==&p.profile}else{bytes==&p.bank.pages[sector]};if !allowed{return Err(fail("third-party or partial memory state; no blind restoration"));}}
   if actual[&p.selection.sector]!=p.bank.pages[&p.selection.sector]{d.write_sector(t,p.selection.sector,&p.bank.pages[&p.selection.sector])?;actual.insert(p.selection.sector,p.bank.pages[&p.selection.sector].clone());verify(d,t,p,&actual)?;}
   for sector in p.macro_pages.keys(){if actual[sector]!=p.bank.pages[sector]{d.write_sector(t,*sector,&p.bank.pages[sector])?;actual.insert(*sector,p.bank.pages[sector].clone());verify(d,t,p,&actual)?;}}
   verify(d,t,p,&p.bank.pages)
  })();return Err(fail(&format!("macro save failed ({error}); {}",match recovery{Ok(())=>"original bank restored/verified".into(),Err(e)=>format!("state uncertain, keep backup and refresh without retry: {e}")})));
 }
 let reload=(||->Result<bool>{d.verify_native_preview(t,&p.bank.device_key,&p.selection.expected_connection)?;let mode=d.call_unpadded(t,0x8100,2,&[])?;if mode.first()!=Some(&1){return Ok(false);}let active=d.call_unpadded(t,0x8100,4,&[])?;if active.len()<2{return Err(fail("short active profile reply"));}if active[1]!=p.selection.slot{return Ok(false);}d.verify_native_preview(t,&p.bank.device_key,&p.selection.expected_connection)?;d.call_unpadded(t,0x8100,3,&[0,p.selection.slot])?;d.verify_native_preview(t,&p.bank.device_key,&p.selection.expected_connection)?;Ok(true)})();
 let (active_reloaded,reload_note)=match reload{Ok(v)=>(v,None),Err(e)=>(false,Some(format!("macro and binding saved and verified; slot reload not confirmed: {e}")))};
 Ok(Committed{preview:p.preview(),active_reloaded,reload_note})
}

#[derive(Debug)]pub struct Plan{pub macro_sector:u16,pub pages:BTreeMap<u16,Vec<u8>>}
pub fn plan(bank:&Bank,m:&Macro)->Result<Plan>{
 if !bank.inventory.blockers.is_empty(){return Err(fail("unknown or partial native references prevent allocation; factory macros retained"));}
 let groups=groups(m,bank.info.sector_size)?;let capacity=usize::from(bank.info.sector_size)-2;
 let mut remaining=groups.iter().map(Vec::len).sum::<usize>();let mut at=0;let mut chunks=Vec::new();
 while at<groups.len(){let last=remaining+1<=capacity;let limit=capacity-if last{1}else{5};let mut chunk=Vec::new();let start=at;
  while at<groups.len()&&chunk.len()+groups[at].len()<=limit{chunk.extend(&groups[at]);remaining-=groups[at].len();at+=1;}
  if at==start{return Err(fail("native event group cannot fit into a physical page"));}chunks.push(chunk);
 }
 if chunks.len()>crate::onboard_macros::MAX_PAGES||groups.iter().map(|g|g.len()/3).sum::<usize>()+chunks.len()>crate::onboard_macros::MAX_STEPS{return Err(fail("native chain would exceed the validated reader budget"));}
 if bank.inventory.erased_unreferenced.len()<chunks.len(){return Err(fail("not enough unreferenced erased macro pages; factory data retained"));}
 let sectors=&bank.inventory.erased_unreferenced[..chunks.len()];let mut pages=BTreeMap::new();
 for (i,mut code) in chunks.into_iter().enumerate(){if i+1<sectors.len(){code.push(0x60);code.extend(sectors[i+1].to_be_bytes());code.extend([0,0]);}else{code.push(0xff);}
  let mut payload=vec![0xff;usize::from(bank.info.sector_size)];payload[..code.len()].copy_from_slice(&code);let n=payload.len();let crc=crate::onboard::crc_ccitt(&payload[..n-2]);payload[n-2..].copy_from_slice(&crc.to_be_bytes());pages.insert(sectors[i],payload);
 }
 Ok(Plan{macro_sector:sectors[0],pages})
}
#[cfg(test)]mod tests{
 use super::*;use gearwise_core::model::Modifiers;
 fn info()->Info{Info{memory_model:1,profile_format:3,macro_format:1,profile_count:2,rom_profile_count:0,button_count:5,sector_count:8,sector_size:256,mechanical_layout:10,various_info:1,bindings_writable:true}}
 fn crc(mut p:Vec<u8>)->Vec<u8>{let n=p.len();let c=crate::onboard::crc_ccitt(&p[..n-2]);p[n-2..].copy_from_slice(&c.to_be_bytes());p}
 fn bank()->Bank{
  let mut pages:BTreeMap<_,_>=(0..8u16).map(|s|(s,vec![0xff;256])).collect();let mut dir=pages[&0].clone();dir[..8].copy_from_slice(&[0,1,1,0,0,2,0,0]);pages.insert(0,crc(dir));
  for sector in [1,2]{let mut p=pages[&sector].clone();p[0x20..0x24].copy_from_slice(&[128,1,0,1]);pages.insert(sector,crc(p));}
  // A macro in a disabled profile's last G-Shift record protects an erased
  // page even though the device advertises only five physical buttons.
  let mut p=pages[&2].clone();p[0x9c..0xa0].copy_from_slice(&[0,3,0,0]);pages.insert(2,crc(p));
  let i=info();let inventory=inventory(&i,&pages).unwrap();Bank{device_key:"unit:046d:01020304".into(),info:i,pages,inventory}
 }
 fn action(steps:Vec<MacroStep>)->Macro{Macro{id:"m".into(),name:"test".into(),mode:MacroMode::Once,steps}}
 #[test]fn native_once_compiler_has_exact_press_release_delay_consumer_and_mouse_vectors(){
  let m=action(vec![MacroStep::KeyTap{usage:6,mods:Modifiers(0x11)},MacroStep::Delay(77),MacroStep::Media(0xe2),MacroStep::MouseDown(MouseButton::Back),MacroStep::MouseUp(MouseButton::Back)]);
  let bytes=compile(&m,256).unwrap();assert_eq!(bytes,vec![0x43,0x11,6,0x44,0x11,6,0x40,0,77,0x45,0,0xe2,0x46,0,0xe2,0x41,0,8,0x42,0,8,0xff]);
  let mut page=vec![255;256];page[..bytes.len()].copy_from_slice(&bytes);let tr=inspect(Address{sector:4,offset:0},&info(),|_|Ok(page.clone())).unwrap();assert!(tr.complete);assert_eq!(tr.steps.len(),8);
 }
 #[test]fn unsupported_modes_unbalanced_events_text_and_capacity_never_truncate(){
  let mut m=action(vec![MacroStep::Media(0xe2)]);m.mode=MacroMode::Toggle;assert!(compile(&m,256).is_err());m.mode=MacroMode::WhileHeld;assert!(compile(&m,256).is_err());
  for steps in [vec![MacroStep::KeyDown(4)],vec![MacroStep::Text("x".into())],vec![MacroStep::KeyDown(0xe0),MacroStep::KeyUp(0xe1)],vec![MacroStep::Delay(0)]]{assert!(compile(&action(steps),256).is_err());}
 }
 #[test]fn disabled_slots_and_unadvertised_stored_records_protect_shared_macro_pages(){
  let bank=bank();assert!(bank.inventory.protected_sectors.contains(&3));assert_eq!(bank.inventory.references[0].button,16);assert_eq!(bank.inventory.references[0].layer,"GShift");assert_eq!(bank.inventory.erased_unreferenced,vec![4,5,6,7]);let p=plan(&bank,&action(vec![MacroStep::Media(0xe2)])).unwrap();assert_eq!(p.macro_sector,4);assert!(sector_crc_ok(&p.pages[&p.macro_sector]));assert_eq!(&p.pages[&p.macro_sector][..7],&[0x45,0,0xe2,0x46,0,0xe2,0xff]);
 }
 #[test]fn nonempty_factory_pages_and_unknown_reachable_programs_are_not_reclaimed(){
  let mut b=bank();b.pages.get_mut(&4).unwrap()[0]=0;let inventory=inventory(&b.info,&b.pages).unwrap();assert_eq!(inventory.erased_unreferenced,vec![5,6,7]);b.inventory=inventory;
  b.pages.get_mut(&3).unwrap()[0]=0x20;b.inventory=super::inventory(&b.info,&b.pages).unwrap();assert!(!b.inventory.blockers.is_empty());assert!(plan(&b,&action(vec![MacroStep::Media(0xe2)])).is_err());
 }
 #[test]fn cross_page_roots_from_other_slots_reserve_their_target_page(){
  let mut b=bank();b.pages.get_mut(&3).unwrap()[..5].copy_from_slice(&[0x60,0,6,0,0]);let i=inventory(&b.info,&b.pages).unwrap();assert!(i.protected_sectors.contains(&6));assert!(!i.erased_unreferenced.contains(&6));assert!(i.blockers.is_empty());
 }
 #[test]fn missing_capture_crc_failure_and_directory_aliases_reject_planning(){
  let mut b=bank();b.pages.remove(&7);assert!(inventory(&b.info,&b.pages).is_err());let mut b=bank();b.pages.get_mut(&1).unwrap()[17]^=1;assert!(inventory(&b.info,&b.pages).is_err());let mut b=bank();let mut dir=b.pages[&0].clone();dir[5]=1;b.pages.insert(0,crc(dir));assert!(inventory(&b.info,&b.pages).is_err());
 }
 #[test]fn validated_crc_materialized_empty_pages_are_free_but_bad_crc_tail_is_retained(){
  let mut b=bank();b.pages.insert(4,crc(vec![0xff;256]));let i=inventory(&b.info,&b.pages).unwrap();assert_eq!(i.erased_unreferenced.first(),Some(&4));b.pages.get_mut(&4).unwrap()[255]^=1;let i=inventory(&b.info,&b.pages).unwrap();assert!(!i.erased_unreferenced.contains(&4));
 }

 #[test]fn long_macro_uses_five_byte_links_without_splitting_atomic_input_groups(){
  for size in [255,256]{let mut b=bank();if size==255{b.info.sector_size=size;for bytes in b.pages.values_mut(){bytes.truncate(255);}for sector in [0,1,2]{b.pages.insert(sector,crc(b.pages[&sector].clone()));}b.inventory=inventory(&b.info,&b.pages).unwrap();}
   let m=action(vec![MacroStep::Media(0xe2);43]);let p=plan(&b,&m).unwrap();assert_eq!(p.pages.len(),2);let head=&p.pages[&p.macro_sector];assert_eq!(&head[246..251],&[0x60,0,5,0,0]);assert!(sector_crc_ok(head));let tr=inspect(Address{sector:p.macro_sector,offset:0},&b.info,|s|Ok(p.pages[&s].clone())).unwrap();assert!(tr.complete);assert_eq!(tr.pages.len(),2);assert_eq!(tr.steps.len(),88);assert!(matches!(tr.steps[81].action,crate::onboard_macros::Action::ConsumerUp{usage:0xe2}));assert!(matches!(tr.steps[82].action,crate::onboard_macros::Action::Jump{..}));
  }
 }
 #[test]fn a_long_chain_never_consumes_factory_pages_or_silently_truncates_for_space(){
  let mut b=bank();for sector in [5,6,7]{b.pages.get_mut(&sector).unwrap()[0]=0x12;}b.inventory=inventory(&b.info,&b.pages).unwrap();let m=action(vec![MacroStep::Media(0xe2);43]);assert!(plan(&b,&m).is_err());assert_eq!(compile(&m,256).unwrap().len(),259);
 }

 #[test]fn sufficient_flash_space_cannot_bypass_the_validated_instruction_budget(){
  let mut b=bank();b.info.sector_count=32;for sector in 8..32{b.pages.insert(sector,vec![255;256]);}b.inventory=inventory(&b.info,&b.pages).unwrap();let m=action(vec![MacroStep::Media(0xe2);507]);assert!(compile(&m,256).is_ok());let error=plan(&b,&m).unwrap_err().to_string();assert!(error.contains("reader budget"));
 }

}

#[cfg(test)]
#[path="onboard_modifier_hold_tests.rs"]
mod modifier_hold_tests;
