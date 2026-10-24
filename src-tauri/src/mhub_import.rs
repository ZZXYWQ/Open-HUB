//! Read-only M HUB import. Only a whitelist of device/board fields is used.
use crate::profiles::{ImportSummary, Profile, ProfileStore};
use serde_json::Value;
use std::path::{Path, PathBuf};
pub fn default_path() -> Result<PathBuf, String> {
    Ok(
        PathBuf::from(std::env::var_os("APPDATA").ok_or("APPDATA 未设置")?)
            .join("MCHOSEHUB/files/config/mc_main_store_key.json"),
    )
}
pub fn read(path: &Path) -> Result<Value, String> {
    let meta = std::fs::metadata(path).map_err(|_| {
        "未找到 M HUB 本地配置，请先在官方软件里使用一次鼠标，或选择配置文件".to_string()
    })?;
    if meta.len() > 8 * 1024 * 1024 {
        return Err("M HUB 配置文件超过 8 MB".into());
    }
    serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|_| "M HUB 配置不是有效 JSON".into())
}
pub fn import(store: &mut ProfileStore, doc: &Value) -> Result<ImportSummary, String> {
    let entries = doc
        .get("webIndexList")
        .and_then(Value::as_array)
        .ok_or("未发现 M HUB webIndexList 配置记录")?;
    let mut pending = Vec::new();
    let mut skipped = 0;
    for device in entries {
        let vendor = device["vendorId"].as_u64();
        let pid = device["productId"].as_u64();
        let model = device["deviceName"].as_str() == Some("A7 V3 Ultra+")
            || device["productName"].as_str() == Some("MCHOSE A7 V3 Ultra+");
        if vendor != Some(0x3837) || !matches!(pid, Some(0x1014) | Some(0x4026)) || !model {
            skipped += 1;
            continue;
        }
        let Some(boards) = device.pointer("/profile/detail").and_then(Value::as_array) else {
            skipped += 1;
            continue;
        };
        for board in boards {
            let Some(index) = board["index"].as_u64().filter(|n| *n < 3) else {
                skipped += 1;
                continue;
            };
            let source = board["source"].as_str();
            if source.is_some_and(|s| s != "custom" && s != "onboard") {
                skipped += 1;
                continue;
            }
            let name = board["name"].as_str().unwrap_or("").trim();
            let id = format!("mhub-3837-4026-board-{index}");
            if pending.iter().any(|p: &Profile| p.id == id) {
                skipped += 1;
                continue;
            }
            let app_path = store.get(&id).and_then(|p| p.app_path.clone());
            pending.push(Profile {
                target_vendor: Some(0x3837),
                target_model: Some(0x4026),
                mchose_board: Some(index as u8),
                sino: None,
            multi: None,
                id,
                name: format!(
                    "M HUB · {}",
                    if name.is_empty() {
                        format!("板载配置 {}", index + 1)
                    } else {
                        name.chars().take(80).collect()
                    }
                ),
                app_path,
                levels: vec![],
                active_level: 0,
                report_rate_hz: None,
                ghub_id: None,
            });
        }
    }
    if pending.is_empty() {
        return Err("没有可导入的 A7 V3 Ultra+ 板载配置；其他型号需要先添加对应适配器".into());
    }
    let mut result = ImportSummary {
        imported: 0,
        updated: 0,
        skipped,
    };
    for p in pending {
        if store.get(&p.id).is_some() {
            result.updated += 1
        } else {
            result.imported += 1
        }
        store.upsert(p);
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn official_boards_read_only_and_idempotent() {
        let doc: Value =
            serde_json::from_str(include_str!("../tests/fixtures/mhub-boards.json")).unwrap();
        let mut s = ProfileStore::default();
        let r = import(&mut s, &doc).unwrap();
        assert_eq!(r.imported, 3);
        let p = s.get("mhub-3837-4026-board-0").unwrap();
        assert_eq!(p.mchose_board, Some(0));
        assert!(p.levels.is_empty());
        assert!(p.targets(0x3837, &[0x4026]));
        assert!(!p.targets(0x046d, &[0x4093]));
        let r = import(&mut s, &doc).unwrap();
        assert_eq!(r.updated, 3);
        assert_eq!(s.profiles.len(), 4);
    }
    #[test]
    fn unsupported_model_does_not_modify_store() {
        let mut doc: Value =
            serde_json::from_str(include_str!("../tests/fixtures/mhub-boards.json")).unwrap();
        doc["webIndexList"][0]["vendorId"] = serde_json::json!(0x046d);
        let mut s = ProfileStore::default();
        assert!(import(&mut s, &doc).is_err());
        assert_eq!(s.profiles.len(), 1);
    }
}

