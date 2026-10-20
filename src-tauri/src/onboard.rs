//! Read-only access to on-board profile memory (HID++ 0x8100).
//!
//! Layout (matches G HUB's onboard_profiles model and libratbag's HID++
//! implementation): memory is split into sectors. Sector 0x0000 is the
//! profile directory (4-byte entries: big-endian sector address, enabled
//! flag, reserved; 0xFFFF ends the list). User profiles live in sectors
//! 0x0001.., factory (ROM) profiles in 0x0101... Every sector ends with a
//! big-endian CRC-CCITT (seed 0xFFFF) of the bytes before it.
//!
//! Nothing here writes to the device.

use serde::Serialize;

use crate::device::{feature::ONBOARD_PROFILES, Device};
use crate::hidpp::{Result, Transport};

const DIRECTORY_SECTOR: u16 = 0x0000;
const ROM_BASE: u16 = 0x0100;
const DIRECTORY_END: u16 = 0xFFFF;

/// Function numbers of feature 0x8100.
mod func {
    pub const GET_INFO: u8 = 0;
    pub const GET_ACTIVE_PROFILE: u8 = 4;
    pub const READ: u8 = 5;
    pub const GET_ACTIVE_DPI_INDEX: u8 = 0xB;
}

pub fn crc_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        let temp = (crc >> 8) ^ u16::from(byte);
        crc <<= 8;
        let mut quick = temp ^ (temp >> 4);
        crc ^= quick;
        quick <<= 5;
        crc ^= quick;
        quick <<= 7;
        crc ^= quick;
    }
    crc
}

fn sector_crc_ok(data: &[u8]) -> bool {
    data.len() >= 2 && crc_ccitt(&data[..data.len() - 2]) == u16::from_be_bytes([data[data.len() - 2], data[data.len() - 1]])
}

#[derive(Clone, Debug, Serialize)]
pub struct Info {
    pub memory_model: u8,
    pub profile_format: u8,
    pub macro_format: u8,
    pub profile_count: u8,
    pub rom_profile_count: u8,
    pub button_count: u8,
    pub sector_count: u8,
    pub sector_size: u16,
    pub mechanical_layout: u8,
    pub various_info: u8,
}

/// One 4-byte button binding, decoded for display.
#[derive(Clone, Debug, Serialize)]
pub struct ButtonBinding {
    pub raw: [u8; 4],
    pub kind: &'static str,
    pub description: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Profile {
    /// 1-based user slot, or ROM slot when `rom` is true.
    pub slot: u8,
    pub sector: u16,
    pub rom: bool,
    pub enabled: bool,
    /// None for ROM profiles, which carry no CRC.
    pub crc_valid: Option<bool>,
    pub name: Option<String>,
    pub report_rate_hz: u32,
    /// Index into `dpi_levels` the profile starts on.
    pub default_dpi_index: u8,
    /// DPI-shift (sniper) index.
    pub shift_dpi_index: u8,
    pub dpi_levels: Vec<u16>,
    pub angle_snapping: u8,
    pub powersave_timeout: u16,
    pub poweroff_timeout: u16,
    pub buttons: Vec<ButtonBinding>,
    pub gshift_buttons: Vec<ButtonBinding>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub info: Info,
    pub directory_crc_valid: bool,
    /// Active on-board profile slot (1-based); 0 when none is active,
    /// e.g. while the mouse is in software (host) mode.
    pub active_profile: u8,
    pub active_dpi_index: Option<u8>,
    pub profiles: Vec<Profile>,
    pub rom_profiles: Vec<Profile>,
}

/// Raw sectors, for a byte-exact backup file.
#[derive(Clone, Debug, Serialize)]
pub struct RawSector {
    pub sector: u16,
    /// None for ROM sectors, which carry no CRC.
    pub crc_valid: Option<bool>,
    /// Hex-encoded sector bytes.
    pub hex: String,
}

fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}

const SPECIALS: [&str; 12] = [
    "invalid",
    "Wheel left",
    "Wheel right",
    "DPI up",
    "DPI down",
    "DPI cycle",
    "DPI default",
    "DPI shift",
    "Profile up",
    "Profile down",
    "Profile cycle",
    "G-Shift",
];

