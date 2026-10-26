//! Synapse parser adapted from danscMax/Sidearm (MIT, Dennis Max, 2026).
//! See resources/notices/Sidearm-MIT.txt. Does not execute or apply imported data.
pub mod format_v3;
pub mod format_v4;
pub mod macro_steps;
pub mod macro_xml;
pub mod makecode;
pub mod mapping;
pub mod types;
pub mod raw_source;
use std::{io::Read,path::Path};
pub fn preview(path:&Path)->Result<types::ParsedSynapseProfiles,String>{
    let mut file=std::fs::File::open(path).map_err(|e|e.to_string())?;
    let size=file.metadata().map_err(|e|e.to_string())?.len();
    if size>8*1024*1024{return Err("Synapse export exceeds 8 MiB".into());}
    let mut head=[0;2];let n=file.read(&mut head).map_err(|e|e.to_string())?;
    let mut parsed=if n==2&&head==*b"PK"{format_v3::parse_synapse_v3_file(path).map_err(|e|e.to_string())?}
      else{format_v4::parse_synapse_v4_file(path).map_err(|e|e.to_string())?};
    if parsed.profiles.is_empty(){return Err("No supported profiles found in Synapse export".into());}
    match raw_source::read(path, parsed.source_kind) {
        Ok(raw) => parsed.raw_source=Some(std::sync::Arc::new(raw)),
        Err(error) => parsed.raw_source_error=Some(error),
    }
    Ok(parsed)
}
pub use preview as parse_synapse_source;

#[cfg(test)]mod wire_tests{
 #[test]fn macro_delay_wire_field_matches_frontend(){let step=crate::types::ParsedSequenceStep::Sleep{delay_ms:100};let value=serde_json::to_value(step).unwrap();assert_eq!(value["kind"],"sleep");assert_eq!(value["delay_ms"],100);}
}
