//! Standalone macro editing with copy-on-write for other-board references.
//! No transport discovery, board selection, host input, or automatic retry.
use crate::{
    hidpp::{Error, Result},
    mchose::Transport,
    mchose_config::{self as native, HardwareState, MouseMacro, Snapshot},
    mhub_full::Compiled,
    mhub_transaction::{self, Keys},
};
use std::collections::{BTreeMap, BTreeSet};
fn bad(s: impl Into<String>) -> Error {
    Error::Other(s.into())
}

pub(crate) fn compile(before: &Snapshot, keys: &Keys, record: &MouseMacro) -> Result<Compiled> {
    native::validate_snapshot(before)?;
    let board = usize::from(before.state.profile);
    if keys[board] != before.state.buttons {
        return Err(bad("当前板载按键已改变，请重新读取"));
    }
    let canonical = native::macro_record(
        record.slot,
        &record.name,
        record.mode,
        &record.events,
        Some(&record.raw),
    )?;
    if canonical != *record {
        return Err(bad("宏内容与原始数据不一致"));
    }
    let mut references = BTreeSet::new();
    let mut shared = false;
    let mut unknown_other = false;
    for (index, buttons) in keys.iter().enumerate() {
        if buttons.len() != 6 || native::parse_buttons(&buttons.concat())? != *buttons {
            return Err(bad("三组按键快照不完整"));
        }
        for binding in buttons {
            if index != board && binding != &[255; 3] && native::validate_binding(binding).is_err()
            {
                unknown_other = true;
            }
            if binding[0] == 0x21 {
                native::validate_binding(binding)?;
                references.insert(binding[1]);
                if index != board && binding[1] == record.slot {
                    shared = true;
                }
            }
        }
    }
    let existing = before.macros.iter().find(|m| m.slot == record.slot);
    let same = existing.is_some_and(|m| m.raw == record.raw);
    // Preserve an explicitly selected private slot. When other boards may use it,
    // reuse identical bytes or allocate a truly empty, unreferenced global slot.
    let target = if same || (!shared && !unknown_other) {
        record.slot
    } else {
        before
            .macros
            .iter()
            .find(|m| m.raw == record.raw)
            .map(|m| m.slot)
            .or_else(|| {
                if unknown_other {
                    None
                } else {
                    (0..18).find(|slot| {
                        !references.contains(slot) && !before.macros.iter().any(|m| m.slot == *slot)
                    })
                }
            })
            .ok_or_else(|| {
                bad("宏被其他板载组引用或存在未知引用，且没有安全可用宏槽；原宏和按键未修改")
            })?
    };
    let mut desired = before.clone();
    let mut saved = record.clone();
    saved.slot = target;
    let same_target = before
        .macros
        .iter()
        .any(|m| m.slot == target && m.raw == saved.raw);
    if let Some(old) = desired.macros.iter_mut().find(|m| m.slot == target) {
        *old = saved.clone();
    } else {
        desired.macros.push(saved.clone());
    }
    desired.macros.sort_by_key(|m| m.slot);
    if target != record.slot {
        for binding in &mut desired.state.buttons {
            if binding == &[0x21, record.slot, 0] {
                binding[1] = target;
            }
        }
        desired.state.raw_buttons = desired.state.buttons.concat();
    }
    native::validate_snapshot(&desired)?;
    Ok(Compiled {
        snapshot: desired,
        macro_writes: if same_target { vec![] } else { vec![saved] },
        slot_map: BTreeMap::from([(record.slot, target)]),
    })
}

pub(crate) fn save(
    t: &Transport,
    board: u8,
    record: MouseMacro,
) -> Result<(HardwareState, MouseMacro)> {
    // Reject malformed drafts before backup or native I/O.
    native::macro_record(
        record.slot,
        &record.name,
        record.mode,
        &record.events,
        Some(&record.raw),
    )?;
    t.identity()?;
    t.require_board(board)?;
    let before = t.full_snapshot()?;
    let keys = mhub_transaction::read_keys(t, board)?;
    let compiled = compile(&before, &keys, &record)?;
    let actual = *compiled
        .slot_map
        .get(&record.slot)
        .ok_or_else(|| bad("缺少保存宏槽"))?;
    let saved = compiled
        .snapshot
        .macros
        .iter()
        .find(|m| m.slot == actual)
        .cloned()
        .ok_or_else(|| bad("缺少保存宏正文"))?;
    let applied = mhub_transaction::commit_compiled(t, board, before, keys, compiled, false)?;
    Ok((applied.state, saved))
}
