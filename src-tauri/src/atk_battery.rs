//! ATK HUB WS: level/charge/u16 voltage. Voltage units are not claimed.
use gearwise_core::{model::Battery, Error, Result};
pub(super) fn decode(raw: &[u8]) -> Result<Battery> {
    if raw.len() != 4 || raw[0] > 100 || raw[1] > 1 {
        return Err(Error::invalid(
            "COMPX battery response has unknown length, level or charging state",
        ));
    }
    Ok(Battery {
        percent: Some(raw[0]),
        charging: raw[1] == 1,
    })
}
#[cfg(test)]
#[path = "atk_battery_tests.rs"]
mod tests;
