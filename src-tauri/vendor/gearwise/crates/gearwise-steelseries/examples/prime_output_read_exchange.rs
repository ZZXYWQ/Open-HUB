//! Run the actual output reader over a JSON-lines emulated transport. No HID.
use gearwise_steelseries::{prime_feature_read::image_reads, prime_output_read::Reader};
use serde_json::json;
#[path = "support/remote_transport.rs"]
mod remote_transport;
use remote_transport::{Remote, receive};
fn main() {
    let input = receive().unwrap();
    let seed: [u8; 16] = serde_json::from_value(input["seed"].clone()).unwrap();
    let requests: Vec<usize> = serde_json::from_value(input["requests"].clone()).unwrap();
    let reads = image_reads();
    let mut reader = Reader::new(Remote, seed).unwrap();
    let mut results = Vec::new();
    for index in requests {
        match reader.read(&reads[index]) {
            Ok(reply)=>results.push(json!({"index":index,"bytes":reply.bytes(),"fences":reply.completed_fences(),"narrowed":reply.used_narrow_reads(),"identity_verified":reply.physical_identity_verified(),"atomic_verified":reply.atomic_image_verified()})),
            Err(error)=>{println!("{}",json!({"op":"done","error":error.to_string(),"results":results}));return;}
        }
    }
    println!("{}", json!({"op":"done","results":results}));
}
