//! Original production restore/compiler/transport, authored memory device only.
use super::*;
use crate::mchose_backup as backup;
struct Fixture {
    rig: Rig,
    saved: Snapshot,
    path: std::path::PathBuf,
    device: crate::device::Device,
    target: crate::mchose_target::Target,
}
impl Fixture {
    fn new() -> Self {
        let rig = Rig::new();
        let before = rig.t.full_snapshot().unwrap();
        let keys = crate::mhub_transaction::read_keys(&rig.t, 0).unwrap();
        let mut saved = compile(&plan(), &before, &keys).unwrap().snapshot;
        let events = saved.macros[0].events.clone();
        saved
            .macros
            .push(native::macro_record(7, "Backup unrelated", 2, &events, None).unwrap());
        {
            let mut m = rig.mouse.lock().unwrap();
            m.macros.insert(
                0,
                native::macro_record(0, "Other board", 2, &events, None)
                    .unwrap()
                    .raw,
            );
            m.macros.insert(
                7,
                native::macro_record(7, "Current unrelated", 2, &events, None)
                    .unwrap()
                    .raw,
            );
            m.other_keys[0][4] = vec![0x21, 0, 0];
        }
        let device = crate::mchose_target::simulated_device("backup-A");
        let target = crate::mchose_target::read_context(&device, &device.connection_key).unwrap();
        let path = rig.directory.with_extension("selected-backup.json");
        let f = Self {
            rig,
            saved,
            path,
            device,
            target,
        };
        f.save();
        f
    }
    fn save(&self) {
        std::fs::write(&self.path, serde_json::to_vec_pretty(&self.saved).unwrap()).unwrap();
    }
    fn preview(&self) -> Result<backup::Preview> {
        backup::preview(&self.device, &self.rig.t, &self.path, &self.target, 0)
    }
    fn set_count(&self) -> usize {
        set_records(&self.rig.mouse.lock().unwrap()).len()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
fn tables(s: &mut native::HardwareState) {
    for (axis, values) in [(0, &s.dpi_x), (1, &s.dpi_y)] {
        let mut raw = vec![s.profile, axis, s.count, s.active, s.independent as u8];
        for v in values {
            raw.extend_from_slice(&v.to_le_bytes());
        }
        if axis == 0 {
            s.raw_x = raw
        } else {
            s.raw_y = raw
        }
    }
}
#[test]
fn restore_preview_is_read_only_and_commit_remaps_shared_slots_without_touching_other_boards_or_unrelated_library(
) {
    let f = Fixture::new();
    let other = Rig::new();
    let old = f.rig.mouse.lock().unwrap().macros.clone();
    let other_keys = f.rig.mouse.lock().unwrap().other_keys.clone();
    let p = f.preview().unwrap();
    assert_eq!(p.slot_map[&0], 1);
    assert_eq!(p.macros.len(), 1);
    assert!(p.macros[0].upload);
    assert_eq!(p.unrelated_backup_macros, 1);
    assert_eq!(p.macros[0].events, f.saved.macros[0].events);
    assert_eq!(f.set_count(), 0);
    assert!(!f.rig.directory.exists());
    assert!(p.current == f.rig.t.hardware_state().unwrap());
    let result = backup::restore(&f.device, &f.rig.t, &f.path, &p.selection).unwrap();
    assert_eq!(result.slot_map[&0], 1);
    assert_eq!(result.uploaded_macros, vec![1]);
    assert!(result.backup.is_some());
    assert!(result.state == f.rig.t.hardware_state().unwrap());
    assert_eq!(result.state.buttons[4], vec![0x21, 1, 0]);
    let m = f.rig.mouse.lock().unwrap();
    assert_eq!(m.other_keys, other_keys);
    for (slot, raw) in &old {
        assert_eq!(m.macros.get(slot), Some(raw));
    }
    assert_eq!(m.macros[&1], f.saved.macros[0].raw);
    assert!(other.mouse.lock().unwrap().requests.is_empty());
    drop(m);
    let image: Snapshot =
        serde_json::from_slice(&std::fs::read(result.backup.unwrap()).unwrap()).unwrap();
    assert_eq!(image.macros.len(), 2);
    for record in &image.macros {
        assert_eq!(old[&record.slot], record.raw);
    }
    let count = f.set_count();
    let p = f.preview().unwrap();
    let again = backup::restore(&f.device, &f.rig.t, &f.path, &p.selection).unwrap();
    assert!(again.backup.is_none());
    assert!(again.uploaded_macros.is_empty());
    assert_eq!(f.set_count(), count);
    assert_eq!(std::fs::read_dir(&f.rig.directory).unwrap().count(), 1);
}
#[test]
fn restore_file_change_board_owner_or_any_state_change_invalidates_preview_before_backup_or_set() {
    for case in 0..6 {
        let mut f = Fixture::new();
        let mut p = f.preview().unwrap();
        match case {
            0 => {
                let mut bytes = std::fs::read(&f.path).unwrap();
                bytes.push(b' ');
                std::fs::write(&f.path, bytes).unwrap();
            }
            1 => f.device.connection_key = "connection:backup-A:2".into(),
            2 => {
                f.rig.mouse.lock().unwrap().state.raw_settings[3] = 4;
            }
            3 => {
                f.rig.mouse.lock().unwrap().other_keys[0][3] = vec![0, 8, 0];
            }
            4 => {
                f.rig.mouse.lock().unwrap().macros.remove(&7);
            }
            _ => p.selection.board = 2,
        }
        assert!(backup::restore(&f.device, &f.rig.t, &f.path, &p.selection).is_err());
        assert_eq!(f.set_count(), 0);
        assert!(!f.rig.directory.exists());
    }
}
#[test]
fn restore_loader_rejects_duplicates_unknown_fields_model_and_incomplete_macro_body() {
    for case in 0..8 {
        let mut f = Fixture::new();
        match case {
            0 => {
                let s = String::from_utf8(std::fs::read(&f.path).unwrap()).unwrap();
                let s = s.replacen("{", "{\"model\":\"3837:4026\",", 1);
                std::fs::write(&f.path, s).unwrap();
            }
            1 => {
                let mut v = serde_json::to_value(&f.saved).unwrap();
                v["state"]["future_setting"] = json!(true);
                std::fs::write(&f.path, serde_json::to_vec(&v).unwrap()).unwrap();
            }
            2 => {
                f.saved.model = "1532:00b6".into();
                f.save();
            }
            3 => {
                f.saved.macros.remove(0);
                f.save();
            }
            4 => {
                f.saved.state.receiver_firmware.push('9');
                f.save();
            }
            5 => {
                f.saved.state.raw_settings[18] ^= 1;
                f.save();
            }
            6 => {
                f.saved.state.raw_buttons.extend_from_slice(&[0, 1, 2]);
                f.save();
            }
            _ => {
                f.saved.state.raw_settings[1] = (f.saved.state.raw_settings[1] & 0xf0) | 7;
                f.save();
            }
        }
        assert!(f.preview().is_err());
        assert_eq!(f.set_count(), 0);
        assert!(!f.rig.directory.exists());
    }
}
#[test]
fn restore_retains_inactive_y_and_separate_link_header_indices_after_dpi_updates_headers() {
    let mut f = Fixture::new();
    f.saved.state.independent = false;
    f.saved.state.active = 2;
    f.saved.state.dpi_y = vec![450, 950, 1850, 3650, 7250, 14450];
    tables(&mut f.saved.state);
    f.saved.state.raw_settings[1] = (f.saved.state.raw_settings[1] & 0xf0) | 1;
    f.saved.state.raw_settings[2] = (f.saved.state.raw_settings[2] & 0xf0) | 4;
    f.save();
    f.rig.mouse.lock().unwrap().fault = Some(Fault::DpiUpdatesHeaders);
    let p = f.preview().unwrap();
    assert_eq!(p.desired.dpi_y, f.saved.state.dpi_y);
    let result = backup::restore(&f.device, &f.rig.t, &f.path, &p.selection).unwrap();
    assert_eq!(result.state.dpi_y, f.saved.state.dpi_y);
    assert!(!result.state.independent);
    assert_eq!(result.state.active, 2);
    assert_eq!(
        (
            result.state.raw_settings[1] & 15,
            result.state.raw_settings[2] & 15
        ),
        (1, 4)
    );
    let sets = set_records(&f.rig.mouse.lock().unwrap());
    let dpi = sets.iter().rposition(|(c, _)| *c == 0x0103).unwrap();
    let header = sets.iter().position(|(c, _)| *c == 0x0102).unwrap();
    assert!(header > dpi);
}
#[test]
fn unknown_other_board_action_prevents_new_macro_allocation_but_identical_reuse_is_allowed() {
    let f = Fixture::new();
    f.rig.mouse.lock().unwrap().other_keys[1][5] = vec![0x22, 0, 0, 0];
    assert!(f.preview().is_err());
    assert_eq!(f.set_count(), 0);
    assert!(!f.rig.directory.exists());
    f.rig
        .mouse
        .lock()
        .unwrap()
        .macros
        .insert(4, f.saved.macros[0].raw.clone());
    let p = f.preview().unwrap();
    assert_eq!(p.slot_map[&0], 4);
    assert!(!p.macros[0].upload);
}
#[test]
fn restore_partial_write_keeps_before_image_and_never_retries_or_emits_success() {
    let f = Fixture::new();
    let p = f.preview().unwrap();
    f.rig.mouse.lock().unwrap().fault = Some(Fault::IgnoreBinding);
    let e = backup::restore(&f.device, &f.rig.t, &f.path, &p.selection)
        .err()
        .unwrap()
        .to_string();
    assert!(e.contains("可能已有部分设置生效"));
    assert!(e.contains("原始备份"));
    let sets = set_records(&f.rig.mouse.lock().unwrap());
    assert_eq!(sets.iter().filter(|(c, _)| *c == 0x0101).count(), 1);
    assert!(!sets.iter().any(|(c, _)| *c == 0x0910));
    assert_eq!(std::fs::read_dir(&f.rig.directory).unwrap().count(), 1);
}
#[test]
fn restore_file_bounds_allow_large_complete_json_but_refuse_over_limit_before_device_reads() {
    let f = Fixture::new();
    let mut bytes = std::fs::read(&f.path).unwrap();
    bytes.resize(1024 * 1024 + 1, b' ');
    std::fs::write(&f.path, &bytes).unwrap();
    assert!(f.preview().is_ok());
    let count = f.rig.mouse.lock().unwrap().requests.len();
    bytes.resize(8 * 1024 * 1024 + 1, b' ');
    std::fs::write(&f.path, bytes).unwrap();
    assert!(f.preview().is_err());
    assert_eq!(f.rig.mouse.lock().unwrap().requests.len(), count);
    assert_eq!(f.set_count(), 0);
}

#[test]
#[ignore = "requires explicit built backup UI bridge request/response paths"]
fn mchose_backup_ui_bridge() {
    let request: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("OPEN_HUB_MCHOSE_BACKUP_UI_REQUEST").unwrap()).unwrap(),
    )
    .unwrap();
    let f = Fixture::new();
    let operation = request["operation"].as_str().unwrap();
    let args = &request["args"];
    let value: Result<Value> = (|| {
        if operation == "hardware" {
            return serde_json::to_value(crate::mchose_target::read_hardware(
                &f.device,
                &f.rig.t,
                &f.device.connection_key,
            )?)
            .map_err(|e| Error::Other(e.to_string()));
        }
        assert_eq!(args["id"], f.device.id);
        if operation == "preview" {
            let target: crate::mchose_target::Target =
                serde_json::from_value(args["target"].clone()).unwrap();
            return serde_json::to_value(backup::preview(
                &f.device,
                &f.rig.t,
                &f.path,
                &target,
                args["expectedProfile"].as_u64().unwrap() as u8,
            )?)
            .map_err(|e| Error::Other(e.to_string()));
        }
        assert_eq!(operation, "restore");
        let selection: backup::Selection =
            serde_json::from_value(args["selection"].clone()).unwrap();
        match request["scenario"].as_str().unwrap_or("success") {
            "file_changed" => {
                let mut b = std::fs::read(&f.path).unwrap();
                b.push(b' ');
                std::fs::write(&f.path, b).unwrap();
            }
            "state_changed" => f.rig.mouse.lock().unwrap().state.raw_settings[3] = 4,
            "partial" => f.rig.mouse.lock().unwrap().fault = Some(Fault::IgnoreBinding),
            "success" => {}
            _ => panic!("unknown authored UI scenario"),
        }
        let applied = backup::restore(&f.device, &f.rig.t, &f.path, &selection)?;
        serde_json::to_value(crate::profiles::MchoseReceipt {
            target: selection.target,
            device_name: f.device.name.clone(),
            applied,
        })
        .map_err(|e| Error::Other(e.to_string()))
    })();
    let m = f.rig.mouse.lock().unwrap();
    assert_eq!(
        m.macros[&0],
        native::macro_record(0, "Other board", 2, &f.saved.macros[0].events, None)
            .unwrap()
            .raw
    );
    assert_eq!(
        m.macros[&7],
        native::macro_record(7, "Current unrelated", 2, &f.saved.macros[0].events, None)
            .unwrap()
            .raw
    );
    assert_eq!(m.other_keys[0][4], vec![0x21, 0, 0]);
    let slots = m.macros.keys().copied().collect::<Vec<_>>();
    let set_count = set_records(&m).len();
    drop(m);
    let macros = slots
        .into_iter()
        .map(|slot| f.rig.t.read_macro(slot).unwrap().unwrap())
        .collect::<Vec<_>>();
    let audit = json!({"settings_and_macro_sets":set_count,"has_before_image":f.rig.directory.exists(),"other_board_and_unrelated_macros_preserved":true,"macros":macros});
    let response = match value {
        Ok(value) => json!({"ok":true,"value":value,"audit":audit}),
        Err(e) => json!({"ok":false,"error":e.to_string(),"audit":audit}),
    };
    std::fs::write(
        std::env::var("OPEN_HUB_MCHOSE_BACKUP_UI_RESPONSE").unwrap(),
        serde_json::to_vec_pretty(&response).unwrap(),
    )
    .unwrap();
}
