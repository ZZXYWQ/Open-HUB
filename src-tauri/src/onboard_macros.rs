//! Read-only native Macro Format 1 inspection. Bytecode remains a per-mouse
//! device program; it is never played through OS input or silently flattened.
//! JUMP has a four-byte address operand (sector BE16, offset BE16). Older
//! three-byte-entry decoders are not used for cross-page traversal.
use crate::{device::Device,hidpp::{Error,FeatureRequest,Result},onboard::{sector_crc_ok,Info},onboard_bindings::{Layer,validate_info}};
use serde::{Serialize,Deserialize};
use std::collections::{BTreeMap,BTreeSet};
const MAX_PAGES:usize=16;
const MAX_STEPS:usize=1024;
#[derive(Clone,Copy,Debug,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Address{pub sector:u16,pub offset:u16}
#[derive(Debug,Serialize,PartialEq,Eq)]
pub enum Action{
 NoOp,WaitForRelease,RepeatWhilePressed,RepeatUntilCanceled,
 Delay{milliseconds:u16},KeyDown{modifiers:u8,usage:u8},KeyUp{modifiers:u8,usage:u8},
 MouseDown{buttons:u16},MouseUp{buttons:u16},ConsumerDown{usage:u16},ConsumerUp{usage:u16},
 Jump{target:Address},End,
}
#[derive(Debug,Serialize)]pub struct Step{pub address:Address,pub bytes:Vec<u8>,pub action:Action}
#[derive(Debug,Serialize)]pub struct Page{pub sector:u16,pub crc_valid:bool,pub hex:String}
#[derive(Debug,Serialize)]pub struct Trace{pub entry:Address,pub complete:bool,pub note:Option<String>,pub steps:Vec<Step>,pub pages:Vec<Page>}
#[derive(Clone,Debug,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection{pub expected_device:String,pub slot:u8,pub sector:u16,pub layer:Layer,pub button:u8,pub expected:[u8;4]}
fn fail(message:&str)->Error{Error::Other(format!("Logitech native macro: {message}"))}
fn allowed(a:Address,info:&Info)->bool{a.sector>0&&a.sector<u16::from(info.sector_count)&&a.offset<info.sector_size}
fn be(raw:&[u8],at:usize)->u16{u16::from_be_bytes([raw[at],raw[at+1]])}
fn inspect(entry:Address,info:&Info,mut load:impl FnMut(u16)->Result<Vec<u8>>)->Result<Trace>{
 let mut trace=Trace{entry,complete:false,note:None,steps:vec![],pages:vec![]};
 let mut pages=BTreeMap::<u16,Vec<u8>>::new();let mut visited=BTreeSet::new();let mut pc=entry;
 loop{
  let reason=if !allowed(pc,info){Some("address outside the reported live memory; no page was guessed")}
   else if trace.steps.len()>=MAX_STEPS{Some("instruction limit reached")}
   else if !visited.insert(pc){Some("cyclic jump found; this viewer does not execute a loop")}
   else{None};
  if let Some(reason)=reason{trace.note=Some(reason.into());break;}
  if !pages.contains_key(&pc.sector){
   if pages.len()>=MAX_PAGES{trace.note=Some("page limit reached".into());break;}
   let bytes=load(pc.sector)?;if bytes.len()!=usize::from(info.sector_size){return Err(fail("page length differs from descriptor"));}
   trace.pages.push(Page{sector:pc.sector,crc_valid:sector_crc_ok(&bytes),hex:bytes.iter().map(|b|format!("{b:02x}")).collect()});pages.insert(pc.sector,bytes);
  }
  let page=&pages[&pc.sector];let at=usize::from(pc.offset);let opcode=page[at];
  let size=match opcode{0|1|2|3|255=>1,0x40..=0x46=>3,0x60=>5,_=>{trace.note=Some(format!("unverified opcode 0x{opcode:02x} at {}:{}; raw page retained",pc.sector,pc.offset));break;}};
  if at+size>page.len(){trace.note=Some("instruction crosses the physical page boundary; no implicit fallthrough".into());break;}
  let bytes=page[at..at+size].to_vec();
  let action=match opcode{
   0=>Action::NoOp,1=>Action::WaitForRelease,2=>Action::RepeatWhilePressed,3=>Action::RepeatUntilCanceled,
   0x40=>Action::Delay{milliseconds:be(&bytes,1)},
   0x41=>Action::MouseDown{buttons:be(&bytes,1)},0x42=>Action::MouseUp{buttons:be(&bytes,1)},
   0x43=>Action::KeyDown{modifiers:bytes[1],usage:bytes[2]},0x44=>Action::KeyUp{modifiers:bytes[1],usage:bytes[2]},
   0x45=>Action::ConsumerDown{usage:be(&bytes,1)},0x46=>Action::ConsumerUp{usage:be(&bytes,1)},
   0x60=>Action::Jump{target:Address{sector:be(&bytes,1),offset:be(&bytes,3)}},255=>Action::End,_=>unreachable!(),
  };
  let next=match &action{Action::End=>None,Action::Jump{target}=>Some(*target),_=>Some(Address{sector:pc.sector,offset:pc.offset+size as u16})};
  trace.steps.push(Step{address:pc,bytes,action});
  match next{None=>{trace.complete=true;break;},Some(next)=>pc=next}
 }
 Ok(trace)
}
/// Uses only getInfo/readSector/unit-id. No profile activation, mode switch,
/// macro allocation, flash write, input hook or input injection is reachable.
pub fn read(d:&Device,t:&dyn FeatureRequest,s:&Selection)->Result<Trace>{
 if d.vendor_id!=0x046d||!d.kind.eq_ignore_ascii_case("mouse"){return Err(fail("requires a Logitech mouse"));}
 if d.index!=crate::hidpp::DIRECT&&!d.profile_key.starts_with("unit:"){return Err(fail("receiver inspection requires a verified physical unit"));}
 d.verify_native_identity(t,&s.expected_device)?;let info=d.onboard_info(t)?;validate_info(&info).map_err(Error::Other)?;
 if s.slot==0||s.slot>info.profile_count||s.button==0||s.button>info.button_count||s.sector==0||s.sector>=u16::from(info.sector_count){return Err(fail("unknown slot, button or sector"));}
 if s.layer==Layer::GShift&&info.mechanical_layout&3!=2{return Err(fail("G-Shift is not advertised"));}
 let directory=d.read_sector(t,0,info.sector_size)?;let offset=usize::from(s.slot-1)*4;
 if !sector_crc_ok(&directory)||be(&directory,offset)!=s.sector{return Err(fail("profile directory changed; read it again"));}
 let profile=d.read_sector(t,s.sector,info.sector_size)?;let at=(if s.layer==Layer::Normal{0x20}else{0x60})+usize::from(s.button-1)*4;
 if !sector_crc_ok(&profile)||profile[at..at+4]!=s.expected{return Err(fail("profile binding changed; read it again"));}
 if s.expected[0]!=0{return Err(fail("selected record is not a supported native macro address"));}
 let entry=Address{sector:u16::from(s.expected[1]),offset:be(&s.expected,2)};
 let trace=inspect(entry,&info,|sector|{d.verify_native_identity(t,&s.expected_device)?;let bytes=d.read_sector(t,sector,info.sector_size)?;d.verify_native_identity(t,&s.expected_device)?;Ok(bytes)})?;
 if d.read_sector(t,0,info.sector_size)?!=directory||d.read_sector(t,s.sector,info.sector_size)?!=profile{return Err(fail("on-board profile changed during inspection; old result discarded"));}
 d.verify_native_identity(t,&s.expected_device)?;Ok(trace)
}
#[cfg(test)]mod tests{
 use super::*;
 fn info()->Info{Info{memory_model:1,profile_format:3,macro_format:1,profile_count:2,rom_profile_count:0,button_count:11,sector_count:16,sector_size:256,mechanical_layout:10,various_info:1,bindings_writable:true}}
 fn page(code:&[u8])->Vec<u8>{let mut p=vec![0xff;256];p[..code.len()].copy_from_slice(code);p}
 #[test]fn known_bytecode_and_five_byte_jump_have_independent_golden_steps(){
  let pages:BTreeMap<_,_>=[(8,page(&[0x43,0x11,4,0x40,0x01,0xf4,0x44,0x11,4,0x60,0,9,0,3])),(9,page(&[0xff,0xff,0xff,0x45,0,0xe2,0x46,0,0xe2,0xff]))].into();
  let tr=inspect(Address{sector:8,offset:0},&info(),|p|Ok(pages[&p].clone())).unwrap();assert!(tr.complete);assert_eq!(tr.pages.len(),2);assert_eq!(tr.steps[1].action,Action::Delay{milliseconds:500});assert_eq!(tr.steps[3].bytes,vec![0x60,0,9,0,3]);assert_eq!(tr.steps[4].address,Address{sector:9,offset:3});assert_eq!(tr.steps[4].action,Action::ConsumerDown{usage:0xe2});assert!(!tr.pages[0].crc_valid);
 }
 #[test]fn a_shared_page_is_read_once_and_full_sixteen_bit_offsets_are_preserved(){
  let mut i=info();i.sector_size=512;let mut bytes=vec![0xff;512];bytes[..5].copy_from_slice(&[0x60,0,8,1,0x20]);bytes[0x120..0x124].copy_from_slice(&[0x43,0,6,0xff]);let mut count=0;
  let tr=inspect(Address{sector:8,offset:0},&i,|_|{count+=1;Ok(bytes.clone())}).unwrap();assert!(tr.complete);assert_eq!(count,1);assert_eq!(tr.steps[1].address.offset,0x120);
 }
 #[test]fn cycles_invalid_jumps_and_unsupported_wheel_never_guess_or_execute(){
  for code in [&[0x60,0,8,0,0][..],&[0x60,1,0,0,0][..],&[0x20,1,0,0xff][..],&[0x61,0,1,0,2][..]]{let mut reads=0;let tr=inspect(Address{sector:8,offset:0},&info(),|_|{reads+=1;Ok(page(code))}).unwrap();assert!(!tr.complete);assert!(tr.note.is_some());assert_eq!(reads,1);}
 }
 #[test]fn missing_end_and_partial_records_stop_at_boundary_without_next_page_fallthrough(){
  for start in [0,254]{let tr=inspect(Address{sector:8,offset:start},&info(),|_|Ok(vec![0x43;256])).unwrap();assert!(!tr.complete);assert_eq!(tr.pages.len(),1);assert!(tr.note.unwrap().contains("boundary")||start==0);}
 }
 #[test]fn memory_limits_and_wrong_page_lengths_fail_without_unbounded_reads(){
  let mut reads=0;let tr=inspect(Address{sector:16,offset:0},&info(),|_|{reads+=1;Ok(vec![])}).unwrap();assert!(!tr.complete);assert_eq!(reads,0);
  assert!(inspect(Address{sector:8,offset:0},&info(),|_|Ok(vec![0;3])).is_err());
  let tr=inspect(Address{sector:8,offset:0},&info(),|_|Ok(page(&[1,2,3,0x41,0,16,0x42,0,16,0xff]))).unwrap();assert!(tr.complete);assert_eq!(tr.steps[3].action,Action::MouseDown{buttons:16});
 }
 #[test]fn page_and_instruction_budgets_bound_malicious_chains(){
  let mut i=info();i.sector_count=255;let mut pages=0;
  let tr=inspect(Address{sector:8,offset:0},&i,|sector|{pages+=1;Ok(page(&[0x60,0,(sector+1)as u8,0,0]))}).unwrap();assert!(!tr.complete);assert_eq!(pages,16);assert!(tr.note.unwrap().contains("page limit"));
  i.sector_size=4096;let tr=inspect(Address{sector:8,offset:0},&i,|_|Ok(vec![0;4096])).unwrap();assert!(!tr.complete);assert_eq!(tr.steps.len(),1024);assert!(tr.note.unwrap().contains("instruction limit"));
 }

}
