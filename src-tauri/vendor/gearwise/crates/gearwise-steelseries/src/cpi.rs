//! Sensor resolution codes: how a DPI value becomes the register value the
//! firmware expects.
//!
//! SteelSeries firmware takes a sensor-specific code, not a DPI figure, and
//! the codes are not a simple function of the DPI for the PixArt TrueMove
//! sensors. The tables below list the code for every step of the sensor's
//! range; the values come from rivalcfg's per-sensor tables.

use gearwise_core::{Error, Result};

/// How DPI values map to wire codes.
#[derive(Clone, Copy, Debug)]
pub enum Map {
    /// `code = table[(dpi - min) / step]`.
    Table(&'static [u8]),
    /// `code = dpi / divisor + offset`.
    Linear { divisor: u16, offset: i16 },
    /// A fixed set of supported values, each with its code.
    List(&'static [(u16, u8)]),
}

/// A sensor's selectable resolutions.
#[derive(Clone, Copy, Debug)]
pub struct Cpi {
    pub min: u16,
    pub max: u16,
    /// Grid spacing; for a [`Map::List`] the greatest common step.
    pub step: u16,
    pub map: Map,
}

impl Cpi {
    /// The supported DPI nearest to `dpi` and its wire code.
    ///
    /// Values outside `min..=max` are refused rather than clamped; values
    /// between steps snap to the closest one (the lower one on a tie).
    pub fn resolve(&self, dpi: u16) -> Result<(u16, u16)> {
        if dpi < self.min || dpi > self.max {
            return Err(Error::invalid(format!(
                "{dpi} DPI is outside {}-{}",
                self.min, self.max
            )));
        }
        if let Map::List(list) = self.map {
            // Lists are sorted ascending and never empty.
            let (snapped, code) = list
                .iter()
                .copied()
                .min_by_key(|(d, _)| d.abs_diff(dpi))
                .expect("DPI list is never empty");
            return Ok((snapped, u16::from(code)));
        }
        let steps = (dpi - self.min + self.step / 2) / self.step;
        let snapped = (self.min + steps * self.step).min(self.max);
        let code = match self.map {
            Map::Table(table) => u16::from(table[usize::from((snapped - self.min) / self.step)]),
            Map::Linear { divisor, offset } => {
                u16::try_from(i32::from(snapped / divisor) + i32::from(offset))
                    .expect("linear DPI code is positive")
            }
            Map::List(_) => unreachable!("handled above"),
        };
        Ok((snapped, code))
    }
}

/// Codes for 200-8500 DPI in steps of 100 (PixArt TrueMove Core class).
const CORE_CODES: [u8; 84] = [
    0x04, 0x06, 0x08, 0x0b, 0x0d, 0x0f, 0x12, 0x14, 0x16, 0x19, 0x1b, 0x1d, 0x20, 0x22, 0x24, 0x27,
    0x29, 0x2b, 0x2e, 0x30, 0x32, 0x34, 0x37, 0x39, 0x3b, 0x3e, 0x40, 0x42, 0x45, 0x47, 0x49, 0x4c,
    0x4e, 0x50, 0x53, 0x55, 0x57, 0x5a, 0x5c, 0x5e, 0x61, 0x63, 0x65, 0x68, 0x6a, 0x6c, 0x6f, 0x71,
    0x73, 0x76, 0x78, 0x7a, 0x7d, 0x7f, 0x81, 0x84, 0x86, 0x88, 0x8b, 0x8d, 0x8f, 0x92, 0x94, 0x96,
    0x99, 0x9b, 0x9d, 0xa0, 0xa2, 0xa4, 0xa7, 0xa9, 0xab, 0xad, 0xb0, 0xb2, 0xb4, 0xb7, 0xb9, 0xbc,
    0xbe, 0xc0, 0xc3, 0xc5,
];
/// Codes for 100-18000 DPI in steps of 100 (TrueMove Air class).
const AIR_CODES: [u8; 180] = [
    0x00, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x10, 0x11, 0x12,
    0x13, 0x14, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1d, 0x1e, 0x1f, 0x20, 0x21, 0x23, 0x25, 0x26,
    0x27, 0x28, 0x29, 0x2a, 0x2c, 0x2d, 0x2e, 0x2f, 0x30, 0x32, 0x33, 0x34, 0x35, 0x36, 0x38, 0x39,
    0x3a, 0x3b, 0x3c, 0x3e, 0x3f, 0x40, 0x41, 0x42, 0x44, 0x45, 0x46, 0x47, 0x48, 0x4a, 0x4b, 0x4c,
    0x4d, 0x4e, 0x50, 0x51, 0x52, 0x53, 0x54, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x5c, 0x5d, 0x5e, 0x5f,
    0x60, 0x62, 0x63, 0x64, 0x65, 0x66, 0x68, 0x69, 0x6a, 0x6b, 0x6c, 0x6e, 0x6f, 0x70, 0x71, 0x72,
    0x74, 0x75, 0x76, 0x77, 0x78, 0x7a, 0x7b, 0x7c, 0x7d, 0x7e, 0x80, 0x81, 0x82, 0x83, 0x84, 0x86,
    0x87, 0x88, 0x89, 0x8a, 0x8c, 0x8d, 0x8e, 0x8f, 0x90, 0x92, 0x93, 0x94, 0x95, 0x96, 0x98, 0x99,
    0x9a, 0x9b, 0x9c, 0x9e, 0x9f, 0xa0, 0xa1, 0xa2, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xaa, 0xab, 0xac,
    0xad, 0xae, 0xb0, 0xb1, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xbb, 0xbc, 0xbd,
    0xbf, 0xc0, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcc, 0xcd, 0xcf, 0xd0, 0xd1,
    0xd2, 0xd3, 0xd5, 0xd6,
];

/// Rival 3, Aerox 3 (wired).
pub const CORE: Cpi = Cpi {
    min: 200,
    max: 8500,
    step: 100,
    map: Map::Table(&CORE_CODES),
};

/// Rival 110/106/300S: the PMW3326 codes are the first 71 core codes.
pub const PMW3326: Cpi = Cpi {
    min: 200,
    max: 7200,
    step: 100,
    map: Map::Table(&CORE_CODES),
};

/// Aerox 5, Aerox 3/5/9 Wireless, Prime Wireless, Rival 3 Wireless.
pub const AIR: Cpi = Cpi {
    min: 100,
    max: 18000,
    step: 100,
    map: Map::Table(&AIR_CODES),
};

/// Rival 300 family: `dpi / 50`.
pub const LINEAR_6500: Cpi = Cpi {
    min: 50,
    max: 6500,
    step: 50,
    map: Map::Linear {
        divisor: 50,
        offset: 0,
    },
};

/// Rival 310, Sensei 310, Rival 600: `dpi / 100 - 1`.
pub const LINEAR_12000: Cpi = Cpi {
    min: 100,
    max: 12000,
    step: 100,
    map: Map::Linear {
        divisor: 100,
        offset: -1,
    },
};

/// Sensei Ten, Prime, Prime+, Prime Mini: `dpi / 50`, sent as 16 bits.
pub const LINEAR_18000: Cpi = Cpi {
    min: 50,
    max: 18000,
    step: 50,
    map: Map::Linear {
        divisor: 50,
        offset: 0,
    },
};

/// Rival 100 and 105: eight fixed settings, coded from 8 (lowest) down to 1.
pub const RIVAL_100: Cpi = Cpi {
    min: 250,
    max: 4000,
    step: 250,
    map: Map::List(&[
        (250, 8),
        (500, 7),
        (1000, 6),
        (1250, 5),
        (1500, 4),
        (1750, 3),
        (2000, 2),
        (4000, 1),
    ]),
};

/// Kana v2: eight fixed settings, coded from 8 (lowest) down to 1.
pub const KANA_V2: Cpi = Cpi {
    min: 400,
    max: 4000,
    step: 400,
    map: Map::List(&[
        (400, 8),
        (800, 7),
        (1200, 6),
        (1600, 5),
        (2000, 4),
        (2400, 3),
        (3200, 2),
        (4000, 1),
    ]),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_cover_their_ranges() {
        assert_eq!(CORE_CODES.len(), (8500 - 200) / 100 + 1);
        assert_eq!(AIR_CODES.len(), (18000 - 100) / 100 + 1);
        assert!(CORE_CODES.windows(2).all(|w| w[0] < w[1]));
        assert!(AIR_CODES.windows(2).all(|w| w[0] < w[1]));
        // The PMW3326 range ends inside the core table.
        assert_eq!(usize::from((PMW3326.max - PMW3326.min) / 100 + 1), 71);
    }

    #[test]
    fn resolves_known_codes() {
        assert_eq!(CORE.resolve(800).unwrap(), (800, 0x12));
        assert_eq!(CORE.resolve(8500).unwrap(), (8500, 0xc5));
        assert_eq!(AIR.resolve(100).unwrap(), (100, 0x00));
        assert_eq!(AIR.resolve(800).unwrap(), (800, 0x09));
        assert_eq!(AIR.resolve(18000).unwrap(), (18000, 0xd6));
        assert_eq!(LINEAR_6500.resolve(6500).unwrap(), (6500, 130));
        assert_eq!(LINEAR_12000.resolve(800).unwrap(), (800, 7));
        assert_eq!(LINEAR_12000.resolve(12000).unwrap(), (12000, 119));
        assert_eq!(LINEAR_18000.resolve(18000).unwrap(), (18000, 360));
    }

    #[test]
    fn snaps_between_steps() {
        assert_eq!(CORE.resolve(849).unwrap().0, 800);
        assert_eq!(CORE.resolve(850).unwrap().0, 900);
        assert_eq!(LINEAR_18000.resolve(1274).unwrap().0, 1250);
    }

    #[test]
    fn lists_snap_to_the_nearest_entry() {
        assert_eq!(RIVAL_100.resolve(1250).unwrap(), (1250, 5));
        assert_eq!(RIVAL_100.resolve(4000).unwrap(), (4000, 1));
        assert_eq!(RIVAL_100.resolve(3000).unwrap(), (2000, 2), "tie goes down");
        assert_eq!(RIVAL_100.resolve(3500).unwrap(), (4000, 1));
        assert_eq!(KANA_V2.resolve(1000).unwrap(), (800, 7));
    }

    #[test]
    fn refuses_values_outside_the_range() {
        assert!(matches!(CORE.resolve(100), Err(Error::InvalidValue(_))));
        assert!(matches!(CORE.resolve(8501), Err(Error::InvalidValue(_))));
        assert!(matches!(
            RIVAL_100.resolve(200),
            Err(Error::InvalidValue(_))
        ));
    }
}
