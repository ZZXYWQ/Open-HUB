//! On-board profile memory (HID++ 0x8100).
//!
//! Layout (matches G HUB's onboard_profiles model and libratbag's HID++
//! implementation): memory is split into sectors. Sector 0x0000 is the
//! profile directory (4-byte entries: big-endian sector address, enabled
//! flag, reserved; 0xFFFF ends the list). User profiles live in sectors
//! 0x0001.., factory (ROM) profiles in 0x0101... Every user sector ends with
//! a big-endian CRC-CCITT (seed 0xFFFF) of the bytes before it.
//!
//! Reads have no side effects. Writes ([`Device::write_slot_settings`],
//! [`Device::restore_sectors`]) only touch user profile sectors and the
//! directory, change only the bytes Open HUB manages, recompute the CRC and
//! verify by reading back. Factory (ROM) sectors are never written.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::device::{feature::ONBOARD_PROFILES, Device};
use crate::hidpp::{Error, Result, FeatureRequest};

const DIRECTORY_SECTOR: u16 = 0x0000;
const ROM_BASE: u16 = 0x0100;
const DIRECTORY_END: u16 = 0xFFFF;
/// Committing a sector to flash can take longer than an ordinary request.
const COMMIT_TIMEOUT: Duration = Duration::from_secs(5);
/// Report rates the on-board format can store, as a period in milliseconds.
const STORABLE_RATES_MS: [u8; 4] = [1, 2, 4, 8];

/// Function numbers of feature 0x8100.
mod func {
    pub const GET_INFO: u8 = 0;
    pub const SET_ACTIVE_PROFILE: u8 = 3;
    pub const GET_ACTIVE_PROFILE: u8 = 4;
    pub const READ: u8 = 5;
    pub const WRITE_START: u8 = 6;
    pub const WRITE_DATA: u8 = 7;
    pub const WRITE_END: u8 = 8;
    pub const GET_ACTIVE_DPI_INDEX: u8 = 0xB;
}

/// New DPI levels, starting level and report rate for one user slot.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotSettings {
    pub expected_device: String,
    pub expected_connection:String,
    pub sector: u16,
    pub slot: u8,
    pub dpi_levels: Vec<u16>,
    pub default_index: u8,
    pub report_rate_hz: u32,
}

/// Result of a verified write.
#[derive(Clone, Debug, Serialize)]
pub struct WriteReport {
    pub slot: u8,
    pub sector: u16,
    /// Bytes that differ from what was stored before.
    pub bytes_changed: usize,
    /// True when nothing changed, so nothing was written.
    pub unchanged: bool,
    pub active_reloaded: bool,
    pub reload_note: Option<String>,
}

pub struct PreparedDpi{tx:crate::onboard_restore::Prepared,report:WriteReport}
impl PreparedDpi{pub fn raw(&self)->Vec<RawSector>{self.tx.raw()}}

