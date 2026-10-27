//! Strict original settings subset preview; no model inference, UI, or HID.
use super::{
    invalid,
    oem_library::{db, open_source, schema},
    ConfigImage, ConfigPatch, LightMode,
};
use gearwise_core::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// File-side profile ID. Does not choose a hardware page or mouse.
    pub profile_id: i64,
    pub fields: BTreeMap<String, String>,
    pub patch: ConfigPatch,
    pub unconverted: BTreeMap<String, String>,
}
impl Source {
    /// Offline only. Unknown source settings block whole-source conversion.
    /// Regenerate patch from source fields so a modified preview cannot add
    /// different values or silently clear the unconverted-field gate.
    pub fn prepare(&self, original: &ConfigImage) -> Result<ConfigImage> {
        let checked = convert(self.profile_id, self.fields.clone())?;
        if &checked != self {
            return Err(invalid("OEM settings preview changed since conversion"));
        }
        if !self.unconverted.is_empty() {
            return Err(invalid("OEM settings contain unconverted fields"));
        }
        original.patch(&self.patch)
    }
}
pub fn read(path: &std::path::Path, profile_id: i64) -> Result<Source> {
    let c = open_source(path)?;
    let tx = db(c.unchecked_transaction())?;
    let result = parse(&tx, profile_id)?;
    db(tx.commit())?;
    Ok(result)
}
fn parse(c: &rusqlite::Connection, profile_id: i64) -> Result<Source> {
    schema(
        c,
        "t_configdata",
        &[
            ("config_id", "INTEGER", 1),
            ("profile", "INTEGER", 0),
            ("key", "TEXT", 0),
            ("value", "TEXT", 0),
        ],
    )?;
    if profile_id <= 0 || profile_id > i32::MAX as i64 {
        return Err(invalid("OEM source profile ID"));
    }
    let mut stmt = db(c.prepare(
        "SELECT key,value FROM t_configdata WHERE profile=?1 ORDER BY config_id LIMIT 65",
    ))?;
    let rows = db(stmt.query_map([profile_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    }))?;
    let mut fields = BTreeMap::new();
    for row in rows {
        let (k, v) = db(row)?;
        if fields.len() >= 64 || k.len() > 128 || v.len() > 512 || fields.insert(k, v).is_some() {
            return Err(invalid("duplicate/oversized OEM config fields"));
        }
    }
    convert(profile_id, fields)
}
fn value(fields: &BTreeMap<String, String>, key: &str, max: u32) -> Result<u32> {
    let s = fields
        .get(key)
        .ok_or_else(|| invalid(&format!("missing OEM config key {key}")))?;
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid(&format!("nondecimal OEM config key {key}")));
    }
    let n = s
        .parse::<u32>()
        .map_err(|_| invalid("OEM config integer overflow"))?;
    if n > max {
        return Err(invalid(&format!("OEM config {key} out of range")));
    }
    Ok(n)
}
fn convert(profile_id: i64, fields: BTreeMap<String, String>) -> Result<Source> {
    if profile_id <= 0 || profile_id > i32::MAX as i64 {
        return Err(invalid("OEM source profile ID"));
    }
    if fields.len() > 64 || fields.iter().any(|(k, v)| k.len() > 128 || v.len() > 512) {
        return Err(invalid("oversized OEM config fields"));
    }
    let light = match value(&fields, "light_mode", 6)? {
        0 => LightMode::Wave,
        1 => LightMode::Neon,
        2 => LightMode::CyclingFlash,
        3 => LightMode::YoyoBall,
        4 => LightMode::SingleFlash,
        5 => LightMode::BreathingLoop,
        6 => LightMode::Off,
        _ => unreachable!(),
    };
    let rate = match value(&fields, "report_rate", 4)? {
        1 => 125,
        2 => 250,
        3 => 500,
        4 => 1000,
        _ => return Err(invalid("unverified OEM report rate code")),
    };
    let debounce = value(&fields, "button_respondtime", 20)? as u8;
    let active = value(&fields, "dpi_index", 5)? as u8;
    let mut dpi = [0u16; 6];
    let mut colors = [[0u8; 3]; 6];
    let mut unconverted = fields.clone();
    for key in [
        "light_mode",
        "report_rate",
        "button_respondtime",
        "dpi_index",
    ] {
        unconverted.remove(key);
    }
    for i in 0..6 {
        let dk = format!("dpi_group{i}_value");
        let ck = format!("dpi_group{i}_rgb");
        let n = value(&fields, &dk, 26000)?;
        if n < 50 {
            return Err(invalid("OEM DPI below50"));
        }
        dpi[i] = n as u16;
        let c = value(&fields, &ck, 0xffffff)?;
        colors[i] = [c as u8, (c >> 8) as u8, (c >> 16) as u8];
        unconverted.remove(&dk);
        unconverted.remove(&ck);
    }
    let mut patch = ConfigPatch {
        light: Some(light),
        active: Some(active),
        dpi: Some(dpi),
        colors: Some(colors),
        report_rate_hz: Some(rate),
        debounce_ms: Some(debounce),
        ..ConfigPatch::default()
    };
    if fields.contains_key("liftoff_height") {
        let n = value(&fields, "liftoff_height", 255)?;
        if matches!(n, 1 | 2) {
            patch.lod_mm = Some(n as u8);
            unconverted.remove("liftoff_height");
        }
    }
    if fields.contains_key("scroll_flag") {
        let n = value(&fields, "scroll_flag", 255)?;
        if n <= 1 {
            patch.scroll_reverse = Some(n == 1);
            unconverted.remove("scroll_flag");
        }
    }
    if fields.contains_key("e-sports_flag") {
        let n = value(&fields, "e-sports_flag", 255)?;
        if matches!(n, 1 | 2) {
            patch.esports = Some(n == 2);
            unconverted.remove("e-sports_flag");
        }
    }
    if fields.contains_key("sleep_light") {
        let n = value(&fields, "sleep_light", 255)?;
        if matches!(n, 1 | 3 | 5 | 10 | 20 | 30 | 60) {
            patch.sleep_minutes = Some(n as u8);
            unconverted.remove("sleep_light");
        }
    }
    if fields.contains_key("sensor_flag") {
        let n = value(&fields, "sensor_flag", 255)?;
        // The source's two low bits are retained file metadata. The verified
        // SC620 native writer reads only4/8/16; other higher bits stay gated.
        if n & !0x1f == 0 {
            patch.ripple_control = Some(n & 4 != 0);
            patch.angle_snap = Some(n & 8 != 0);
            patch.motion_sync = Some(n & 16 != 0);
            unconverted.remove("sensor_flag");
        }
    }
    if fields.contains_key("dpi_flag") {
        if value(&fields, "dpi_flag", 255)? == 6 {
            unconverted.remove("dpi_flag");
        }
    }
    Ok(Source {
        profile_id,
        fields,
        patch,
        unconverted,
    })
}

#[cfg(test)]
#[path = "yjx_sc620_oem_config_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "yjx_sc620_tuning_tests.rs"]
mod tuning_tests;
