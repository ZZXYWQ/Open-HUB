//! Production diagnostic over three independently populated memory boards.
use super::*;
use crate::mchose_config::roundtrip::{capture, owned, trial};
fn dir(r: &Rig, name: &str) -> std::path::PathBuf {
    r.backup.with_extension(name)
}
fn clean(path: &std::path::Path) {
    for f in std::fs::read_dir(path).unwrap().flatten() {
        std::fs::remove_file(f.path()).unwrap();
    }
    std::fs::remove_dir(path).unwrap();
}
#[test]
fn active_dpi_roundtrip_restores_exact_three_board_images_and_other_mouse() {
    let a = Rig::new(None);
    let b = Rig::new(None);
    let before = capture(&a.transport).unwrap();
    let path = dir(&a, "roundtrip");
    let report = a.transport.hardware_dpi_roundtrip(&path).unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(report["restored"], true);
    assert!(capture(&a.transport).unwrap() == before);
    let m = a.mouse.lock().unwrap();
    assert!(m
        .writes
        .iter()
        .all(|(c, p)| [0x0103, 0x0102].contains(c) && p[0] == 0));
    assert_eq!(m.writes.iter().filter(|(c, _)| *c == 0x0103).count(), 2);
    assert!(b.mouse.lock().unwrap().requests.is_empty());
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.join("before.json")).unwrap()).unwrap();
    assert_eq!(
        saved["before"]["snapshot"]["state"]["dpi_x"],
        serde_json::json!(before.snapshot.state.dpi_x)
    );
    assert!(std::path::Path::new(report["original_backup"].as_str().unwrap()).exists());
    drop(m);
    clean(&path);
}
#[test]
fn upper_bound_dpi_trial_subtracts_fifty_and_tracks_linked_mirror() {
    let a = Rig::new(None);
    let mut before = capture(&a.transport).unwrap().snapshot;
    let at = before.state.active as usize;
    before.state.dpi_x[at] = 50000;
    before.state.raw_x[5 + at * 2..7 + at * 2].copy_from_slice(&50000u16.to_le_bytes());
    before.state.dpi_y = before.state.dpi_x.clone();
    before.state.raw_y[5..].copy_from_slice(&before.state.raw_x[5..]);
    let next = trial(&before).unwrap();
    assert_eq!(next.state.dpi_x[at], 49950);
    assert_eq!(next.state.dpi_y, next.state.dpi_x);
    assert_eq!(next.state.raw_settings, before.state.raw_settings);
}
#[test]
fn unchanged_or_partial_owned_dpi_can_restore_but_external_changes_cannot() {
    let a = Rig::new(None);
    let before = capture(&a.transport).unwrap();
    let desired = trial(&before.snapshot).unwrap();
    assert!(owned(&before, &desired, &before).is_ok());
    let mut actual = before.clone();
    actual.snapshot.state = desired.state.clone();
    actual.banks[0][0] = desired.state.raw_x.clone();
    assert!(owned(&before, &desired, &actual).is_ok());
    for case in 0..6 {
        let mut changed = actual.clone();
        match case {
            0 => changed.snapshot.state.raw_settings[3] ^= 1,
            1 => changed.snapshot.state.raw_y[5] ^= 1,
            2 => changed.keys[1][4] = vec![0, 8, 0],
            3 => changed.banks[2][0][5] ^= 1,
            4 => changed.snapshot.state.raw_x[5] ^= 1,
            _ => changed.snapshot.state.names[1] = "another writer".into(),
        }
        assert!(owned(&before, &desired, &changed).is_err(), "case {case}");
    }
}
#[test]
fn board_switch_after_first_set_stops_without_retargeting_restore() {
    let a = Rig::new(Some(Switch::AfterFirstDpiSet));
    let path = dir(&a, "switched");
    let report = a.transport.hardware_dpi_roundtrip(&path).unwrap();
    assert_eq!(report["passed"], false);
    assert_eq!(report["restored"], false);
    let m = a.mouse.lock().unwrap();
    assert_eq!(m.active, 1);
    assert_eq!(m.writes.len(), 1);
    assert_eq!(m.writes[0].0, 0x0103);
    assert_eq!(m.writes[0].1[0], 0);
    assert!(path.join("before.json").exists());
    assert!(path.join("result.json").exists());
    drop(m);
    clean(&path);
}
#[test]
fn existing_receipt_directory_refuses_all_io_and_never_overwrites_file() {
    let a = Rig::new(None);
    let path = dir(&a, "existing");
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("user.txt"), "untouched").unwrap();
    assert!(a.transport.hardware_dpi_roundtrip(&path).is_err());
    assert!(a.mouse.lock().unwrap().requests.is_empty());
    assert_eq!(
        std::fs::read_to_string(path.join("user.txt")).unwrap(),
        "untouched"
    );
    clean(&path);
}

