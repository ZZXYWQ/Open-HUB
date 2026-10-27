//! Current-board full backup preview/restore. Shared or unrelated macros are
//! never overwritten merely because a backup used their historical slot number.
use crate::{
    device::Device,
    hidpp::{Error, Result},
    mchose::Transport,
    mchose_config::{self as native, Snapshot},
    mchose_target::Target,
    mhub_full::{self, Plan},
    mhub_transaction::{self, Applied, Keys},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, io::Read, path::Path};
const LIMIT: u64 = 8 * 1024 * 1024;
fn bad(s: impl Into<String>) -> Error {
    Error::Other(s.into())
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub target: Target,
    pub board: u8,
    pub sha256: String,
    pub before_sha256: String,
}
#[derive(Serialize)]
pub struct Preview {
    pub selection: Selection,
    pub source_format: String,
    pub source_firmware: String,
    pub source_receiver_firmware: String,
    pub device_name: String,
    pub current: native::HardwareState,
    pub desired: native::HardwareState,
    pub slot_map: std::collections::BTreeMap<u8, u8>,
    pub macros: Vec<MacroSummary>,
    pub unrelated_backup_macros: usize,
}
#[derive(Serialize)]
pub struct MacroSummary {
    pub source_slot: u8,
    pub target_slot: u8,
    pub name: String,
    pub mode: u8,
    pub events: Vec<native::MacroEvent>,
    pub upload: bool,
}
struct Loaded {
    snapshot: Snapshot,
    sha256: String,
}
fn fields(v: &serde_json::Value, allowed: &[&str], at: &str) -> Result<()> {
    let object = v
        .as_object()
        .ok_or_else(|| bad(format!("{at}必须为对象")))?;
    if let Some(name) = object.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(bad(format!("{at}含未解释字段 {name}，不能忽略后恢复")));
    }
    Ok(())
}
fn schema(v: &serde_json::Value) -> Result<()> {
    fields(v, &["format", "model", "state", "macros"], "备份")?;
    fields(
        &v["state"],
        &[
            "profile",
            "names",
            "dpi_x",
            "dpi_y",
            "count",
            "active",
            "independent",
            "sleep",
            "never_sleep",
            "left_debounce",
            "right_debounce",
            "sensor",
            "rotation",
            "lod",
            "buttons",
            "adaptive",
            "firmware",
            "receiver_firmware",
            "raw_settings",
            "raw_x",
            "raw_y",
            "raw_buttons",
        ],
        "备份状态",
    )?;
    if let Some(macros) = v["macros"].as_array() {
        for m in macros {
            fields(m, &["slot", "name", "mode", "events", "raw"], "宏")?;
            if let Some(events) = m["events"].as_array() {
                for e in events {
                    fields(e, &["kind", "code", "down", "delay_ms"], "宏事件")?;
                }
            }
        }
    }
    Ok(())
}
pub(crate) fn validate_restore_shape(saved: &Snapshot) -> Result<()> {
    native::validate_snapshot(saved)?;
    let s = &saved.state;
    if native::mirrored_linked_axes(s)&&!s.independent&&s.dpi_x!=s.dpi_y{return Err(bad("鼠标17/接收器6的联动 XY 会镜像 Y，不能精确恢复不同的未启用 Y 表；未写入"));}
    if s.raw_buttons != s.buttons.concat()
        || s.raw_settings[1] & 15 > 5
        || s.raw_settings[2] & 15 > 5
    {
        return Err(bad("备份含未解释的按键尾部或无效 DPI 索引，不能忽略后恢复"));
    }
    Ok(())
}
fn read(path: &Path) -> Result<Loaded> {
    let file = std::fs::File::open(path).map_err(|e| bad(e.to_string()))?;
    let meta = file.metadata().map_err(|e| bad(e.to_string()))?;
    if !meta.is_file() || meta.len() > LIMIT {
        return Err(bad("请选择不超过8 MiB的普通迈从备份文件"));
    }
    let mut bytes = vec![];
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| bad(e.to_string()))?;
    if bytes.len() as u64 > LIMIT {
        return Err(bad("迈从备份超过8 MiB"));
    }
    let strict: crate::mhub_full::StrictValue =
        serde_json::from_slice(&bytes).map_err(|e| bad(format!("备份JSON无效或字段重复：{e}")))?;
    schema(&strict.0)?;
    let snapshot: Snapshot =
        serde_json::from_value(strict.0).map_err(|e| bad(format!("不是完整迈从备份：{e}")))?;
    validate_restore_shape(&snapshot)?;
    Ok(Loaded {
        snapshot,
        sha256: crate::onboard::backup_digest(&bytes),
    })
}
fn digest(before: &Snapshot, keys: &Keys) -> Result<String> {
    let bytes = serde_json::to_vec(&(before, keys)).map_err(|e| bad(e.to_string()))?;
    Ok(crate::onboard::backup_digest(&bytes))
}
fn capture(t: &Transport) -> Result<(Snapshot, Keys)> {
    let before = t.full_snapshot()?;
    let keys = mhub_transaction::read_keys(t, before.state.profile)?;
    native::validate_snapshot(&before)?;
    if keys[before.state.profile as usize] != before.state.buttons {
        return Err(bad("板载按键已变化，请重新预览"));
    }
    Ok((before, keys))
}
pub(crate) fn compile(
    saved: &Snapshot,
    before: &Snapshot,
    keys: &Keys,
) -> Result<mhub_full::Compiled> {
    validate_restore_shape(saved)?;
    native::validate_snapshot(before)?;
    let s = &saved.state;
    let b = &before.state;
    if s.profile != b.profile {
        return Err(bad(
            "请先选择与备份相同的板载组，再重新预览；不自动切换板载组",
        ));
    }
    if s.firmware != b.firmware || s.receiver_firmware != b.receiver_firmware {
        return Err(bad("备份与当前鼠标或接收器固件不同，尚未核对跨固件恢复"));
    }
    if s.raw_settings[4] & !1 != b.raw_settings[4] & !1
        || s.sensor & 0x60 != b.sensor & 0x60
        || s.raw_settings[9..] != b.raw_settings[9..]
    {
        return Err(bad("备份包含与当前不同的未解释参数，不能覆盖未知字段"));
    }
    let refs = s
        .buttons
        .iter()
        .filter(|k| k.first() == Some(&0x21))
        .map(|k| k[1])
        .collect::<BTreeSet<_>>();
    let macros = refs
        .iter()
        .map(|slot| {
            saved
                .macros
                .iter()
                .find(|m| m.slot == *slot)
                .cloned()
                .ok_or_else(|| bad("备份缺少当前板载组引用的宏正文"))
        })
        .collect::<Result<Vec<_>>>()?;
    let plan = Plan {
        vendor: crate::mchose::VID,
        model: crate::mchose::MODEL_PID,
        board: s.profile,
        name: s.names[s.profile as usize].clone(),
        dpi_x: s.dpi_x.clone(),
        dpi_y: Some(s.dpi_y.clone()),
        count: s.count,
        active: s.active,
        independent: s.independent,
        usb_rate: s.raw_settings[1] >> 4,
        wireless_rate: s.raw_settings[2] >> 4,
        sleep: s.sleep,
        never_sleep: s.never_sleep,
        left_debounce: s.left_debounce,
        right_debounce: s.right_debounce,
        sensor: s.sensor,
        rotation: s.rotation,
        lod: s.lod,
        buttons: s.buttons.clone(),
        macros,
    };
    let mut compiled = mhub_full::compile_restore(&plan, before, keys).map_err(bad)?;
    // Restore exact raw tables and both header nibbles. The observed linked
    // firmware's mirror invariant was checked before compilation; never discard
    // an incompatible inactive Y silently. Portable Plan semantics differ.
    compiled.snapshot.state.dpi_y = s.dpi_y.clone();
    compiled.snapshot.state.raw_settings = s.raw_settings.clone();
    compiled.snapshot.state.raw_x = s.raw_x.clone();
    compiled.snapshot.state.raw_y = s.raw_y.clone();
    native::validate_snapshot(&compiled.snapshot)?;
    Ok(compiled)
}
pub fn preview(
    device: &Device,
    t: &Transport,
    path: &Path,
    target: &Target,
    expected_board: u8,
) -> Result<Preview> {
    target.verify(device)?;
    let loaded = read(path)?;
    if loaded.snapshot.state.profile != expected_board {
        return Err(bad("备份与当前选择的板载组不同，请先选择对应组并重新预览"));
    }
    let (before, keys) = capture(t)?;
    let compiled = compile(&loaded.snapshot, &before, &keys)?;
    let macros = compiled
        .slot_map
        .iter()
        .map(|(source, target)| {
            let m = loaded
                .snapshot
                .macros
                .iter()
                .find(|m| m.slot == *source)
                .expect("compiled source macro");
            MacroSummary {
                source_slot: *source,
                target_slot: *target,
                name: m.name.clone(),
                mode: m.mode,
                events: m.events.clone(),
                upload: compiled.macro_writes.iter().any(|m| m.slot == *target),
            }
        })
        .collect::<Vec<_>>();
    Ok(Preview {
        selection: Selection {
            target: target.clone(),
            board: expected_board,
            sha256: loaded.sha256,
            before_sha256: digest(&before, &keys)?,
        },
        source_format: loaded.snapshot.format,
        source_firmware: loaded.snapshot.state.firmware,
        source_receiver_firmware: loaded.snapshot.state.receiver_firmware,
        device_name: device.name.clone(),
        current: before.state,
        desired: compiled.snapshot.state,
        slot_map: compiled.slot_map,
        unrelated_backup_macros: loaded.snapshot.macros.len() - macros.len(),
        macros,
    })
}
pub fn restore(
    device: &Device,
    t: &Transport,
    path: &Path,
    selection: &Selection,
) -> Result<Applied> {
    selection.target.verify(device)?;
    if selection.board > 2
        || [&selection.sha256, &selection.before_sha256]
            .iter()
            .any(|s| s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(bad("恢复预览标记无效，请重新预览；未写入"));
    }

    let loaded = read(path)?;
    if loaded.sha256 != selection.sha256 {
        return Err(bad("备份文件已改变，请重新选择并预览；未写入"));
    }
    if loaded.snapshot.state.profile != selection.board {
        return Err(bad("恢复板载组与预览不一致；未写入"));
    }
    let (before, keys) = capture(t)?;
    if digest(&before, &keys)? != selection.before_sha256 {
        return Err(bad("鼠标配置或宏已在预览后变化，请重新预览；未写入"));
    }
    let board = before.state.profile;
    let compiled = compile(&loaded.snapshot, &before, &keys)?;
    mhub_transaction::commit_compiled(t, board, before, keys, compiled, true)
}
