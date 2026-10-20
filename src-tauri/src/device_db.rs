//! Built-in Logitech device names, keyed by USB / wireless product id.
//!
//! Firmware often reports an internal name (a PRO X SUPERLIGHT reports
//! "PRO X Wireless"), so devices are named from this table first and fall back
//! to the firmware name. Product ids are those exposed over USB (Cxxx/Axxx/Bxxx)
//! or by the device behind a Lightspeed/Unifying receiver (4xxx) via HID++
//! feature 0x0003.
//!
//! To add a device: append `(product_id, "NAME", "Kind")`, keeping ids sorted.

/// (product id, display name, kind), sorted by product id.
pub static DEVICES: &[(u16, &str, &str)] = &[
    (0x0A5B, "G933", "Headset"),
    (0x0A5C, "G633", "Headset"),
    (0x0A66, "G533", "Headset"),
    (0x0A6D, "G433", "Headset"),
    (0x0A78, "G560", "Speaker"),
    (0x0A87, "G935", "Headset"),
    (0x0A89, "G635", "Headset"),
    (0x0A9B, "G431", "Headset"),
    (0x0A9C, "G432", "Headset"),
    (0x0AA7, "PRO", "Headset"),
    (0x0AAA, "PRO X", "Headset"),
    (0x0AB5, "G733", "Headset"),
    (0x0ABA, "PRO X WIRELESS", "Headset"),
    (0x0AC4, "G535", "Headset"),
    (0x0AD8, "G735", "Headset"),
    (0x0AFE, "G733", "Headset"),
    (0x0B1F, "G733", "Headset"),
    (0x4053, "G900", "Mouse"),
    (0x405D, "G403", "Mouse"),
    (0x405F, "POWERPLAY", "Charge pad"),
    (0x4065, "G613", "Keyboard"),
    (0x4067, "G903", "Mouse"),
    (0x406C, "G603", "Mouse"),
    (0x4070, "G703", "Mouse"),
    (0x4074, "G305", "Mouse"),
    (0x4079, "PRO WIRELESS", "Mouse"),
    (0x407C, "G915", "Keyboard"),
    (0x407F, "G502 LIGHTSPEED", "Mouse"),
    (0x4085, "G604 LIGHTSPEED", "Mouse"),
    (0x4086, "G703 LIGHTSPEED", "Mouse"),
    (0x4087, "G903 LIGHTSPEED", "Mouse"),
    (0x408E, "G915 TKL", "Keyboard"),
    (0x4093, "PRO X SUPERLIGHT", "Mouse"),
    (0x409E, "G303 SHROUD EDITION", "Mouse"),
    (0xAAB4, "G402", "Mouse"),
    (0xAAB5, "G302", "Mouse"),
    (0xAAB6, "G303", "Mouse"),
    (0xAABA, "G900", "Mouse"),
    (0xAABC, "G410", "Keyboard"),
    (0xAABD, "G810", "Keyboard"),
    (0xAACC, "G810", "Keyboard"),
    (0xAAD8, "G903", "Mouse"),
    (0xAADF, "PRO WIRELESS", "Mouse"),
    (0xAAE3, "G915", "Keyboard"),
    (0xAAE6, "G502 HERO", "Mouse"),
    (0xAAE7, "PRO HERO", "Mouse"),
    (0xAAEC, "PRO RACING PEDALS", "Pedals"),
    (0xAAED, "RS H-SHIFTER", "Shifter"),
    (0xAAEF, "G502 LIGHTSPEED", "Mouse"),
    (0xAAF2, "G403 HERO", "Mouse"),
    (0xAAF6, "G703 LIGHTSPEED", "Mouse"),
    (0xAAF7, "G903 LIGHTSPEED", "Mouse"),
    (0xAAFB, "G915 TKL", "Keyboard"),
    (0xAAFF, "PRO", "Steering wheel"),
    (0xAB00, "PRO X SUPERLIGHT", "Mouse"),
    (0xAB11, "G303 SHROUD EDITION", "Mouse"),
    (0xAB2B, "RS50", "Steering wheel"),
    (0xAB2C, "RS PEDALS", "Pedals"),
    (0xAB2D, "RS SHIFTER & HANDBRAKE", "Shifter"),
    (0xB024, "G604 LIGHTSPEED", "Mouse"),
    (0xB354, "G915", "Keyboard"),
    (0xB35F, "G915 TKL", "Keyboard"),
    (0xC07D, "G502", "Mouse"),
    (0xC07E, "G402", "Mouse"),
    (0xC07F, "G302", "Mouse"),
    (0xC080, "G303", "Mouse"),
    (0xC081, "G900", "Mouse"),
    (0xC082, "G403", "Mouse"),
    (0xC083, "G403", "Mouse"),
    (0xC084, "G102", "Mouse"),
    (0xC085, "PRO", "Mouse"),
    (0xC086, "G903", "Mouse"),
    (0xC087, "G703", "Mouse"),
    (0xC088, "PRO WIRELESS", "Mouse"),
    (0xC08B, "G502 HERO", "Mouse"),
    (0xC08C, "PRO HERO", "Mouse"),
    (0xC08D, "G502 LIGHTSPEED", "Mouse"),
    (0xC08E, "MX518", "Mouse"),
    (0xC08F, "G403 HERO", "Mouse"),
    (0xC090, "G703 LIGHTSPEED", "Mouse"),
    (0xC091, "G903 LIGHTSPEED", "Mouse"),
    (0xC092, "G203 LIGHTSYNC", "Mouse"),
    (0xC094, "PRO X SUPERLIGHT", "Mouse"),
    (0xC097, "G303 SHROUD EDITION", "Mouse"),
    (0xC09D, "G203 LIGHTSYNC", "Mouse"),
    (0xC268, "PRO", "Steering wheel"),
    (0xC26A, "PRO RACING PEDALS", "Pedals"),
    (0xC26B, "RS H-SHIFTER", "Shifter"),
    (0xC272, "PRO", "Steering wheel"),
    (0xC276, "RS50", "Steering wheel"),
    (0xC277, "RS PEDALS", "Pedals"),
    (0xC278, "RS SHIFTER & HANDBRAKE", "Shifter"),
    (0xC32B, "G910", "Keyboard"),
    (0xC32D, "G310", "Keyboard"),
    (0xC330, "G410", "Keyboard"),
    (0xC331, "G810", "Keyboard"),
    (0xC332, "G502", "Mouse"),
    (0xC333, "G610", "Keyboard"),
    (0xC335, "G910", "Keyboard"),
    (0xC336, "G213", "Keyboard"),
    (0xC337, "G810", "Keyboard"),
    (0xC338, "G610", "Keyboard"),
    (0xC339, "PRO", "Keyboard"),
    (0xC33A, "G413", "Keyboard"),
    (0xC33C, "G513", "Keyboard"),
    (0xC33E, "G915", "Keyboard"),
    (0xC33F, "G815", "Keyboard"),
    (0xC342, "G512", "Keyboard"),
    (0xC343, "G915 TKL", "Keyboard"),
    (0xC537, "G602", "Mouse"),
];

/// Display name and kind for a product id.
pub fn lookup(product_id: u16) -> Option<(&'static str, &'static str)> {
    DEVICES
        .binary_search_by_key(&product_id, |(id, _, _)| *id)
        .ok()
        .map(|i| (DEVICES[i].1, DEVICES[i].2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_without_duplicates() {
        assert!(DEVICES.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn superlight_resolves_on_both_transports() {
        assert_eq!(lookup(0x4093).map(|d| d.0), Some("PRO X SUPERLIGHT"));
        assert_eq!(lookup(0xC094).map(|d| d.0), Some("PRO X SUPERLIGHT"));
        assert_eq!(lookup(0x0001), None);
    }
}