#[test]
fn failed_durable_backup_sends_no_set() {
    let a = Rig::new(None);
    std::fs::write(&a.backup, "blocked backup location").unwrap();
    let path = dir(&a, "backup-blocked");
    assert!(a.transport.hardware_dpi_roundtrip(&path).is_err());
    assert!(a.mouse.lock().unwrap().writes.is_empty());
    assert!(!path.join("before.json").exists());
    assert_eq!(
        std::fs::read_to_string(&a.backup).unwrap(),
        "blocked backup location"
    );
    std::fs::remove_file(&a.backup).unwrap();
    clean(&path);
}
#[test]
fn temporary_readback_corruption_is_reported_as_failure_even_after_owned_restore() {
    let a = Rig::new(None);
    let source = a.mouse.clone();
    let used = std::sync::atomic::AtomicBool::new(false);
    let t = Transport::memory(
        move |cmd, args| {
            let mut m = source.lock().unwrap();
            let mut response = m.request(cmd, args)?;
            if cmd == 3
                && args == [0, 0]
                && m.dpi_written
                && !used.swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                response[5] ^= 1;
            }
            Ok(response)
        },
        a.backup.clone(),
    );
    let before = capture(&t).unwrap();
    let path = dir(&a, "bad-readback");
    let report = t.hardware_dpi_roundtrip(&path).unwrap();
    assert_eq!(report["passed"], false);
    assert_eq!(report["restored"], true);
    assert!(report["trial_error"].as_str().unwrap().contains("DPI"));
    assert!(capture(&t).unwrap() == before);
    let m = a.mouse.lock().unwrap();
    assert_eq!(m.writes.iter().filter(|(c, _)| *c == 0x0103).count(), 2);
    assert!(m
        .writes
        .iter()
        .all(|(c, p)| [0x0103, 0x0102].contains(c) && p[0] == 0));
    drop(m);
    clean(&path);
}
#[test]
fn concurrent_settings_change_is_not_overwritten_by_diagnostic_restore() {
    let a = Rig::new(None);
    let source = a.mouse.clone();
    let used = std::sync::atomic::AtomicBool::new(false);
    let t = Transport::memory(
        move |cmd, args| {
            let mut m = source.lock().unwrap();
            let response = m.request(cmd, args)?;
            if cmd == 0x0103
                && args[1] == 0
                && !used.swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                m.boards[0].raw_settings[3] = 9;
                m.boards[0].sleep = 9;
            }
            Ok(response)
        },
        a.backup.clone(),
    );
    let path = dir(&a, "external-settings");
    let report = t.hardware_dpi_roundtrip(&path).unwrap();
    assert_eq!(report["passed"], false);
    assert_eq!(report["restored"], false);
    assert_eq!(report["manual_review_required"], true);
    assert!(report["error"].as_str().unwrap().contains("配置已改变"));
    let m = a.mouse.lock().unwrap();
    assert_eq!(m.boards[0].raw_settings[3], 9);
    assert_eq!(m.writes.len(), 1);
    assert!(m.writes.iter().all(|(c, _)| *c == 0x0103));
    drop(m);
    assert!(path.join("before.json").exists());
    clean(&path);
}

#[test]
fn current_only_dpi_firmware_is_never_mislabeled_as_three_bank_direct_reads() {
    let a = Rig::new(None);
    let source = a.mouse.clone();
    let t = Transport::memory(
        move |cmd, args| {
            let mut m = source.lock().unwrap();
            if cmd == 3 {
                let active = m.active;
                return m.request(cmd, &[active, args[1]]);
            }
            m.request(cmd, args)
        },
        a.backup.clone(),
    );
    let path = dir(&a, "current-only");
    assert!(t.hardware_dpi_roundtrip(&path).is_err());
    assert!(a.mouse.lock().unwrap().writes.is_empty());
    assert!(!path.join("before.json").exists());
    clean(&path);
}
#[test]
fn explicit_selected_bank_reads_support_current_only_firmware_and_restore_original() {
    let a = Rig::new(None);
    let source = a.mouse.clone();
    let t = Transport::memory(
        move |cmd, args| {
            let mut m = source.lock().unwrap();
            if cmd == 3 {
                let active = m.active;
                return m.request(cmd, &[active, args[1]]);
            }
            m.request(cmd, args)
        },
        a.backup.clone(),
    );
    let states = a.mouse.lock().unwrap().boards.clone();
    let path = dir(&a, "selected-boards");
    let report = t.hardware_dpi_roundtrip_with_board_reads(&path).unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(report["restored"], true);
    assert_eq!(report["explicit_board_selection_reads"], true);
    let m = a.mouse.lock().unwrap();
    assert_eq!(m.active, 0);
    assert!(m.boards == states);
    assert_eq!(m.writes.iter().filter(|(c, _)| *c == 0x0906).count(), 9);
    assert!(m
        .writes
        .iter()
        .filter(|(c, _)| *c != 0x0906)
        .all(|(c, p)| [0x0103, 0x0102].contains(c) && p[0] == 0));
    assert!(path.join("before-selection.json").exists());
    drop(m);
    clean(&path);
}

#[test]
fn unsupported_selected_board_reply_returns_to_original_without_dpi_set() {
    let a = Rig::new(None);
    let source = a.mouse.clone();
    let t = Transport::memory(
        move |cmd, args| {
            let mut m = source.lock().unwrap();
            let mut result = m.request(cmd, args)?;
            if cmd == 3 && m.active == 1 {
                result[0] = 0;
            }
            Ok(result)
        },
        a.backup.clone(),
    );
    let initial = a.mouse.lock().unwrap().boards.clone();
    let path = dir(&a, "selected-invalid");
    let report = t.hardware_dpi_roundtrip_with_board_reads(&path).unwrap();
    assert_eq!(report["passed"], false);
    assert_eq!(report["restored"], true);
    assert_eq!(report["dpi_sets_started"], false);
    let m = a.mouse.lock().unwrap();
    assert_eq!(m.active, 0);
    assert!(m.boards == initial);
    assert_eq!(m.writes.len(), 2);
    assert!(m.writes.iter().all(|(cmd, _)| *cmd == 0x0906));
    drop(m);
    clean(&path);
}

#[test]
fn independent_y_is_retained_while_testing_x() {
    let a = Rig::new(None);
    let mut before = capture(&a.transport).unwrap().snapshot;
    before.state.independent = true;
    before.state.raw_x[4] = 1;
    before.state.raw_y[4] = 1;
    let next = trial(&before).unwrap();
    assert_eq!(next.state.dpi_y, before.state.dpi_y);
    assert_eq!(next.state.raw_y, before.state.raw_y);
    assert_eq!(next.state.independent, true);
}
