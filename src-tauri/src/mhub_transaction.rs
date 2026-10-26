//! Commit a complete known M HUB plan to one live current board. No automatic
//! board switch, retry or rollback that could overwrite a subsequent writer.
use crate::{
    device::Device,
    hidpp::{Error, Result},
    mchose::Transport,
    mchose_config::{self as native, HardwareState, MouseMacro},
    mchose_target::Target,
    mhub_full::{self, Plan},
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

type Keys = [Vec<Vec<u8>>; 3];
#[derive(Serialize)]
pub struct Applied {
    #[serde(flatten)]
    pub state: HardwareState,
    pub slot_map: BTreeMap<u8, u8>,
    pub uploaded_macros: Vec<u8>,
    pub backup: Option<String>,
}
fn error(message: impl Into<String>) -> Error {
    Error::Other(message.into())
}
fn changed() -> Error {
    error("设备配置或其他板载组引用已改变，请重新读取；不继续写入")
}
fn read_keys(t: &Transport, board: u8) -> Result<Keys> {
    let mut out: [Vec<Vec<u8>>; 3] = std::array::from_fn(|_| vec![]);
    for p in 0..3 {
        out[p] = native::parse_buttons(&t.request(1, &[p as u8, 0, 6])?)?;
    }
    t.require_board(board)?;
    Ok(out)
}
fn require_keys(t: &Transport, board: u8, expected: &Keys) -> Result<()> {
    t.require_board(board)?;
    if read_keys(t, board)? != *expected {
        return Err(changed());
    }
    Ok(())
}
fn require_state(t: &Transport, board: u8, expected: &HardwareState, keys: &Keys) -> Result<()> {
    t.require_board(board)?;
    if t.hardware_state()? != *expected {
        return Err(changed());
    }
    require_keys(t, board, keys)
}
fn require_macro(t: &Transport, slot: u8, expected: Option<&MouseMacro>) -> Result<()> {
    if t.read_macro(slot)?.as_ref() != expected {
        return Err(error("宏槽内容已改变，请重新读取；不覆盖新的内容"));
    }
    Ok(())
}

/// The manager lock must remain held across this call. The Target is verified
/// before identity GET, backup or SET. A transport generation is not an RF unit id.
pub fn apply(
    device: &Device,
    t: &Transport,
    target: &Target,
    board: u8,
    plan: &Plan,
) -> Result<Applied> {
    crate::mchose_target::with_target(device, t, target, |t| apply_current(t, board, plan))
}
fn apply_current(t: &Transport, board: u8, plan: &Plan) -> Result<Applied> {
    mhub_full::validate(plan).map_err(error)?;
    if board != plan.board {
        return Err(error("方案板载组与当前读取目标不一致"));
    }
    t.identity()?;
    t.require_board(board)?;
    let before = t.full_snapshot()?;
    let mut keys = read_keys(t, board)?;
    let compiled = mhub_full::compile(plan, &before, &keys).map_err(error)?;
    require_state(t, board, &before.state, &keys)?;
    let needed = compiled.slot_map.values().copied().collect::<BTreeSet<_>>();
    for slot in &needed {
        require_macro(t, *slot, before.macros.iter().find(|m| m.slot == *slot))?;
    }
    if compiled.macro_writes.is_empty() && compiled.snapshot.state == before.state {
        return Ok(Applied {
            state: before.state,
            slot_map: compiled.slot_map,
            uploaded_macros: vec![],
            backup: None,
        });
    }
    let backup = t.persist_snapshot(&before)?;
    let desired = &compiled.snapshot.state;
    let mut expected = before.state.clone();
    let trial = (|| -> Result<()> {
        // The exact before-image is durable before the first SET. Recheck the
        // read set after disk I/O rather than adopting a new baseline silently.
        require_state(t, board, &expected, &keys)?;
        for slot in &needed {
            require_macro(t, *slot, before.macros.iter().find(|m| m.slot == *slot))?;
        }
        for record in &compiled.macro_writes {
            require_state(t, board, &expected, &keys)?;
            require_macro(
                t,
                record.slot,
                before.macros.iter().find(|m| m.slot == record.slot),
            )?;
            t.write_macro_checked(record.slot, &record.raw, || require_keys(t, board, &keys))?;
        }
        require_state(t, board, &expected, &keys)?;
        // Settings and DPI headers can share firmware state (active index).
        // Treat them as one verified stage instead of assuming intermediate
        // GETs keep the old DPI header after SET settings.
        if expected.raw_settings != desired.raw_settings {
            require_keys(t, board, &keys)?;
            t.checked_settings(&desired.raw_settings)?;
        }
        if expected.raw_x != desired.raw_x || expected.raw_y != desired.raw_y {
            t.write_dpi_table_checked(
                board,
                &desired.dpi_x,
                &desired.dpi_y,
                desired.count,
                desired.active,
                desired.independent,
                || require_keys(t, board, &keys),
            )?;
        }
        expected.raw_settings = desired.raw_settings.clone();
        expected.raw_x = desired.raw_x.clone();
        expected.raw_y = desired.raw_y.clone();
        expected.dpi_x = desired.dpi_x.clone();
        expected.dpi_y = desired.dpi_y.clone();
        expected.count = desired.count;
        expected.active = desired.active;
        expected.independent = desired.independent;
        expected.sleep = desired.sleep;
        expected.never_sleep = desired.never_sleep;
        expected.left_debounce = desired.left_debounce;
        expected.right_debounce = desired.right_debounce;
        expected.sensor = desired.sensor;
        expected.rotation = desired.rotation;
        require_state(t, board, &expected, &keys)?;
        if expected.lod != desired.lod {
            require_keys(t, board, &keys)?;
            t.request(0x0109, &[board, desired.lod])?;
            if t.request(9, &[board])? != [desired.lod] {
                return Err(error("LOD 回读未匹配"));
            }
            expected.lod = desired.lod;
        }
        require_state(t, board, &expected, &keys)?;
        // Establish every destination ordinary left button before replacing any
        // prior left button, then commit the remaining changed mappings.
        let first = (0..6)
            .filter(|i| desired.buttons[*i] == [0, 1, 0])
            .collect::<Vec<_>>();
        let rest = (0..6)
            .filter(|i| desired.buttons[*i] != [0, 1, 0])
            .collect::<Vec<_>>();
        for index in first.into_iter().chain(rest) {
            if expected.buttons[index] == desired.buttons[index] {
                continue;
            }
            require_state(t, board, &expected, &keys)?;
            t.write_binding(board, index as u8, &desired.buttons[index])?;
            expected.buttons[index] = desired.buttons[index].clone();
            expected.raw_buttons = expected.buttons.concat();
            keys[board as usize] = expected.buttons.clone();
            require_state(t, board, &expected, &keys)?;
        }
        if expected.names[board as usize] != desired.names[board as usize] {
            require_state(t, board, &expected, &keys)?;
            t.write_name_checked(board, &desired.names[board as usize], || {
                require_keys(t, board, &keys)
            })?;
            expected.names[board as usize] = desired.names[board as usize].clone();
        }
        require_state(t, board, desired, &keys)?;
        for slot in &needed {
            require_macro(
                t,
                *slot,
                compiled.snapshot.macros.iter().find(|m| m.slot == *slot),
            )?;
        }
        t.require_board(board)?;
        Ok(())
    })();
    if let Err(reason) = trial {
        return Err(error(format!(
            "{reason}；可能已有部分设置生效，未自动重试。原始备份：{}",
            backup.display()
        )));
    }
    Ok(Applied {
        state: compiled.snapshot.state,
        slot_map: compiled.slot_map,
        uploaded_macros: compiled.macro_writes.iter().map(|m| m.slot).collect(),
        backup: Some(backup.to_string_lossy().into_owned()),
    })
}
