//! Rival600 native proximity levels. Factual wire layout from GG122 descriptor,
//! level/threshold table from inert GG122 renderer factory 83485. No mm claim.
use crate::link::Packet;
use gearwise_core::{Error, Result};
pub const HIGH: [u8; 8] = [120, 115, 110, 105, 100, 95, 90, 85];
pub fn packet(index: u8) -> Result<Packet> {
    let high = *HIGH
        .get(usize::from(index))
        .ok_or_else(|| Error::invalid("Rival600 proximity level must be 0–7"))?;
    Ok(Packet::output(vec![0x96, 0, 1, high - 4, high]))
}
pub fn labels() -> Vec<String> {
    (1..=8)
        .map(|v| match v {
            1 => "1 / 8（低）".into(),
            8 => "8 / 8（高）".into(),
            _ => format!("{v} / 8"),
        })
        .collect()
}
