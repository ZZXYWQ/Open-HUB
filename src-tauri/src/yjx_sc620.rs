//! SC620 protocol work, independently derived from the official 1.2.2 archive.
//! Not registered: A8A4/A8A5:2255 are shared identifiers. No live writer.
//! Source addresses and remaining gates: docs/AULA-SC620-PROTOCOL.md.
#![allow(dead_code)]
use gearwise_core::{Error, HidTransport, Result};
use std::time::{Duration, Instant};
const FRAME: usize = 64;
const CONFIG_LEN: usize = 46;
const BINDING_LEN: usize = 44;
const PAGES: u8 = 6;
fn invalid(reason: &str) -> Error {
    Error::invalid(format!("SC620 protocol: {reason}"))
}
/// Only proven read commands. 0D/00 is a WRITE, including with a zero body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Read {
    Identity,
    Bindings { page: u8 },
    Config { page: u8 },
}
impl Read {
    pub fn report(self) -> Result<[u8; 65]> {
        let (cmd, sub, len, page) = match self {
            Self::Identity => (3, 0, 0, 0),
            Self::Bindings { page } => (8, 0xa5, BINDING_LEN, page),
            Self::Config { page } => (0x0e, 0xa5, CONFIG_LEN, page),
        };
        if page >= PAGES {
            return Err(invalid("page is outside the official six-page layout"));
        }
        let mut raw = [0; FRAME];
        raw[0] = 0x55;
        raw[1] = cmd;
        raw[2] = sub;
        raw[4] = len as u8;
        raw[7] = page;
        Ok(output(raw))
    }
    fn response<'a>(self, raw: &'a [u8]) -> Result<&'a [u8]> {
        let request = self.report()?;
        if raw.len() != FRAME {
            return Err(invalid("input must contain exactly 64 bytes"));
        }
        if raw[0] != 0xaa || raw[1] != request[2] || raw[2] != request[3] {
            return Err(invalid("response command does not match the query"));
        }
        if raw[5..8] != request[6..9] {
            return Err(invalid("response offset/page does not match the query"));
        }
        let len = usize::from(raw[4]);
        if len > FRAME - 8 {
            return Err(invalid("response payload exceeds the frame"));
        }
        if self != Self::Identity && raw[4] != request[5] {
            return Err(invalid("response payload length differs from the query"));
        }
        // Response checksum semantics are not established. Do not pretend
        // the transmitter's checksum algorithm is a verified receive check.
        Ok(&raw[8..8 + len])
    }
}
/// OEM 43A450..43A484: sum Windows report bytes 5..64 into byte 4.
fn output(mut raw: [u8; FRAME]) -> [u8; 65] {
    raw[3] = raw[4..].iter().fold(0u8, |sum, b| sum.wrapping_add(*b));
    let mut report = [0; 65];
    report[1..].copy_from_slice(&raw);
    report
}
/// Caller owns an already-open handle and its lock. Discovery does not call
/// this diagnostic, and it never promotes shared USB IDs to a model name.
pub fn query(io: &mut dyn HidTransport, request: Read) -> Result<Vec<u8>> {
    let report = request.report()?;
    let mut raw = [0; FRAME + 1];
    let mut drained = false;
    for _ in 0..32 {
        if io.read_timeout(&mut raw, 0)? == 0 {
            drained = true;
            break;
        }
    }
    if !drained {
        return Err(invalid("input did not become idle before the query"));
    }
    if io.write(&report)? != report.len() {
        return Err(invalid("short query write; query was not retried"));
    }
    let deadline = Instant::now() + Duration::from_millis(1000);
    for _ in 0..32 {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let n = io.read_timeout(&mut raw, left.as_millis().min(100) as i32)?;
        if n == 0 {
            continue;
        }
        if n > raw.len() {
            return Err(invalid("invalid transport read length"));
        }
        // Skip only complete unrelated event/reply frames. Matching short or
        // malformed frames fail; never turn a short read into zero padding.
        if n == FRAME && raw[0] == 0xaa && (raw[1] != report[2] || raw[2] != report[3]) {
            continue;
        }
        request.response(&raw[..n])?;
        return Ok(raw[..n].to_vec());
    }
    Err(invalid("query timed out; query was not retried"))
}
/// Read-preserving image. No factory values substituted for unknown bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigImage {
    page: u8,
    bytes: [u8; CONFIG_LEN],
}
/// OEM lighting combobox entries 0..6 (language IDs 531..537).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum LightMode {
    Wave = 0,
    Neon = 1,
    CyclingFlash = 2,
    YoyoBall = 3,
    SingleFlash = 4,
    BreathingLoop = 5,
    Off = 6,
}
impl LightMode {
    fn from_byte(byte: u8) -> Option<Self> {
        Some(match byte {
            0 => Self::Wave,
            1 => Self::Neon,
            2 => Self::CyclingFlash,
            3 => Self::YoyoBall,
            4 => Self::SingleFlash,
            5 => Self::BreathingLoop,
            6 => Self::Off,
            _ => return None,
        })
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigPatch {
    pub report_rate_hz: Option<u16>,
    pub debounce_ms: Option<u8>,
    pub lod_mm: Option<u8>,
    pub scroll_reverse: Option<bool>,
    pub esports: Option<bool>,
    pub sleep_minutes: Option<u8>,
    pub ripple_control: Option<bool>,
    pub angle_snap: Option<bool>,
    pub motion_sync: Option<bool>,
    pub light: Option<LightMode>,
    /// Zero-based stage; firmware stores 1..6.
    pub active: Option<u8>,
    pub dpi: Option<[u16; 6]>,
    /// Per-DPI indicator colours, not a general lighting effect.
    pub colors: Option<[[u8; 3]; 6]>,
}
impl ConfigImage {
    pub fn from_response(page: u8, response: &[u8]) -> Result<Self> {
        let data = Read::Config { page }.response(response)?;
        if data[0] != page {
            return Err(invalid("embedded page differs from header"));
        }
        if data[3] != 6 || !(1..=6).contains(&data[4]) {
            return Err(invalid("unverified DPI stage layout"));
        }
        let mut bytes = [0; CONFIG_LEN];
        bytes.copy_from_slice(data);
        let image = Self { page, bytes };
        validate_dpi(&image.dpi())?;
        Ok(image)
    }
    pub fn lod_mm(&self)->Option<u8>{matches!(self.bytes[41],1|2).then_some(self.bytes[41])}
    pub fn scroll_reverse(&self)->Option<bool>{match self.bytes[40]{0=>Some(false),1=>Some(true),_=>None}}
    pub fn esports(&self)->Option<bool>{match self.bytes[45]{1=>Some(false),2=>Some(true),_=>None}}
    pub fn sleep_minutes(&self)->Option<u8>{matches!(self.bytes[44],1|3|5|10|20|30|60).then_some(self.bytes[44])}
    pub fn ripple_control(&self)->bool{self.bytes[42]&0x10!=0}
    pub fn angle_snap(&self)->bool{self.bytes[42]&1!=0}
    pub fn motion_sync(&self)->bool{self.bytes[42]&0x20!=0}
    pub fn report_rate_hz(&self)->Option<u16>{match self.bytes[2]{1=>Some(125),2=>Some(250),3=>Some(500),4=>Some(1000),_=>None}}
    pub fn debounce_ms(&self)->Option<u8>{(self.bytes[43]<=20).then_some(self.bytes[43])}
    pub fn light(&self) -> Option<LightMode> {
        LightMode::from_byte(self.bytes[1])
    }
    pub fn page(&self) -> u8 {
        self.page
    }
    pub fn active(&self) -> u8 {
        self.bytes[4] - 1
    }
    pub fn dpi(&self) -> [u16; 6] {
        std::array::from_fn(|i| u16::from_le_bytes([self.bytes[5 + i * 2], self.bytes[6 + i * 2]]))
    }
    pub fn colors(&self) -> [[u8; 3]; 6] {
        std::array::from_fn(|i| self.bytes[17 + i * 3..20 + i * 3].try_into().unwrap())
    }
    pub fn bytes(&self) -> &[u8; CONFIG_LEN] {
        &self.bytes
    }
    pub fn patch(&self, changes: &ConfigPatch) -> Result<Self> {
        if changes == &ConfigPatch::default() {
            return Err(invalid("empty configuration patch"));
        }
        if changes.active.is_some_and(|v| v >= 6) {
            return Err(invalid("active stage must be 0..5"));
        }
        if let Some(dpi) = changes.dpi {
            validate_dpi(&dpi)?;
        }
        if let Some(rate)=changes.report_rate_hz{if !matches!(rate,125|250|500|1000){return Err(invalid("unverified SC620 report rate"));}}
        if changes.debounce_ms.is_some_and(|ms|ms>20){return Err(invalid("SC620 debounce must be0..20ms"));}
        if changes.lod_mm.is_some_and(|v|!matches!(v,1|2)){return Err(invalid("SC620 LOD is1or2mm"));}
        if changes.sleep_minutes.is_some_and(|v|!matches!(v,1|3|5|10|20|30|60)){return Err(invalid("unverified SC620 sleep minutes"));}
        let mut next = self.clone();
        if let Some(v)=changes.lod_mm{next.bytes[41]=v;}
        if let Some(v)=changes.scroll_reverse{next.bytes[40]=u8::from(v);}
        if let Some(v)=changes.esports{next.bytes[45]=if v{2}else{1};}
        if let Some(v)=changes.sleep_minutes{next.bytes[44]=v;}
        for (selected,mask) in [(changes.ripple_control,0x10),(changes.angle_snap,1),(changes.motion_sync,0x20)]{if let Some(on)=selected{next.bytes[42]=(next.bytes[42]&!mask)|if on{mask}else{0};}}
        if let Some(rate)=changes.report_rate_hz{next.bytes[2]=match rate{125=>1,250=>2,500=>3,1000=>4,_=>unreachable!()};}
        if let Some(ms)=changes.debounce_ms{next.bytes[43]=ms;}
        if let Some(light) = changes.light {
            next.bytes[1] = light as u8;
        }
        if let Some(active) = changes.active {
            next.bytes[4] = active + 1;
        }
        if let Some(dpi) = changes.dpi {
            for (i, value) in dpi.iter().enumerate() {
                next.bytes[5 + i * 2..7 + i * 2].copy_from_slice(&value.to_le_bytes());
            }
        }
        if let Some(colors) = changes.colors {
            for (i, color) in colors.iter().enumerate() {
                next.bytes[17 + i * 3..20 + i * 3].copy_from_slice(color);
            }
        }
        Ok(next)
    }
    /// Packet compilation only, no live executor until identity/recovery gates.
    pub fn compile_output(&self) -> [u8; 65] {
        let mut raw = [0; FRAME];
        raw[0] = 0x55;
        raw[1] = 0x0f;
        raw[2] = 0xae;
        raw[4] = CONFIG_LEN as u8;
        raw[7] = self.page;
        raw[8..8 + CONFIG_LEN].copy_from_slice(&self.bytes);
        output(raw)
    }
}
fn validate_dpi(dpi: &[u16; 6]) -> Result<()> {
    // OEM checks unsigned(value - 50) <= 0x655E, no inferred step size.
    if dpi.iter().any(|v| !(50..=26000).contains(v)) {
        return Err(invalid("DPI outside official 50..26000 range"));
    }
    Ok(())
}
#[path = "yjx_sc620_macros.rs"]
pub mod macros;
#[path = "yjx_sc620_oem_compile.rs"]
pub mod oem_compile;
#[path = "yjx_sc620_oem_config.rs"]
pub mod oem_config;
#[path = "yjx_sc620_oem_library.rs"]
pub mod oem_library;
#[path = "yjx_sc620_library_plan.rs"]
pub mod library_plan;
#[path = "yjx_sc620_bindings.rs"]
pub mod bindings;

#[cfg(test)]
#[path = "yjx_sc620_tests.rs"]
mod tests;
