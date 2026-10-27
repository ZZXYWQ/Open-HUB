//! Portable logical native macro files, never transport/selection archives.
use gearwise_core::profile::Macro;
use serde::{Deserialize, Serialize};
use std::path::Path;
const FORMAT: &str = "open-hub-logitech-native-macro/1";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Archive {
    format: String,
    definition: Macro,
}
fn validate(definition: &Macro) -> Result<(), String> {
    crate::onboard_macro_plan::compile(definition, 256)
        .map(|_| ())
        .map_err(|e| e.to_string())
}
fn decode(bytes: &[u8]) -> Result<Macro, String> {
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("宏文件超过8 MiB".into());
    }
    let crate::mhub_full::StrictValue(raw) =
        serde_json::from_slice(bytes).map_err(|e| format!("宏文件JSON无效：{e}"))?;
    let archive: Archive = serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
    if archive.format != FORMAT || serde_json::to_value(&archive).map_err(|e| e.to_string())? != raw
    {
        return Err("需要完整的罗技板载宏格式1，不能忽略未知字段或设备选择".into());
    }
    validate(&archive.definition)?;
    Ok(archive.definition)
}
pub fn read(path: &Path) -> Result<Macro, String> {
    decode(&crate::mhub_full::read_explicit_file(path)?)
}
pub fn write(path: &Path, definition: Macro) -> Result<(), String> {
    validate(&definition)?;
    let bytes = serde_json::to_vec_pretty(&Archive {
        format: FORMAT.into(),
        definition,
    })
    .map_err(|e| e.to_string())?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("宏文件超过8 MiB".into());
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|e| format!("请使用新的宏文件名：{e}"))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())
}
#[cfg(test)]
#[path = "onboard_macro_archive_tests.rs"]
mod tests;