/// Puts `settings` into a copy of a profile sector. Only the report rate,
/// starting and shift DPI indices and the five DPI values change; buttons,
/// lighting, power settings and the name are kept byte for byte. The CRC is
/// recomputed.
pub fn patch_profile_sector(original: &[u8], settings: &SlotSettings) -> std::result::Result<Vec<u8>, String> {
    if original.len() < 0x20 {
        return Err("sector too small for a profile".into());
    }
    let levels = &settings.dpi_levels;
    if levels.is_empty() || levels.len() > 5 {
        return Err("between 1 and 5 DPI levels are required".into());
    }
    if levels.iter().any(|&d| d == 0 || d == 0xFFFF) {
        return Err("DPI levels must be between 1 and 65534".into());
    }
    if usize::from(settings.default_index) >= levels.len() {
        return Err("the starting level must be one of the DPI levels".into());
    }
    let period = (1000 / settings.report_rate_hz.max(1)).min(255) as u8;
    if !STORABLE_RATES_MS.contains(&period) || 1000 / u32::from(period) != settings.report_rate_hz {
        return Err(format!(
            "{} Hz can't be stored on the mouse; use 125, 250, 500 or 1000 Hz",
            settings.report_rate_hz
        ));
    }

    let mut data = original.to_vec();
    data[0] = period;
    data[1] = settings.default_index;
    // Keep the shift level valid for the new number of levels.
    if usize::from(data[2]) >= levels.len() {
        data[2] = 0;
    }
    for i in 0..5 {
        let value = levels.get(i).copied().unwrap_or(0);
        data[3 + 2 * i..5 + 2 * i].copy_from_slice(&value.to_le_bytes());
    }
    let n = data.len();
    let crc = crc_ccitt(&data[..n - 2]);
    data[n - 2..].copy_from_slice(&crc.to_be_bytes());
    Ok(data)
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

pub(crate) fn sector_crc_ok(data: &[u8]) -> bool {
    data.len() >= 2 && crc_ccitt(&data[..data.len() - 2]) == u16::from_be_bytes([data[data.len() - 2], data[data.len() - 1]])
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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
    /// New binding writes currently require a direct physical HID++ handle.
    pub bindings_writable:bool,
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
    pub device_key: String,
    pub connection_key:String,
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
#[derive(Clone, Debug, Serialize, Deserialize)]
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
                ("mouse", if mask.count_ones() > 1 {
                    let buttons = (1..=16).filter(|n| mask & (1 << (n - 1)) != 0).map(|n| n.to_string()).collect::<Vec<_>>();
                    format!("Mouse buttons {}", buttons.join(" + "))
                } else { format!("Mouse button {button}") })
            }
            0x02 => ("keyboard", format!("Key 0x{:02x} (modifiers 0x{:02x})", raw[3], raw[2])),
            0x03 => ("consumer", format!("Media key 0x{:04x}", u16::from_be_bytes([raw[2], raw[3]]))),
            other => ("unknown", format!("HID subtype 0x{other:02x}")),
        },
        0x90 => {
            let name = SPECIALS.get(usize::from(raw[1])).copied().unwrap_or("invalid");
            ("special", name.to_string())
        }
        0x00 => ("macro", format!("Macro at page {} offset {}", raw[1], u16::from_be_bytes([raw[2],raw[3]]))),
        0xFF => ("disabled", "Disabled".to_string()),
        other => ("unknown", format!("Type 0x{other:02x}")),
    };
    ButtonBinding { raw, kind, description }
}

