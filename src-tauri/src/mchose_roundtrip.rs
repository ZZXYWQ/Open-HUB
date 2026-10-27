//! Explicit diagnostic: one +/−50 active-X DPI change and owned-image restore.
//! Never runs during normal startup. No button/macro/rate changes.
//! An explicitly selected mode reads banks through reversible profile selection.
use super::*;
use crate::mhub_transaction::{self, Keys};
use std::{io::Write, path::Path};
type Banks = [[Vec<u8>; 2]; 3];
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct Capture {
    pub snapshot: Snapshot,
    pub keys: Keys,
    pub banks: Banks,
}
fn fail(s: impl Into<String>) -> crate::hidpp::Error {
    bad(s)
}
fn write_new(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| fail(e.to_string()))?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| fail(e.to_string()))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| fail(e.to_string()))
}
pub(crate) fn capture(t: &Transport) -> Result<Capture> {
    let snapshot = t.full_snapshot()?;
    crate::mchose_backup::validate_restore_shape(&snapshot)?;
    let p = snapshot.state.profile;
    let keys = mhub_transaction::read_keys(t, p)?;
    let mut banks: Banks = std::array::from_fn(|_| std::array::from_fn(|_| Vec::new()));
    for board in 0..3 {
        for axis in 0..2 {
            banks[board][axis] = t.dpi_table(board as u8, axis as u8)?;
        }
    }
    t.require_board(p)?;
    if keys[p as usize] != snapshot.state.buttons
        || banks[p as usize] != [snapshot.state.raw_x.clone(), snapshot.state.raw_y.clone()]
        || t.hardware_state()? != snapshot.state
    {
        return Err(fail("只读捕获期间配置改变；未开始诊断写入"));
    }
    Ok(Capture {
        snapshot,
        keys,
        banks,
    })
}
/// Explicit board-selection diagnostic, not an implicit fallback from bad GETs.
fn capture_selected(t: &Transport) -> Result<Capture> {
    let initial = t.full_snapshot()?;
    crate::mchose_backup::validate_restore_shape(&initial)?;
    let original = initial.state.profile;
    let keys = mhub_transaction::read_keys(t, original)?;
    let mut banks: Banks = std::array::from_fn(|_| std::array::from_fn(|_| Vec::new()));
    banks[original as usize] = [initial.state.raw_x.clone(), initial.state.raw_y.clone()];
    let mut expected = initial.state.clone();
    let mut last_requested = None;
    let operation = (|| -> Result<Capture> {
        let sequence = (0..3u8)
            .filter(|p| *p != original)
            .chain(std::iter::once(original));
        for board in sequence {
            t.identity()?;
            t.require_board(expected.profile)?;
            if t.hardware_state()? != expected {
                return Err(fail("切组采集期间配置改变；不继续切换板载组"));
            }
            last_requested = Some(board);
            t.request(0x0906, &[board])?;
            let state = t.hardware_state()?;
            if state.profile != board || state.buttons != keys[board as usize] {
                return Err(fail("板载切换或分组按键未确认；停止采集并保留原始备份"));
            }
            banks[board as usize] = [state.raw_x.clone(), state.raw_y.clone()];
            expected = state;
        }
        let final_image = t.full_snapshot()?;
        let after_keys = mhub_transaction::read_keys(t, original)?;
        if final_image != initial || after_keys != keys {
            return Err(fail(
                "返回原组后的配置或宏不一致；保留备份，不继续 DPI 写入",
            ));
        }
        Ok(Capture {
            snapshot: final_image,
            keys: keys.clone(),
            banks,
        })
    })();
    if let Err(reason) = operation {
        // Only undo the selector we just sent; never replay any group parameters.
        let cleanup = (|| -> Result<()> {
            t.identity()?;
            let current = t.settings()?[0];
            if current != original {
                if Some(current) != last_requested {
                    return Err(fail("板载组被其他操作改变，不覆盖新的选择"));
                }
                t.request(0x0906, &[original])?;
                t.require_board(original)?;
            }
            if t.full_snapshot()? != initial || mhub_transaction::read_keys(t, original)? != keys {
                return Err(fail("原组内容发生变化，未覆盖配置"));
            }
            Ok(())
        })();
        return Err(fail(format!(
            "{reason}；原组选择恢复：{}",
            match cleanup {
                Ok(()) => "已确认".into(),
                Err(e) => e.to_string(),
            }
        )));
    }
    operation
}
pub(crate) fn trial(before: &Snapshot) -> Result<Snapshot> {
    crate::mchose_backup::validate_restore_shape(before)?;
    let mut desired = before.clone();
    let s = &mut desired.state;
    let at = s.active as usize;
    let original = s.dpi_x[at];
    s.dpi_x[at] = if original < 50000 {
        original + 50
    } else {
        original - 50
    };
    s.raw_x[5 + at * 2..7 + at * 2].copy_from_slice(&s.dpi_x[at].to_le_bytes());
    // Preserve independent Y; linked observed firmware mirrors the X table.
    if mirrored_linked_axes(s) && !s.independent {
        s.dpi_y = s.dpi_x.clone();
        s.raw_y[5..].copy_from_slice(&s.raw_x[5..]);
    }
    crate::mchose_backup::validate_restore_shape(&desired)?;
    Ok(desired)
}
/// Restoration authority is limited to images produced by this single DPI SET.
/// An unrelated writer, board/identity change or changed macro/key/bank denies it.
pub(crate) fn owned(before: &Capture, desired: &Snapshot, actual: &Capture) -> Result<()> {
    let b = &before.snapshot;
    let a = &actual.snapshot;
    let p = b.state.profile as usize;
    if a.model != b.model
        || a.format != b.format
        || a.state.profile != b.state.profile
        || actual.keys != before.keys
        || a.macros != b.macros
    {
        return Err(fail("诊断期间目标、按键或宏已改变；保留备份，不自动恢复"));
    }
    for board in 0..3 {
        if board != p && actual.banks[board] != before.banks[board] {
            return Err(fail("其他板载组 DPI 已改变；保留备份，不自动恢复"));
        }
    }
    if a.state.raw_x != b.state.raw_x && a.state.raw_x != desired.state.raw_x {
        return Err(fail("当前 DPI 不是本次诊断原值或测试值；不覆盖后续修改"));
    }
    if a.state.raw_y != b.state.raw_y && a.state.raw_y != desired.state.raw_y {
        return Err(fail("Y 表不是原值或已确认的联动镜像；不覆盖后续修改"));
    }
    let mut normalized = a.state.clone();
    normalized.raw_x = b.state.raw_x.clone();
    normalized.dpi_x = b.state.dpi_x.clone();
    normalized.raw_y = b.state.raw_y.clone();
    normalized.dpi_y = b.state.dpi_y.clone();
    for at in [1, 2] {
        let old = b.state.raw_settings[at] & 15;
        let new = a.state.raw_settings[at] & 15;
        if new != old && new != b.state.active {
            return Err(fail("DPI 头部索引不属于本次写入；不自动恢复"));
        }
        normalized.raw_settings[at] = (a.state.raw_settings[at] & 0xf0) | old;
    }
    if normalized != b.state {
        return Err(fail("DPI 以外的配置已改变；保留备份，不自动恢复"));
    }
    Ok(())
}
impl Transport {
    pub fn hardware_dpi_roundtrip(&self, directory: &Path) -> Result<serde_json::Value> {
        self.hardware_dpi_roundtrip_impl(directory, false)
    }
    pub fn hardware_dpi_roundtrip_with_board_reads(
        &self,
        directory: &Path,
    ) -> Result<serde_json::Value> {
        self.hardware_dpi_roundtrip_impl(directory, true)
    }
    fn hardware_dpi_roundtrip_impl(
        &self,
        directory: &Path,
        select_boards: bool,
    ) -> Result<serde_json::Value> {
        // Do not reuse a prior receipt, even when the previous observer timed out.
        std::fs::create_dir(directory).map_err(|e| fail(format!("诊断目录必须是新目录：{e}")))?;
        let take = || {
            if select_boards {
                capture_selected(self)
            } else {
                capture(self)
            }
        };
        // A board-selection mode needs an immutable current-group backup before
        // its first selection SET, as well as the later complete DPI receipt.
        let mut selection_before = None;
        if select_boards {
            let current = self.full_snapshot()?;
            crate::mchose_backup::validate_restore_shape(&current)?;
            let backup = self.persist_snapshot(&current)?;
            write_new(
                &directory.join("before-selection.json"),
                &serde_json::json!({"snapshot":current,"original_backup":backup}),
            )?;
            selection_before = Some(current);
        }
        let before = match take() {
            Ok(value) => value,
            Err(error) => {
                if select_boards {
                    let restored = selection_before.as_ref().is_some_and(|original| {
                        self.full_snapshot().is_ok_and(|actual| actual == *original)
                    });
                    let result = serde_json::json!({"passed":false,"restored":restored,"error":error.to_string(),"manual_review_required":!restored,"phase":"board-inventory","dpi_sets_started":false,"receipt_directory":directory});
                    write_new(&directory.join("result.json"), &result)?;
                    return Ok(result);
                } else {
                    return Err(error);
                }
            }
        };
        let desired = trial(&before.snapshot)?;
        let board = before.snapshot.state.profile;
        let compiled = crate::mchose_backup::compile(&desired, &before.snapshot, &before.keys)?;
        if !compiled.macro_writes.is_empty() || compiled.snapshot != desired {
            return Err(fail("测试计划改变了 DPI 以外的内容；未开始写入"));
        }
        let backup = self.persist_snapshot(&before.snapshot)?;
        write_new(
            &directory.join("before.json"),
            &serde_json::json!({"diagnostic":"mchose-active-x-dpi-roundtrip-v1","backup":backup,"before":before,"desired":desired}),
        )?;
        let outcome = (|| -> Result<serde_json::Value> {
            let applied = mhub_transaction::commit_compiled(
                self,
                board,
                before.snapshot.clone(),
                before.keys.clone(),
                compiled,
                true,
            );
            let observed = take()?;
            write_new(&directory.join("observed.json"), &observed)?;
            owned(&before, &desired, &observed)?;
            let trial_passed = applied.is_ok() && observed.snapshot == desired;
            let restore = crate::mchose_backup::compile(
                &before.snapshot,
                &observed.snapshot,
                &observed.keys,
            )?;
            if !restore.macro_writes.is_empty() || restore.snapshot != before.snapshot {
                return Err(fail("恢复计划改变了原始镜像；不开始恢复"));
            }
            mhub_transaction::commit_compiled(
                self,
                board,
                observed.snapshot,
                observed.keys,
                restore,
                true,
            )?;
            let after = take()?;
            write_new(&directory.join("after.json"), &after)?;
            if after != before {
                return Err(fail(
                    "最终完整配置、三组按键/DPI 或宏未恢复；请检查原始备份",
                ));
            }
            Ok(
                serde_json::json!({"passed":trial_passed,"restored":true,"trial_error":applied.err().map(|e|e.to_string()),"board":board,"active":before.snapshot.state.active,"original_dpi":before.snapshot.state.dpi_x[before.snapshot.state.active as usize],"tested_dpi":desired.state.dpi_x[desired.state.active as usize],"checked_dpi_banks":3,"checked_key_banks":3,"checked_macro_slots":18}),
            )
        })();
        let mut result = match outcome {
            Ok(value) => value,
            Err(error) => {
                serde_json::json!({"passed":false,"restored":false,"error":error.to_string(),"manual_review_required":true})
            }
        };
        result["explicit_board_selection_reads"] = serde_json::json!(select_boards);
        result["original_backup"] = serde_json::json!(backup);
        result["receipt_directory"] = serde_json::json!(directory);
        write_new(&directory.join("result.json"), &result)
            .map_err(|e| fail(format!("{e}；原始备份：{}", backup.display())))?;
        Ok(result)
    }
}
