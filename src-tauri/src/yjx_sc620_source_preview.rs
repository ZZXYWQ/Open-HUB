//! Explicit-file source preview/archive only. Never returns an applicable Profile.
use super::{
    bindings::{Action, Playback},
    oem_compile, oem_config,
    oem_library::{db, open_source},
    oem_profile::{self, Source},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};
const FORMAT: &str = "open-hub-sc620-source/1";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pack {
    format: String,
    source: Source,
}
#[derive(Serialize, Deserialize)]
pub struct KeyPreview {
    pub source_control: i64,
    pub layer: i64,
    pub record: Option<u8>,
    pub action: Option<Action>,
    pub macro_slot: Option<u8>,
    pub problem: Option<String>,
}
#[derive(Serialize, Deserialize)]
pub struct MacroPreview {
    pub id: i64,
    pub problem: Option<String>,
}
#[derive(Serialize, Deserialize)]
pub struct Preview {
    pub profile_ids: Vec<i64>,
    pub source: Source,
    pub keys: Vec<KeyPreview>,
    pub macros: Vec<MacroPreview>,
    pub issues: Vec<String>,
    pub static_subset_convertible: bool,
    pub hardware_apply_available: bool,
}
fn safe_integer(n: i64) -> bool {
    (-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&n)
}
fn validate(source: &Source) -> Result<(), String> {
    let checked = oem_config::convert(source.settings.profile_id, source.settings.fields.clone())
        .map_err(|e| e.to_string())?;
    if checked != source.settings {
        return Err("来源设置与转换摘要不一致".into());
    }
    if source.keys.len() > 66
        || source.macros.len() > 32
        || source.macros.iter().map(|m| m.events.len()).sum::<usize>() > 4096
    {
        return Err("来源条目超过上限".into());
    }
    let mut keys = BTreeSet::new();
    for row in &source.keys {
        if ![
            row.id,
            row.profile,
            row.layer,
            row.key_code,
            row.layout_value,
            row.kind,
            row.value,
            row.value2,
            row.value3,
        ]
        .into_iter()
        .all(safe_integer)
            || row.id <= 0
            || row.profile != source.settings.profile_id
            || row.layout_description.len() > 1024
            || row.description.len() > 1024
            || !keys.insert((row.layer, row.key_code))
        {
            return Err("来源按键字段或重复控制无效".into());
        }
    }
    let mut macros = BTreeSet::new();
    let mut record_ids = BTreeSet::new();
    for m in &source.macros {
        if m.id <= 0
            || m.id > i32::MAX as i64
            || m.name.len() > 256
            || !macros.insert(m.id)
            || !(0..=i32::MAX as i64).contains(&m.delay_type)
            || !(0..=i32::MAX as i64).contains(&m.delay_time)
            || Playback::from_oem(m.play_type, m.play_times).map_err(|e| e.to_string())?
                != m.playback
        {
            return Err("来源宏头或模式不一致".into());
        }
        let mut last = None;
        for e in &m.events {
            if !safe_integer(e.record_id)
                || !safe_integer(e.index)
                || e.record_id <= 0
                || !record_ids.insert(e.record_id)
                || e.index < 0
                || last.is_some_and(|i| i >= e.index)
                || e.description.len() > 1024
                || !(0..=i32::MAX as i64).contains(&e.kind)
                || !(i32::MIN as i64..=i32::MAX as i64).contains(&e.value)
                || !(0..=i32::MAX as i64).contains(&e.delay_raw)
            {
                return Err("来源宏事件字段或顺序无效".into());
            }
            last = Some(e.index);
        }
    }
    Ok(())
}
pub fn assess(source: Source, profile_ids: Vec<i64>) -> Result<Preview, String> {
    validate(&source)?;
    let mut issues = Vec::new();
    if !source.settings.unconverted.is_empty() {
        issues.push(format!(
            "{} 项设置尚未转换，不能准备整份配置",
            source.settings.unconverted.len()
        ));
    }
    let library = oem_compile::prepare(&source.macros);
    let has_macros = source.keys.iter().any(|k| k.kind == 23);
    if has_macros {
        if let Err(e) = &library {
            issues.push(format!("宏库无法完整转换：{e}"));
        }
    }
    let mut keys = Vec::new();
    for row in &source.keys {
        let record = oem_profile::record(row.key_code).ok();
        let mut problem = None;
        let mut action = None;
        let mut macro_slot = None;
        if row.layer != 0 {
            problem = Some("暂未转换此按键层".into());
        } else if record.is_none() {
            problem = Some("尚未核对的原厂控制 ID".into());
        } else if row.kind == 23 {
            match &library {
                Ok(lib) => match lib.slots.iter().find(|s| s.oem_id == row.value) {
                    Some(s) => macro_slot = Some(s.binding.slot),
                    None => problem = Some("引用的宏 ID 不存在".into()),
                },
                Err(_) => problem = Some("宏库含未支持内容，无法确定完整槽位方案".into()),
            }
        } else {
            match oem_profile::basic(row) {
                Ok(a) => action = Some(a),
                Err(e) => problem = Some(e.to_string()),
            }
        }
        if let Some(p) = &problem {
            issues.push(format!("控制 {}／层 {}：{p}", row.key_code, row.layer));
        }
        keys.push(KeyPreview {
            source_control: row.key_code,
            layer: row.layer,
            record,
            action,
            macro_slot,
            problem,
        });
    }
    let macros = source
        .macros
        .iter()
        .map(|m| MacroPreview {
            id: m.id,
            problem: oem_compile::prepare(std::slice::from_ref(m))
                .err()
                .map(|e| e.to_string()),
        })
        .collect();
    Ok(Preview {
        profile_ids,
        source,
        keys,
        macros,
        static_subset_convertible: issues.is_empty(),
        issues,
        hardware_apply_available: false,
    })
}
pub fn preview(path: &Path, profile: Option<i64>) -> Result<Preview, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > 8 * 1024 * 1024 {
        return Err("需要不超过8 MiB的普通配置文件".into());
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.starts_with(b"SQLite format 3\0") {
        let c = open_source(path).map_err(|e| e.to_string())?;
        let tx = db(c.unchecked_transaction()).map_err(|e| e.to_string())?;
        let mut stmt = tx
            .prepare("SELECT DISTINCT profile FROM t_configdata ORDER BY profile LIMIT 65")
            .map_err(|e| e.to_string())?;
        let ids = stmt
            .query_map([], |r| r.get::<_, i64>(0))
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        drop(stmt);
        if ids.is_empty()
            || ids.len() > 64
            || ids.iter().any(|id| *id <= 0 || *id > i32::MAX as i64)
        {
            return Err("来源配置 ID 或数量无效".into());
        }
        let id = profile.unwrap_or(ids[0]);
        if !ids.contains(&id) {
            return Err("来源文件没有这组配置".into());
        }
        let source = oem_profile::parse(&tx, id).map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        assess(source, ids)
    } else {
        let raw: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let pack: Pack = serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
        if pack.format != FORMAT || serde_json::to_value(&pack).map_err(|e| e.to_string())? != raw {
            return Err("需要完整的 SC620 源方案格式1，不能忽略未知字段".into());
        }
        let id = pack.source.settings.profile_id;
        if profile.is_some_and(|v| v != id) {
            return Err("源方案不含所选配置".into());
        }
        assess(pack.source, vec![id])
    }
}
/// Export a source archive, including unsupported raw fields. No device writes.
/// create_new prevents overwriting an OEM file or an existing user archive.
pub fn export(path: &Path, source: Source) -> Result<(), String> {
    validate(&source)?;
    let bytes = serde_json::to_vec_pretty(&Pack {
        format: FORMAT.into(),
        source,
    })
    .map_err(|e| e.to_string())?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("源方案超过8 MiB".into());
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("请使用新的源方案文件名：{e}"))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())
}
#[cfg(test)]
#[path = "yjx_sc620_source_preview_tests.rs"]
mod tests;
