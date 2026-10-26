//! Explicit, bounded light-only snapshots. Portable files carry family/firmware;
//! the restore preview separately carries a live connection and current state.
use crate::{
    effects::Engine,
    magdock::{Controller, Light, View},
};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::Path,
};
type R<T> = Result<T, String>;
const LIMIT: u64 = 16 * 1024;
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Backup {
    pub format: String,
    pub vendor: u16,
    pub product: u16,
    pub firmware: String,
    pub light: Light,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Saved {
    Versioned(Backup),
    Legacy(Light),
}
struct Loaded {
    light: Light,
    firmware: Option<String>,
    sha256: String,
}
#[derive(Serialize)]
pub struct Preview {
    pub light: Light,
    pub source_firmware: Option<String>,
    pub legacy: bool,
    #[serde(flatten)]
    pub selection: Selection,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub connection_key: String,
    pub sha256: String,
    pub expected_light: Light,
    pub expected_firmware: String,
}
fn loaded(bytes: &[u8]) -> R<Loaded> {
    if bytes.len() > LIMIT as usize {
        return Err("底座备份不能超过16 KB".into());
    }
    let saved: Saved =
        serde_json::from_slice(bytes).map_err(|e| format!("不是有效的底座灯效备份：{e}"))?;
    let (light, firmware) = match saved {
        Saved::Legacy(light) => (light, None),
        Saved::Versioned(b) => {
            if b.format != "open-hub-magdock-light/1"
                || b.vendor != 0x3837
                || b.product != 0x1012
                || b.firmware.is_empty()
                || b.firmware.len() > 32
                || !b.firmware.bytes().all(|v| v.is_ascii_digit() || v == b'.')
            {
                return Err("不支持的底座备份格式、设备或固件标记".into());
            }
            (b.light, Some(b.firmware))
        }
    };
    light.validate()?;
    Ok(Loaded {
        light,
        firmware,
        sha256: crate::onboard::backup_digest(bytes),
    })
}
fn read(path: &Path) -> R<Loaded> {
    let mut bytes = vec![];
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    loaded(&bytes)
}
fn idle_view(c: &Controller, engine: &Engine, connection: &str) -> R<View> {
    c.check_connection(connection)?;
    if engine.controls_dock()? {
        return Err("请先停止底座同步，再导出或恢复底座设置".into());
    }
    let view = c.view_selected(connection)?;
    if view.music_running {
        return Err("请先停止底座音乐律动，再导出或恢复底座设置".into());
    }
    Ok(view)
}
fn compatible(saved: &Loaded, view: &View) -> R<()> {
    if saved
        .firmware
        .as_ref()
        .is_some_and(|fw| fw != &view.firmware)
    {
        return Err("备份与当前底座固件不同，尚未核对跨固件恢复".into());
    }
    Ok(())
}
pub fn export(c: &Controller, engine: &Engine, path: &Path, connection: &str) -> R<()> {
    engine.route(|| {
        let view = idle_view(c, engine, connection)?;
        let backup = Backup {
            format: "open-hub-magdock-light/1".into(),
            vendor: 0x3837,
            product: 0x1012,
            firmware: view.firmware,
            light: view.light,
        };
        let bytes = serde_json::to_vec_pretty(&backup).map_err(|e| e.to_string())?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())
    })
}
pub fn preview(c: &Controller, engine: &Engine, path: &Path, connection: &str) -> R<Preview> {
    let saved = read(path)?;
    engine.route(|| {
        let view = idle_view(c, engine, connection)?;
        compatible(&saved, &view)?;
        Ok(Preview {
            light: saved.light,
            legacy: saved.firmware.is_none(),
            source_firmware: saved.firmware,
            selection: Selection {
                connection_key: view.connection_key,
                sha256: saved.sha256,
                expected_light: view.light,
                expected_firmware: view.firmware,
            },
        })
    })
}
pub fn restore(c: &Controller, engine: &Engine, path: &Path, selection: &Selection) -> R<View> {
    let saved = read(path)?;
    if selection.sha256 != saved.sha256 {
        return Err("备份文件在预览后已改变，请重新选择".into());
    }
    selection.expected_light.validate()?;
    engine.route(|| {
        let current = idle_view(c, engine, &selection.connection_key)?;
        compatible(&saved, &current)?;
        if current.light != selection.expected_light
            || current.firmware != selection.expected_firmware
        {
            return Err("底座状态在预览后已改变，请重新选择备份".into());
        }
        c.apply_reviewed(
            saved.light,
            &selection.connection_key,
            Some((&selection.expected_light, &selection.expected_firmware)),
        )
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Light {
        serde_json::from_value(serde_json::json!({"enabled":true,"mode":4,"mode_count":6,"speed":1,"brightness":1,"sync":false,"color":[255,0,0],"direction":1})).unwrap()
    }
    #[test]
    fn strict_legacy_and_family_backup_preserve_all_known_fields() {
        let light = sample();
        let old = serde_json::to_vec(&light).unwrap();
        let parsed = loaded(&old).unwrap();
        assert!(parsed.firmware.is_none());
        assert_eq!(parsed.light, light);
        let b = Backup {
            format: "open-hub-magdock-light/1".into(),
            vendor: 0x3837,
            product: 0x1012,
            firmware: "0.3".into(),
            light: light.clone(),
        };
        let parsed = loaded(&serde_json::to_vec(&b).unwrap()).unwrap();
        assert_eq!(parsed.firmware.as_deref(), Some("0.3"));
        assert_eq!(parsed.light, light);
    }
    #[test]
    fn foreign_unknown_duplicate_incomplete_and_oversized_files_are_rejected() {
        let light = serde_json::to_value(sample()).unwrap();
        let valid = serde_json::json!({"format":"open-hub-magdock-light/1","vendor":0x3837,"product":0x1012,"firmware":"0.3","light":light});
        for (key, value) in [
            ("format", serde_json::json!("other/1")),
            ("vendor", serde_json::json!(0x046d)),
            ("product", serde_json::json!(0x1014)),
            ("firmware", serde_json::json!("")),
            ("firmware", serde_json::json!("x")),
        ] {
            let mut bad = valid.clone();
            bad[key] = value;
            assert!(loaded(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
        let mut extra = light.clone();
        extra["other"] = true.into();
        assert!(loaded(&serde_json::to_vec(&extra).unwrap()).is_err());
        let text = serde_json::to_string(&light).unwrap();
        let duplicate = text.replacen("{", "{\"enabled\":false,", 1);
        assert!(loaded(duplicate.as_bytes()).is_err());
        assert!(loaded(b"{}").is_err());
        assert!(loaded(&vec![b' '; LIMIT as usize + 1]).is_err());
    }
}
