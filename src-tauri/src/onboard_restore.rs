//! Directory/profile restoration with a reviewed multi-sector baseline.
//! Caller persists raw() before commit; no OS input or implicit retries.
use std::collections::{BTreeMap,BTreeSet};
use crate::{device::Device,hidpp::{Error,FeatureRequest,Result},onboard::{Info,RawSector,sector_crc_ok}};
fn fail(s:&str)->Error{Error::Other(format!("Logitech restore: {s}"))}
pub struct Prepared{info:Info,owner:String,connection:String,before:BTreeMap<u16,Vec<u8>>,desired:BTreeMap<u16,Vec<u8>>,order:Vec<u16>}
impl Prepared{
 pub fn matches(&self,sector:u16,bytes:&[u8])->bool{self.before.get(&sector).is_some_and(|b|b==bytes)}
 pub fn raw(&self)->Vec<RawSector>{self.before.iter().map(|(sector,bytes)|RawSector{sector:*sector,crc_valid:Some(sector_crc_ok(bytes)),hex:bytes.iter().map(|b|format!("{b:02x}")).collect()}).collect()}
}
fn capture(d:&Device,t:&dyn FeatureRequest,p:&Prepared)->Result<BTreeMap<u16,Vec<u8>>>{
 d.verify_native_preview(t,&p.owner,&p.connection)?;
 if d.onboard_info(t)?!=p.info{return Err(fail("memory descriptor changed since restore planning"));}
 let mut pages=BTreeMap::new();
 for sector in p.before.keys(){d.verify_native_preview(t,&p.owner,&p.connection)?;pages.insert(*sector,d.read_sector(t,*sector,p.info.sector_size)?);}
 d.verify_native_preview(t,&p.owner,&p.connection)?;Ok(pages)
}
fn verify(d:&Device,t:&dyn FeatureRequest,p:&Prepared,expected:&BTreeMap<u16,Vec<u8>>)->Result<()>{
 if &capture(d,t,p)?!=expected{return Err(fail("selected memory or directory differs from the reviewed restore state"));}Ok(())
}
pub fn prepare(d:&Device,t:&dyn FeatureRequest,info:&Info,sectors:&[(u16,Vec<u8>)])->Result<Prepared>{
 crate::onboard_bindings::validate_target(d).map_err(Error::Other)?;
 d.verify_native_preview(t,&d.profile_key,&d.connection_key)?;
 crate::onboard_bindings::validate_info(info).map_err(Error::Other)?;
 let mut seen=BTreeSet::new();
 if sectors.is_empty()||sectors.iter().any(|(sector,data)|*sector>=u16::from(info.sector_count)||!seen.insert(*sector)||data.len()!=usize::from(info.sector_size)||!sector_crc_ok(data)){return Err(fail("invalid, duplicate or unverified restore sector"));}
 // Guard the live directory even for a direct core caller restoring one page.
 seen.insert(0);
 let mut p=Prepared{info:info.clone(),owner:d.profile_key.clone(),connection:d.connection_key.clone(),before:seen.into_iter().map(|s|(s,Vec::new())).collect(),desired:BTreeMap::new(),order:sectors.iter().map(|(s,_)|*s).collect()};
 p.before=capture(d,t,&p)?;verify(d,t,&p,&p.before)?;
 p.desired=p.before.clone();for(sector,bytes)in sectors{p.desired.insert(*sector,bytes.clone());}
 p.order.sort_by_key(|sector|(*sector==0,*sector));Ok(p)
}
pub fn commit(d:&Device,t:&dyn FeatureRequest,p:&Prepared)->Result<usize>{
 verify(d,t,p,&p.before)?;
 let mut stage=p.before.clone();let mut count=0;
 let outcome=(||->Result<()>{for sector in &p.order{if stage[sector]==p.desired[sector]{continue;}
   verify(d,t,p,&stage)?;d.write_sector(t,*sector,&p.desired[sector])?;stage.insert(*sector,p.desired[sector].clone());verify(d,t,p,&stage)?;count+=1;
 }Ok(())})();
 if let Err(error)=outcome{
  let recovery=(||->Result<()>{let mut actual=capture(d,t,p)?;
   for(sector,bytes)in &actual{if bytes!=&p.before[sector]&&bytes!=&p.desired[sector]{return Err(fail("third-party or partial memory state; no blind restoration"));}}
   // Remove an updated directory reference before returning profile payloads.
   let mut order=p.order.clone();order.reverse();
   for sector in order{if actual[&sector]!=p.before[&sector]{verify(d,t,p,&actual)?;d.write_sector(t,sector,&p.before[&sector])?;actual.insert(sector,p.before[&sector].clone());verify(d,t,p,&actual)?;}}
   verify(d,t,p,&p.before)
  })();
  return Err(fail(&format!("multi-sector restore failed ({error}); {}",match recovery{Ok(())=>"original restore state restored and verified".into(),Err(e)=>format!("state uncertain; keep the backup and refresh without retry: {e}")})));
 }
 Ok(count)
}

/// Reload only the already active and enabled slot while already in on-board
/// mode. A failure here does not undo or misreport a verified flash commit.
pub fn reload_current(d:&Device,t:&dyn FeatureRequest,p:&Prepared)->Result<bool>{
 verify(d,t,p,&p.desired)?;
 let mode=d.call_unpadded(t,0x8100,2,&[])?;
 if mode.first()==Some(&2){return Ok(false);}if mode.first()!=Some(&1){return Err(fail("short or unknown native memory mode"));}
 let active=d.call_unpadded(t,0x8100,4,&[])?;
 if active.len()<2{return Err(fail("short active profile response"));}
 let slot=active[1];if slot==0||slot>p.info.profile_count{return Err(fail("active profile is not a known user slot"));}
 let at=usize::from(slot-1)*4;let directory=&p.desired[&0];let sector=u16::from_be_bytes([directory[at],directory[at+1]]);
 if sector==0xffff||directory[at+2]==0{return Err(fail("stored backup does not enable the current slot; select an enabled slot to use it"));}
 let changed=p.before[&0]!=p.desired[&0]||p.order.iter().any(|s|*s==sector&&p.before[s]!=p.desired[s]);
 if !changed{return Ok(false);}
 d.verify_native_preview(t,&p.owner,&p.connection)?;d.call_unpadded(t,0x8100,3,&[0,slot])?;d.verify_native_preview(t,&p.owner,&p.connection)?;Ok(true)
}