pub fn decode_binding(raw: [u8; 4]) -> ButtonBinding {
    let (kind, description) = match raw[0] {
        0x80 => match raw[1] {
            0x00 => ("none", "No action".to_string()),
            0x01 => {
                let mask = u16::from_be_bytes([raw[2], raw[3]]);
                let button = if mask == 0 { 0 } else { mask.trailing_zeros() + 1 };
                ("mouse", format!("Mouse button {button}"))
            }
            0x02 => ("keyboard", format!("Key 0x{:02x} (modifiers 0x{:02x})", raw[3], raw[2])),
            0x03 => ("consumer", format!("Media key 0x{:04x}", u16::from_be_bytes([raw[2], raw[3]]))),
            other => ("unknown", format!("HID subtype 0x{other:02x}")),
        },
        0x90 => {
            let name = SPECIALS.get(usize::from(raw[1])).copied().unwrap_or("invalid");
            ("special", name.to_string())
        }
        0x00 => ("macro", format!("Macro at page {} offset {}", raw[1], raw[3])),
        0xFF => ("disabled", "Disabled".to_string()),
        other => ("unknown", format!("Type 0x{other:02x}")),
    };
    ButtonBinding { raw, kind, description }
}

/// Profile name: all 0xFF means "use the default name". Names written as
/// UTF-16LE (every other byte zero) and as plain UTF-8 are both seen.
fn decode_name(raw: &[u8]) -> Option<String> {
    if raw.iter().all(|&b| b == 0xFF) {
        return None;
    }
    let utf16 = raw.chunks(2).take_while(|c| c != &[0, 0]).all(|c| c[1] == 0);
    let text = if utf16 {
        let units: Vec<u16> = raw
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&u| u != 0 && u != 0xFFFF)
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        let end = raw.iter().position(|&b| b == 0 || b == 0xFF).unwrap_or(raw.len());
        String::from_utf8_lossy(&raw[..end]).into_owned()
    };
    Some(text.trim().to_string()).filter(|s| !s.is_empty())
}

fn parse_profile(slot: u8, sector: u16, rom: bool, enabled: bool, data: &[u8], button_count: usize) -> Profile {
    let le16 = |i: usize| u16::from_le_bytes([data[i], data[i + 1]]);
    let binding = |i: usize| decode_binding([data[i], data[i + 1], data[i + 2], data[i + 3]]);
    // Packed layout: rate, default/shift DPI index, 5 x DPI (LE), RGB color,
    // power mode, angle snapping, 10 reserved, powersave/poweroff timeouts,
    // then 16 buttons, 16 G-Shift buttons, a 48-byte name, LEDs, CRC.
    let buttons_at = 0x20;
    let gshift_at = buttons_at + 16 * 4;
    let name_at = gshift_at + 16 * 4;
    let name = decode_name(&data[name_at..name_at + 48]);
    let count = button_count.min(16);
    Profile {
        slot,
        sector,
        rom,
        enabled,
        crc_valid: (!rom).then(|| sector_crc_ok(data)),
        name,
        report_rate_hz: 1000 / u32::from(data[0].max(1)),
        default_dpi_index: data[1],
        shift_dpi_index: data[2],
        dpi_levels: (0..5).map(|i| le16(3 + 2 * i)).filter(|&d| d != 0 && d != 0xFFFF).collect(),
        angle_snapping: data[0x11],
        powersave_timeout: le16(0x1C),
        poweroff_timeout: le16(0x1E),
        buttons: (0..count).map(|i| binding(buttons_at + 4 * i)).collect(),
        gshift_buttons: (0..count).map(|i| binding(gshift_at + 4 * i)).collect(),
    }
}

