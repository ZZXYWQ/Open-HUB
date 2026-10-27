//! Independently encoded ATK HUB 3.2.27 Fu record; no OEM code is run.
use super::*;
use gearwise_core::model::{DpiIndicatorMode, DpiIndicatorState, IndicatorLevel};
pub(super) fn decode(row: &[u8]) -> Result<DpiIndicatorState> {
    if row.len() != 8 {
        return Err(Error::invalid("COMPX indicator row length"));
    }
    let enabled = match checked_pair(&row[6..8])? {
        0 => false,
        1 => true,
        _ => return Err(Error::invalid("Unknown COMPX indicator enabled flag")),
    };
    // Fu.create(Off) leaves BOTH zero-initialized mode bytes untouched.
    let mode = if !enabled && row[..2] == [0, 0] {
        DpiIndicatorMode::Unspecified
    } else {
        match checked_pair(&row[..2])? {
            1 => DpiIndicatorMode::Static,
            2 => DpiIndicatorMode::Breathing,
            _ => return Err(Error::invalid("Unknown COMPX indicator mode")),
        }
    };
    let brightness = match checked_pair(&row[2..4])? {
        16 => IndicatorLevel::Low,
        128 => IndicatorLevel::Medium,
        255 => IndicatorLevel::High,
        _ => return Err(Error::invalid("Unknown COMPX indicator brightness")),
    };
    let speed = match checked_pair(&row[4..6])? {
        1 => IndicatorLevel::Low,
        3 => IndicatorLevel::Medium,
        5 => IndicatorLevel::High,
        _ => return Err(Error::invalid("Unknown COMPX indicator speed")),
    };
    Ok(DpiIndicatorState {
        enabled,
        mode,
        brightness,
        speed,
    })
}
pub(super) fn encode(value: &DpiIndicatorState) -> [u8; 8] {
    let mode = match value.mode {
        DpiIndicatorMode::Unspecified => 0,
        DpiIndicatorMode::Static => 1,
        DpiIndicatorMode::Breathing => 2,
    };
    let brightness = match value.brightness {
        IndicatorLevel::Low => 16,
        IndicatorLevel::Medium => 128,
        IndicatorLevel::High => 255,
    };
    let speed = match value.speed {
        IndicatorLevel::Low => 1,
        IndicatorLevel::Medium => 3,
        IndicatorLevel::High => 5,
    };
    let mut row = [0; 8];
    for (i, v) in [mode, brightness, speed, u8::from(value.enabled)]
        .into_iter()
        .enumerate()
    {
        row[i * 2..i * 2 + 2].copy_from_slice(&pair(v));
    }
    if value.mode == DpiIndicatorMode::Unspecified {
        row[..2].fill(0);
    }
    row
}
impl Atk {
    pub(super) fn apply_indicator(&mut self, value: &DpiIndicatorState) -> Result<()> {
        validate_indicator_state(self.model.vendor, self.model.product, value)?;
        self.identify()?;
        decode(&self.read(76, 8)?)?;
        // write_verified compares the complete eight-byte row before accepting it.
        self.write_verified(76, &encode(value))
    }
}
