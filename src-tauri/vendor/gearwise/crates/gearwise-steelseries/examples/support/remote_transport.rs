//! Shared JSON-lines transport for isolated firmware examples; never opens HID.
use gearwise_core::{Error, Result, transport::HidTransport};
use serde_json::{Value, json};
use std::io::{self, Write};
pub fn receive() -> Result<Value> {
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|e| Error::protocol(e.to_string()))?;
    serde_json::from_str(&line).map_err(|e| Error::protocol(e.to_string()))
}
pub fn rpc(value: Value) -> Result<Value> {
    println!("{value}");
    io::stdout()
        .flush()
        .map_err(|e| Error::protocol(e.to_string()))?;
    let reply = receive()?;
    if let Some(error) = reply["error"].as_str() {
        return Err(Error::protocol(error));
    }
    Ok(reply)
}
pub struct Remote;
impl HidTransport for Remote {
    fn write(&mut self, data: &[u8]) -> Result<usize> {
        let r = rpc(json!({"op":"write","data":data}))?;
        r["n"]
            .as_u64()
            .map(|n| n as usize)
            .ok_or(Error::protocol("missing write length"))
    }
    fn read_timeout(&mut self, buf: &mut [u8], timeout_ms: i32) -> Result<usize> {
        let r = rpc(json!({"op":"read","capacity":buf.len(),"timeout":timeout_ms}))?;
        let data: Vec<u8> = serde_json::from_value(r["data"].clone())
            .map_err(|e| Error::protocol(e.to_string()))?;
        let n = data.len();
        let copied = n.min(buf.len());
        buf[..copied].copy_from_slice(&data[..copied]);
        Ok(n)
    }
    fn send_feature_report(&mut self, _: &[u8]) -> Result<()> {
        Err(Error::Unsupported(
            "unexpected feature write in output reader",
        ))
    }
    fn get_feature_report(&mut self, _: &mut [u8]) -> Result<usize> {
        Err(Error::Unsupported(
            "unexpected feature read in output reader",
        ))
    }
}