impl Device {
    pub fn onboard_info(&self, t: &Transport) -> Result<Info> {
        let r = self.call(t, ONBOARD_PROFILES, func::GET_INFO, &[])?;
        Ok(Info {
            memory_model: r[0],
            profile_format: r[1],
            macro_format: r[2],
            profile_count: r[3],
            rom_profile_count: r[4],
            button_count: r[5],
            sector_count: r[6],
            sector_size: u16::from_be_bytes([r[7], r[8]]),
            mechanical_layout: r[9],
            various_info: r[10],
        })
    }

    /// Reads a whole sector, 16 bytes at a time. The last read is aligned to
    /// `size - 16`, since the firmware rejects reads past the sector end.
    pub fn read_sector(&self, t: &Transport, sector: u16, size: u16) -> Result<Vec<u8>> {
        let size = usize::from(size);
        let mut data = vec![0u8; size];
        let mut offset = 0usize;
        while offset < size {
            let at = if size - offset < 16 { size - 16 } else { offset };
            let [sh, sl] = sector.to_be_bytes();
            let [oh, ol] = (at as u16).to_be_bytes();
            let r = self.call(t, ONBOARD_PROFILES, func::READ, &[sh, sl, oh, ol])?;
            data[at..at + 16].copy_from_slice(&r[..16]);
            offset = at + 16;
        }
        Ok(data)
    }

    /// Reads and decodes the directory, user profiles and ROM profiles.
    pub fn onboard_snapshot(&self, t: &Transport) -> Result<(Snapshot, Vec<RawSector>)> {
        let info = self.onboard_info(t)?;
        let size = info.sector_size;
        let buttons = usize::from(info.button_count);
        let mut raw = Vec::new();

        let directory = self.read_sector(t, DIRECTORY_SECTOR, size)?;
        let directory_crc_valid = sector_crc_ok(&directory);
        raw.push(RawSector { sector: DIRECTORY_SECTOR, crc_valid: Some(directory_crc_valid), hex: hex(&directory) });

        let mut entries = Vec::new();
        if directory_crc_valid {
            for i in 0..usize::from(info.profile_count) {
                let addr = u16::from_be_bytes([directory[4 * i], directory[4 * i + 1]]);
                if addr == DIRECTORY_END {
                    break;
                }
                entries.push((i as u8 + 1, addr, directory[4 * i + 2] != 0));
            }
        }

        let mut profiles = Vec::new();
        for (slot, sector, enabled) in entries {
            let data = self.read_sector(t, sector, size)?;
            raw.push(RawSector { sector, crc_valid: Some(sector_crc_ok(&data)), hex: hex(&data) });
            profiles.push(parse_profile(slot, sector, false, enabled, &data, buttons));
        }

        let mut rom_profiles = Vec::new();
        for i in 1..=info.rom_profile_count {
            let sector = ROM_BASE + u16::from(i);
            let data = self.read_sector(t, sector, size)?;
            raw.push(RawSector { sector, crc_valid: None, hex: hex(&data) });
            rom_profiles.push(parse_profile(i, sector, true, true, &data, buttons));
        }

        let active = self.call(t, ONBOARD_PROFILES, func::GET_ACTIVE_PROFILE, &[])?;
        let active_profile = active[1];
        let active_dpi_index = self.call(t, ONBOARD_PROFILES, func::GET_ACTIVE_DPI_INDEX, &[]).ok().map(|r| r[0]);

        Ok((Snapshot { info, directory_crc_valid, active_profile, active_dpi_index, profiles, rom_profiles }, raw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_ccitt_false_check_value() {
        // CRC-16/CCITT-FALSE (poly 0x1021, seed 0xFFFF) of "123456789" is 0x29B1.
        assert_eq!(crc_ccitt(b"123456789"), 0x29B1);
    }

    #[test]
    fn decodes_button_bindings() {
        assert_eq!(decode_binding([0x80, 0x01, 0x00, 0x01]).description, "Mouse button 1");
        assert_eq!(decode_binding([0x80, 0x01, 0x00, 0x10]).description, "Mouse button 5");
        assert_eq!(decode_binding([0x90, 0x05, 0x00, 0x00]).description, "DPI cycle");
        assert_eq!(decode_binding([0xFF, 0xFF, 0xFF, 0xFF]).kind, "disabled");
    }
}
