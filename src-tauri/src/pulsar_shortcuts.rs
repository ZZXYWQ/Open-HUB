//! Nordic/Fusion short-key table, independently transcribed from the pinned
//! MIT pulsar-mouse-linux driver (a18ed8fb...), not ATK's native-macro format.
use gearwise_core::{model::{ButtonAction,Modifiers},Error,Result};
pub const SIZE:usize=32;
pub fn address(button:u16)->Result<u16>{if !(1..=5).contains(&button){return Err(Error::invalid("Pulsar shortcut button"));}Ok(0x100+32*(button-1))}
pub fn encode(usage:u8,mods:Modifiers)->Result<[u8;SIZE]>{
 if !(4..=0xa4).contains(&usage)||mods.0.count_ones()>4{return Err(Error::invalid("Pulsar shortcut: one keyboard usage and at most four modifiers (32 bytes)"));}
 let mut events=Vec::new();for bit in 0..8{let mask=1<<bit;if mods.0&mask!=0{events.push((0u8,mask));}}events.push((1,usage));
 let mut out=[0u8;SIZE];out[0]=(events.len()*2) as u8;let mut offset=1;
 for &(kind,value) in &events{out[offset..offset+3].copy_from_slice(&[0x80|kind,value,0]);offset+=3;}
 for &(kind,value) in events.iter().rev(){out[offset..offset+3].copy_from_slice(&[0x40|kind,value,0]);offset+=3;}
 out[offset]=0x55u8.wrapping_sub(out[..offset].iter().fold(0u8,|a,b|a.wrapping_add(*b)));Ok(out)
}
pub fn decode(raw:&[u8])->Option<ButtonAction>{
 if raw.len()!=SIZE||!(2..=10).contains(&raw[0])||raw[0]%2!=0{return None;}
 let end=1+3*usize::from(raw[0]);if end>=SIZE||raw[..=end].iter().fold(0u8,|a,b|a.wrapping_add(*b))!=0x55||raw[end+1..].iter().any(|v|*v!=0){return None;}
 let mut mods=0u8;let mut key=None;for event in raw[1..1+3*(usize::from(raw[0])/2)].chunks_exact(3){
  if event[2]!=0{return None;}match event[0]{0x80 if event[1].is_power_of_two()&&mods&event[1]==0&&key.is_none()=>mods|=event[1],0x81 if key.is_none()=>key=Some(event[1]),_=>return None,}
 }
 let usage=key?;let modifiers=Modifiers(mods);if encode(usage,modifiers).ok()?.as_slice()!=raw{return None;}Some(ButtonAction::Key{usage,mods:modifiers})
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn single_key_and_pointer_match_independent_wire_vectors(){let mut expected=[0;32];expected[..8].copy_from_slice(&[2,0x81,4,0,0x41,4,0,0x89]);assert_eq!(encode(4,Modifiers(0)).unwrap(),expected);assert_eq!(address(2).unwrap(),0x120);assert_eq!(address(4).unwrap(),0x160);assert_eq!(address(5).unwrap(),0x180);}
 #[test]fn every_supported_key_and_modifier_mask_roundtrips(){for mask in 0..=255u8{for usage in 4..=0xa4{let result=encode(usage,Modifiers(mask));if mask.count_ones()>4{assert!(result.is_err());}else{assert_eq!(decode(&result.unwrap()),Some(ButtonAction::Key{usage,mods:Modifiers(mask)}));}}}}
 #[test]fn malformed_checksum_release_order_tail_count_and_keys_remain_unknown(){let valid=encode(4,Modifiers(3)).unwrap();for at in [0,1,2,4,7,10,13,16,19,31]{let mut broken=valid;broken[at]^=1;assert!(decode(&broken).is_none());}assert!(encode(0,Modifiers(0)).is_err());assert!(encode(0xff,Modifiers(0)).is_err());assert!(address(0).is_err());assert!(address(6).is_err());}
}