/// The native name field is 24 UTF-16LE units (also for non-ASCII names).
/// An all-FF field requests the firmware default. Invalid UTF-16 stays unnamed
/// rather than guessing a different encoding or replacing opaque bytes.
fn decode_name(raw:&[u8])->Option<String>{
    if raw.iter().all(|b|*b==0xff){return None;}
    let units=raw.chunks_exact(2).map(|c|u16::from_le_bytes([c[0],c[1]])).take_while(|u|*u!=0&&*u!=0xffff).collect::<Vec<_>>();
    String::from_utf16(&units).ok().map(|s|s.trim().to_string()).filter(|s|!s.is_empty())
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

fn parse_hex(hex: &str) -> std::result::Result<Vec<u8>, String> {
    if hex.len() % 2 != 0 || !hex.is_ascii() {
        return Err("malformed hex in backup".into());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

/// Sectors from a backup that may be written back: the directory and user
/// profile sectors whose stored CRC is valid. Factory (ROM) sectors are skipped.
pub fn restorable_sectors(raw: &[RawSector]) -> std::result::Result<Vec<(u16, Vec<u8>)>, String> {
    let mut out = Vec::new();
    for s in raw {
        if s.sector >= ROM_BASE || s.crc_valid != Some(true) {
            continue;
        }
        let data = parse_hex(&s.hex)?;
        if !sector_crc_ok(&data) {
            return Err(format!("backup sector 0x{:04x} fails its checksum", s.sector));
        }
        out.push((s.sector, data));
    }
    if out.is_empty() {
        return Err("the backup contains no restorable sectors".into());
    }
    Ok(out)
}


/// A profile restore is scoped by model tags and the exact memory layout.
/// Original raw macro/ROM pages may be retained in the file; this legacy
/// restore operation changes the directory and referenced user profiles only.
pub fn scoped_restore(file:&serde_json::Value,current_models:&[u16],info:&Info)->std::result::Result<Vec<(u16,Vec<u8>)>,String>{
    crate::onboard_bindings::validate_info(info)?;
    if info.sector_count==0{return Err("mouse did not report live memory geometry".into());}
    if file.get("format").and_then(|v|v.as_str())!=Some("open-ghub-onboard-backup/1"){return Err("not an Open HUB on-board restore backup".into());}
    let tags:Vec<String>=serde_json::from_value(file.get("model_ids").cloned().unwrap_or_default()).map_err(|_|"backup model metadata is missing or malformed")?;
    if tags.is_empty()||tags.len()>16{return Err("backup model identity is missing or unverified".into());}
    let ids=tags.iter().map(|s|{if s.len()!=4||!s.is_ascii(){return Err("backup model ID is malformed");}u16::from_str_radix(s,16).map_err(|_|"backup model ID is malformed")}).collect::<std::result::Result<Vec<_>,_>>()?;
    if !ids.iter().any(|id|*id!=0&&*id!=0xffff&&current_models.contains(id)){return Err("backup belongs to a different or unknown mouse model".into());}
    let layout=file.pointer("/snapshot/info").ok_or("backup layout metadata is missing")?;
    for (field,value) in [("memory_model",u64::from(info.memory_model)),("profile_format",u64::from(info.profile_format)),("macro_format",u64::from(info.macro_format)),("profile_count",u64::from(info.profile_count)),("button_count",u64::from(info.button_count)),("sector_count",u64::from(info.sector_count)),("sector_size",u64::from(info.sector_size)),("mechanical_layout",u64::from(info.mechanical_layout))]{
        if layout.get(field).and_then(|v|v.as_u64())!=Some(value){return Err(format!("backup {field} differs from this mouse; no restore was sent"));}
    }
    let raw:Vec<RawSector>=serde_json::from_value(file.get("sectors").cloned().unwrap_or_default()).map_err(|e|format!("backup sectors are unreadable: {e}"))?;
    let mut seen=std::collections::BTreeSet::new();if raw.iter().any(|r|!seen.insert(r.sector)){return Err("backup contains duplicate sector records".into());}
    let valid=restorable_sectors(&raw)?.into_iter().collect::<std::collections::BTreeMap<_,_>>();
    if valid.iter().any(|(sector,data)|*sector>=u16::from(info.sector_count)||data.len()!=usize::from(info.sector_size)){return Err("backup live-sector geometry differs from this mouse".into());}
    let directory=valid.get(&0).ok_or("backup has no verified profile directory")?;
    let mut selected=Vec::new();let mut profiles=std::collections::BTreeSet::new();
    for slot in 0..usize::from(info.profile_count){let at=slot*4;let sector=u16::from_be_bytes([directory[at],directory[at+1]]);if sector==0xffff{break;}
        if sector==0||sector>=u16::from(info.sector_count)||!profiles.insert(sector){return Err("backup directory has an unverified or aliased user profile".into());}
        let bytes=valid.get(&sector).ok_or("backup is missing a verified referenced user profile")?;selected.push((sector,bytes.clone()));
    }
    selected.push((0,directory.clone()));Ok(selected)
}


#[derive(Debug,Serialize)]pub struct RestorePreview{pub digest:String,pub device_name:String,pub profiles:Vec<Profile>,pub macro_references:usize}
pub fn backup_digest(bytes:&[u8])->String{use sha2::Digest;sha2::Sha256::digest(bytes).iter().map(|b|format!("{b:02x}")).collect()}
pub fn read_restore_file(path:&std::path::Path)->std::result::Result<(serde_json::Value,String),String>{
    let bytes=std::fs::read(path).map_err(|e|format!("could not read {}: {e}",path.display()))?;
    let value=serde_json::from_slice(&bytes).map_err(|e|format!("not an Open HUB backup file: {e}"))?;Ok((value,backup_digest(&bytes)))
}
pub fn restore_preview(file:&serde_json::Value,models:&[u16],info:&Info,name:&str,digest:String)->std::result::Result<RestorePreview,String>{
    let sectors=scoped_restore(file,models,info)?;let directory=&sectors.iter().find(|(s,_)|*s==0).unwrap().1;let mut profiles=Vec::new();let mut macro_references=0;
    for slot in 0..usize::from(info.profile_count){let at=slot*4;let sector=u16::from_be_bytes([directory[at],directory[at+1]]);if sector==0xffff{break;}let bytes=&sectors.iter().find(|(s,_)|*s==sector).unwrap().1;
        for base in [0x20,0x60]{for button in 0..16{if bytes[base+button*4]==0{macro_references+=1;}}}
        profiles.push(parse_profile(slot as u8+1,sector,false,directory[at+2]!=0,bytes,usize::from(info.button_count)));
    }
    Ok(RestorePreview{digest,device_name:name.into(),profiles,macro_references})
}

impl Device {
    pub fn onboard_info(&self, t: &dyn FeatureRequest) -> Result<Info> {
        let r = self.call_unpadded(t, ONBOARD_PROFILES, func::GET_INFO, &[])?;
        if r.len()<11{return Err(Error::Other("truncated on-board descriptor".into()));}
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
            bindings_writable:self.index==crate::hidpp::DIRECT&&self.vendor_id==0x046d&&self.kind.eq_ignore_ascii_case("mouse"),
        })
    }

    /// Reads a whole sector, 16 bytes at a time. The last read is aligned to
    /// `size - 16`, since the firmware rejects reads past the sector end.
    pub fn read_sector(&self, t: &dyn FeatureRequest, sector: u16, size: u16) -> Result<Vec<u8>> {
        let size = usize::from(size);
        if !(16..=4096).contains(&size){return Err(Error::Other("unsupported on-board sector size".into()));}
        let mut data = vec![0u8; size];
        let mut offset = 0usize;
        while offset < size {
            let at = if size - offset < 16 { size - 16 } else { offset };
            let [sh, sl] = sector.to_be_bytes();
            let [oh, ol] = (at as u16).to_be_bytes();
            let r = self.call_unpadded(t, ONBOARD_PROFILES, func::READ, &[sh, sl, oh, ol])?;
            if r.len()<16{return Err(Error::Other("truncated on-board sector reply".into()));}
            data[at..at + 16].copy_from_slice(&r[..16]);
            offset = at + 16;
        }
        Ok(data)
    }

    /// Reads and decodes the directory, user profiles and ROM profiles.
    pub fn onboard_snapshot(&self, t: &dyn FeatureRequest) -> Result<(Snapshot, Vec<RawSector>)> {
        self.verify_native_identity(t, &self.profile_key)?;
        let info = self.onboard_info(t)?;
        let size = info.sector_size;
        if !(210..=4096).contains(&size)||usize::from(info.profile_count)*4+2>usize::from(size){return Err(Error::Other("unsupported profile/directory geometry".into()));}
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

        self.verify_native_identity(t, &self.profile_key)?;
        Ok((Snapshot { device_key: self.profile_key.clone(),connection_key:self.connection_key.clone(), info, directory_crc_valid, active_profile, active_dpi_index, profiles, rom_profiles }, raw))
    }

    /// Writes one whole sector: start (sector, offset 0, length), 16-byte
    /// chunks, then end, which commits it to flash.
    pub(crate) fn write_sector(&self, t: &dyn FeatureRequest, sector: u16, data: &[u8]) -> Result<()> {
        crate::onboard_bindings::validate_target(self).map_err(Error::Other)?;
        self.verify_native_identity(t, &self.profile_key)?;
        if sector >= ROM_BASE {
            return Err(Error::Other("refusing to write a factory (ROM) sector".into()));
        }
        let [sh, sl] = sector.to_be_bytes();
        let [ch, cl] = (data.len() as u16).to_be_bytes();
        self.call(t, ONBOARD_PROFILES, func::WRITE_START, &[sh, sl, 0, 0, ch, cl])?;
        for chunk in data.chunks(16) {
            let mut buf = [0xFFu8; 16];
            buf[..chunk.len()].copy_from_slice(chunk);
            self.call(t, ONBOARD_PROFILES, func::WRITE_DATA, &buf)?;
        }
        self.call_with_timeout(t, ONBOARD_PROFILES, func::WRITE_END, &[], COMMIT_TIMEOUT)?;
        self.verify_native_identity(t, &self.profile_key)?;
        Ok(())
    }

    /// Changes one user slot's DPI levels, starting level and report rate.
    pub fn prepare_slot_settings(&self, t: &dyn FeatureRequest, settings: &SlotSettings) -> Result<PreparedDpi> {
        crate::onboard_bindings::validate_target(self).map_err(Error::Other)?;
        self.verify_native_preview(t, &settings.expected_device,&settings.expected_connection)?;
        let info = self.onboard_info(t)?;
        crate::onboard_bindings::validate_info(&info).map_err(Error::Other)?;
        if settings.slot == 0 || settings.slot > info.profile_count {
            return Err(Error::Other(format!("slot {} doesn't exist (1–{})", settings.slot, info.profile_count)));
        }
        let directory = self.read_sector(t, DIRECTORY_SECTOR, info.sector_size)?;
        if !sector_crc_ok(&directory) {
            return Err(Error::Other("the profile directory fails its checksum; not writing".into()));
        }
        let i = usize::from(settings.slot - 1) * 4;
        let sector = u16::from_be_bytes([directory[i], directory[i + 1]]);
        if sector != settings.sector || sector == DIRECTORY_END || sector == DIRECTORY_SECTOR || sector >= ROM_BASE {
            return Err(Error::Other(format!("slot {} has no user profile sector", settings.slot)));
        }
        let original = self.read_sector(t, sector, info.sector_size)?;
        if !sector_crc_ok(&original) {
            return Err(Error::Other(format!("slot {} fails its checksum; not writing", settings.slot)));
        }
        let patched = patch_profile_sector(&original, settings).map_err(Error::Other)?;
        let bytes_changed = patched.iter().zip(&original).filter(|(a, b)| a != b).count();
        self.verify_native_preview(t, &settings.expected_device,&settings.expected_connection)?;
        let tx=crate::onboard_restore::prepare(self,t,&info,&[(sector,patched)])?;
        if !tx.matches(sector,&original)||!tx.matches(0,&directory){return Err(Error::Other("DPI profile or directory changed during planning; read it again".into()));}
        Ok(PreparedDpi{tx,report:WriteReport{slot:settings.slot,sector,bytes_changed,unchanged:bytes_changed==0,active_reloaded:false,reload_note:None}})
    }
    pub fn commit_slot_settings(&self,t:&dyn FeatureRequest,mut prepared:PreparedDpi)->Result<WriteReport>{
        crate::onboard_restore::commit(self,t,&prepared.tx)?;
        match crate::onboard_restore::reload_current(self,t,&prepared.tx){Ok(reloaded)=>prepared.report.active_reloaded=reloaded,Err(e)=>prepared.report.reload_note=Some(format!("DPI saved and readback verified; active slot reload was not confirmed: {e}"))};
        Ok(prepared.report)
    }
    #[cfg(test)]
    pub fn write_slot_settings(&self,t:&dyn FeatureRequest,settings:&SlotSettings)->Result<WriteReport>{
        let prepared=self.prepare_slot_settings(t,settings)?;self.commit_slot_settings(t,prepared)
    }

    /// Changes only explicitly reviewed physical bindings in one user sector.
    /// Caller persists the readable directory/profile backup before this method.
    pub fn write_slot_bindings(
        &self,
        t: &dyn FeatureRequest,
        settings: &crate::onboard_bindings::Settings,
    ) -> Result<crate::onboard_bindings::Report> {
        use crate::onboard_bindings::{patch, validate_info, Report};
        crate::onboard_bindings::validate_target(self).map_err(Error::Other)?;
        self.verify_native_preview(t, &settings.expected_device,&settings.expected_connection)?;
        if settings.changes.is_empty() || settings.changes.len() > 32 {
            return Err(Error::Other("select 1..32 bindings".into()));
        }
        let info = self.onboard_info(t)?;
        validate_info(&info).map_err(Error::Other)?;
        if settings.slot == 0
            || settings.slot > info.profile_count
            || settings.sector == 0
            || settings.sector >= ROM_BASE
        {
            return Err(Error::Other("invalid on-board slot or user sector".into()));
        }
        let directory = self.read_sector(t, DIRECTORY_SECTOR, info.sector_size)?;
        if !sector_crc_ok(&directory) {
            return Err(Error::Other("profile directory checksum failed".into()));
        }
        let offset = usize::from(settings.slot - 1) * 4;
        let sector = u16::from_be_bytes([directory[offset], directory[offset + 1]]);
        if sector != settings.sector {
            return Err(Error::Other(
                "profile directory changed since it was read; refresh before editing".into(),
            ));
        }
        let original = self.read_sector(t, sector, info.sector_size)?;
        let patched = patch(&original, &info, &settings.changes).map_err(Error::Other)?;
        let bytes_changed = patched
            .iter()
            .zip(&original)
            .filter(|(a, b)| a != b)
            .count();
        let mut report = Report {
            slot: settings.slot,
            sector,
            bytes_changed,
            unchanged: bytes_changed == 0,
            active_reloaded: false,
            reload_note: None,
        };
        self.verify_native_preview(t, &settings.expected_device,&settings.expected_connection)?;
        if bytes_changed == 0 {
            return Ok(report);
        }
        let outcome = self
            .write_sector(t, sector, &patched)
            .and_then(|()| self.read_sector(t, sector, info.sector_size))
            .and_then(|bytes| { self.verify_native_preview(t, &settings.expected_device,&settings.expected_connection)?; Ok(bytes) });
        let error = match outcome {
            Ok(readback) if readback == patched => None,
            Ok(_) => Some("the stored profile differs from the submitted bindings".to_string()),
            Err(e) => Some(e.to_string()),
        };
        if let Some(error) = error {
            // Do not restore into a sector whose directory ownership changed.
            let ownership = (|| -> Result<()> {
                self.verify_native_preview(t, &settings.expected_device,&settings.expected_connection)?;
                let now = self.onboard_info(t)?;
                if (
                    now.memory_model,
                    now.profile_format,
                    now.macro_format,
                    now.sector_size,
                    now.button_count,
                    now.mechanical_layout,
                ) != (
                    info.memory_model,
                    info.profile_format,
                    info.macro_format,
                    info.sector_size,
                    info.button_count,
                    info.mechanical_layout,
                ) {
                    return Err(Error::Other("memory layout changed".into()));
                }
                let now = self.read_sector(t, DIRECTORY_SECTOR, info.sector_size)?;
                if !sector_crc_ok(&now) || now != directory {
                    return Err(Error::Other("profile directory changed".into()));
                }
                Ok(())
            })();
            let restored = ownership
                .and_then(|()| self.write_sector(t, sector, &original))
                .and_then(|()| self.read_sector(t, sector, info.sector_size))
                .is_ok_and(|back| back == original);
            return Err(Error::Other(format!(
                "binding write failed ({error}); {}",
                if restored {
                    "original sector restored and verified"
                } else {
                    "original state could not be confirmed; keep the backup and refresh, no blind retry"
                }
            )));
        }
        // Persisting a slot does not force board mode or activate another slot.
        // A reload error is distinct from a verified successful flash write.
        let reload = (|| -> Result<bool> {
            self.verify_native_preview(t, &settings.expected_device,&settings.expected_connection)?;
            let mode = self.call(t, ONBOARD_PROFILES, 2, &[])?;
            if mode.first() != Some(&1) {
                return Ok(false);
            }
            let active = self.call(t, ONBOARD_PROFILES, func::GET_ACTIVE_PROFILE, &[])?;
            if active.len() < 2 {
                return Err(Error::Other("truncated active-profile reply".into()));
            }
            if active[1] != settings.slot {
                return Ok(false);
            }
            self.verify_native_preview(t, &settings.expected_device,&settings.expected_connection)?;
            self.call(
                t,
                ONBOARD_PROFILES,
                func::SET_ACTIVE_PROFILE,
                &[0, settings.slot],
            )?;
            self.verify_native_preview(t, &settings.expected_device,&settings.expected_connection)?;
            Ok(true)
        })();
        match reload {
            Ok(reloaded) => report.active_reloaded = reloaded,
            Err(e) => report.reload_note = Some(format!(
                "bindings saved and readback verified; active slot reload was not confirmed: {e}"
            )),
        };
        Ok(report)
    }

    /// Writes backed-up sectors (directory and user profiles only) back to
    /// the mouse, skipping those already identical. Returns how many changed.
    #[cfg(test)]
    pub fn restore_sectors(&self, t: &dyn FeatureRequest, sectors: &[(u16, Vec<u8>)]) -> Result<usize> {
        crate::onboard_bindings::validate_target(self).map_err(Error::Other)?;
        self.verify_native_identity(t, &self.profile_key)?;
        let info=self.onboard_info(t)?;
        let prepared=crate::onboard_restore::prepare(self,t,&info,sectors)?;
        crate::onboard_restore::commit(self,t,&prepared)
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

    fn sample_sector() -> Vec<u8> {
        let mut s = vec![0xFFu8; 255];
        s[0] = 1;
        s[1] = 0;
        s[2] = 0;
        for (i, dpi) in [1500u16, 400, 800, 0, 0].iter().enumerate() {
            s[3 + 2 * i..5 + 2 * i].copy_from_slice(&dpi.to_le_bytes());
        }
        let crc = crc_ccitt(&s[..253]);
        s[253..].copy_from_slice(&crc.to_be_bytes());
        s
    }

    #[test]
    fn patch_changes_only_managed_bytes_and_fixes_crc() {
        let original = sample_sector();
        let settings = SlotSettings { expected_device: "session:patch-only".into(),expected_connection:"connection:patch-only".into(), sector: 1, slot: 1, dpi_levels: vec![800, 1600, 3200], default_index: 1, report_rate_hz: 500 };
        let patched = patch_profile_sector(&original, &settings).unwrap();
        assert!(sector_crc_ok(&patched));
        assert_eq!(patched[0], 2); // 500 Hz = 2 ms
        assert_eq!(patched[1], 1);
        assert_eq!(u16::from_le_bytes([patched[3], patched[4]]), 800);
        assert_eq!(u16::from_le_bytes([patched[7], patched[8]]), 3200);
        assert_eq!(u16::from_le_bytes([patched[9], patched[10]]), 0);
        // Everything after the DPI values (color, power, buttons, name) is untouched.
        assert_eq!(&patched[0x0D..253], &original[0x0D..253]);
    }

    #[test]
    fn patch_rejects_unstorable_values() {
        let original = sample_sector();
        let rate = SlotSettings { expected_device: "session:patch-only".into(),expected_connection:"connection:patch-only".into(), sector: 1, slot: 1, dpi_levels: vec![800], default_index: 0, report_rate_hz: 8000 };
        assert!(patch_profile_sector(&original, &rate).is_err());
        let index = SlotSettings { expected_device: "session:patch-only".into(),expected_connection:"connection:patch-only".into(), sector: 1, slot: 1, dpi_levels: vec![800], default_index: 1, report_rate_hz: 1000 };
        assert!(patch_profile_sector(&original, &index).is_err());
        let count = SlotSettings { expected_device: "session:patch-only".into(),expected_connection:"connection:patch-only".into(), sector: 1, slot: 1, dpi_levels: vec![1; 6], default_index: 0, report_rate_hz: 1000 };
        assert!(patch_profile_sector(&original, &count).is_err());
    }

    #[test]
    fn restore_skips_rom_and_invalid_sectors() {
        let good = sample_sector();
        let hexed: String = good.iter().map(|b| format!("{b:02x}")).collect();
        let raw = vec![
            RawSector { sector: 0x0001, crc_valid: Some(true), hex: hexed.clone() },
            RawSector { sector: 0x0101, crc_valid: None, hex: hexed.clone() },
            RawSector { sector: 0x0002, crc_valid: Some(false), hex: hexed },
        ];
        let sectors = restorable_sectors(&raw).unwrap();
        assert_eq!(sectors.len(), 1);
        assert_eq!(sectors[0].0, 0x0001);
    }

    #[test]
    fn decodes_button_bindings() {
        assert_eq!(decode_binding([0x80, 0x01, 0x00, 0x01]).description, "Mouse button 1");
        assert_eq!(decode_binding([0x80, 0x01, 0x00, 0x10]).description, "Mouse button 5");
        assert_eq!(decode_binding([0x90, 0x05, 0x00, 0x00]).description, "DPI cycle");
        assert_eq!(decode_binding([0xFF, 0xFF, 0xFF, 0xFF]).kind, "disabled");
    }
}

#[cfg(test)]
#[path="onboard_rpc_tests.rs"]
mod rpc_tests;
