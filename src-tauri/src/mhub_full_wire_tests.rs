// Full source/editor plans replay production codecs, differential transactions
// and per-device routing over isolated stateful memory transports. No HID I/O.
use super::*;
use crate::{
    hidpp::{Error, Result},
    mchose::Transport,
};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq)]
enum Fault {
    MacroChunk19,
    IgnoreMacro,
    LostMacroAck,
    IgnoreSettings,
    IgnoreBinding,
    IgnoreName,
    SwitchAfterX,
    RefAfterFirstChunk,
    SettingsAfterMacroScan,
    RefDuringRecheck,
}
struct Mouse {
    state: native::HardwareState,
    other_keys: [Vec<Vec<u8>>; 2],
    macros: BTreeMap<u8, Vec<u8>>,
    staged: BTreeMap<u8, Vec<u8>>,
    names: BTreeMap<u8, Vec<u8>>,
    requests: Vec<(u16, Vec<u8>)>,
    fault: Option<Fault>,
}
impl Mouse {
    fn new() -> Self {
        let mut state = baseline().state;
        state.adaptive.clear();
        Self {
            other_keys: [state.buttons.clone(), state.buttons.clone()],
            state,
            macros: BTreeMap::new(),
            staged: BTreeMap::new(),
            names: BTreeMap::new(),
            requests: vec![],
            fault: None,
        }
    }
    fn request(&mut self, cmd: u16, args: &[u8]) -> Result<Vec<u8>> {
        self.requests.push((cmd, args.to_vec()));
        let response = match cmd {
            0x0900 => {
                let mut r = vec![0; 16];
                r[..4].copy_from_slice(&[0x37, 0x38, 0x26, 0x40]);
                r[10] = 2;
                r[12] = 80;
                r
            }
            2 => self.state.raw_settings.clone(),
            3 => {
                if args[1] == 0 {
                    self.state.raw_x.clone()
                } else {
                    self.state.raw_y.clone()
                }
            }
            1 => {
                if args[0] == 0 {
                    self.state.raw_buttons.clone()
                } else {
                    self.other_keys[args[0] as usize - 1].concat()
                }
            }
            9 => vec![self.state.lod],
            0x000a | 0x010a | 0x010b => panic!("A7 has no adaptive rate in this OEM build"),
            0x0901 => vec![0, if args[0] == 0 { 23 } else { 6 }],
            0x0905 => {
                let mut b = [0; 60];
                let name = self.state.names[args[0] as usize].as_bytes();
                b[..name.len()].copy_from_slice(name);
                b[args[1] as usize..args[1] as usize + args[2] as usize].to_vec()
            }
            0x090c => match self.macros.get(&args[0]) {
                None => vec![0],
                Some(raw) => {
                    let at = u16::from_le_bytes([args[1], args[2]]) as usize;
                    let n = args[3] as usize;
                    [vec![args[3]], raw[at..at + n].to_vec()].concat()
                }
            },
            0x090b => {
                let slot = args[0];
                let at = u16::from_le_bytes([args[1], args[2]]) as usize;
                let n = (args[3] >> 2) as usize;
                assert_eq!(n, args.len() - 4);
                assert!(n <= 19);
                if self.fault == Some(Fault::MacroChunk19) && at == 19 {
                    self.fault = None;
                    return Err(Error::Other("authored macro chunk failure".into()));
                }
                if args[3] & 3 == 1 {
                    assert_eq!(at, 0);
                    self.staged.insert(slot, vec![]);
                }
                let data = self.staged.get_mut(&slot).unwrap();
                assert_eq!(data.len(), at);
                data.extend_from_slice(&args[4..]);
                if args[3] & 3 == 2 {
                    let raw = self.staged.remove(&slot).unwrap();
                    assert_eq!(
                        raw.len(),
                        64 + u16::from_le_bytes([raw[0], raw[1]]) as usize
                    );
                    if self.fault == Some(Fault::IgnoreMacro) {
                        self.fault = None;
                    } else {
                        self.macros.insert(slot, raw);
                    }
                    if self.fault == Some(Fault::LostMacroAck) {
                        self.fault = None;
                        return Err(Error::Other("authored lost commit acknowledgment".into()));
                    }
                }
                vec![]
            }
            0x0103 => {
                assert_eq!(args[0], 0);
                let table = [
                    vec![0, args[1], args[3], args[4], args[2]],
                    args[5..].to_vec(),
                ]
                .concat();
                if args[1] == 0 {
                    self.state.raw_x = table
                } else {
                    self.state.raw_y = table
                }
                vec![]
            }
            0x0102 => {
                assert_eq!(args[0], 0);
                assert_eq!(args.len(), 19);
                if self.fault == Some(Fault::IgnoreSettings) {
                    self.fault = None;
                } else {
                    self.state.raw_settings = args.to_vec();
                }
                vec![]
            }
            0x0101 => {
                assert_eq!(args[0], 0);
                assert_eq!(args[2], 11);
                if self.fault == Some(Fault::IgnoreBinding) {
                    self.fault = None;
                } else {
                    self.state.buttons[args[1] as usize] = args[3..].to_vec();
                }
                self.state.raw_buttons = self.state.buttons.concat();
                assert!(
                    self.state.buttons.iter().any(|b| b == &[0, 1, 0]),
                    "native write removed the last ordinary left button"
                );
                vec![]
            }
            0x0109 => {
                assert_eq!(args[0], 0);
                self.state.lod = args[1];
                vec![]
            }
            0x0910 => {
                let p = args[0];
                let at = args[1] as usize;
                let n = (args[2] >> 2) as usize;
                assert_eq!(n, args.len() - 3);
                if args[2] & 1 != 0 {
                    self.names.insert(p, vec![0; 60]);
                }
                self.names.get_mut(&p).unwrap()[at..at + n].copy_from_slice(&args[3..]);
                if args[2] & 2 != 0 {
                    if self.fault == Some(Fault::IgnoreName) {
                        self.fault = None;
                        self.names.remove(&p);
                        return Ok(vec![]);
                    }
                    self.state.names[p as usize] =
                        String::from_utf8(self.names.remove(&p).unwrap())
                            .unwrap()
                            .trim_end_matches('\0')
                            .into();
                }
                vec![]
            }
            _ => {
                return Err(Error::Other(format!(
                    "unexpected isolated M HUB replay command {cmd:04x}"
                )))
            }
        };
        if self.fault == Some(Fault::SwitchAfterX) && cmd == 0x0103 && args[1] == 0 {
            self.fault = None;
            self.state.raw_settings[0] = 1;
        }
        if self.fault == Some(Fault::RefAfterFirstChunk) && cmd == 0x090b && args[1..3] == [0, 0] {
            self.fault = None;
            self.other_keys[0][4] = vec![0x21, args[0], 0];
        }
        if self.fault == Some(Fault::SettingsAfterMacroScan) && cmd == 0x090c && args[0] == 17 {
            self.fault = None;
            self.state.raw_settings[3] = 4;
        }
        if self.fault == Some(Fault::RefDuringRecheck)
            && cmd == 1
            && args[0] == 1
            && self
                .requests
                .iter()
                .filter(|(cmd, args)| *cmd == 1 && args[0] == 1)
                .count()
                == 2
        {
            self.fault = None;
            self.other_keys[0][4] = vec![0x21, 0, 0];
        }
        Ok(response)
    }
}
struct Rig {
    t: Transport,
    mouse: Arc<Mutex<Mouse>>,
    directory: std::path::PathBuf,
}
impl Rig {
    fn new() -> Self {
        let mouse = Arc::new(Mutex::new(Mouse::new()));
        let source = mouse.clone();
        let directory =
            std::env::temp_dir().join(format!("open-hub-mhub-wire-{}", crate::profiles::new_id()));
        let t = Transport::memory(
            move |cmd, args| source.lock().unwrap().request(cmd, args),
            directory.clone(),
        );
        Self {
            t,
            mouse,
            directory,
        }
    }
}
impl Drop for Rig {
    fn drop(&mut self) {
        if let Ok(files) = std::fs::read_dir(&self.directory) {
            for file in files.flatten() {
                let _ = std::fs::remove_file(file.path());
            }
            let _ = std::fs::remove_dir(&self.directory);
        }
    }
}
#[test]
fn converted_known_template_replays_through_production_restore_codec_on_one_target() {
    let plan = plan();
    let a = Rig::new();
    let b = Rig::new();
    let before = a.t.full_snapshot().unwrap();
    let all = boards(&before);
    let compiled = compile(&plan, &before, &all).unwrap();
    let expected = compiled.snapshot.state.clone();
    let macro_bytes = compiled.macro_writes[0].raw.clone();
    let device = crate::mchose_target::simulated_device("mchose-A");
    let target = crate::mchose_target::read_context(&device, &device.connection_key).unwrap();
    let actual = crate::mchose_target::with_target(&device, &a.t, &target, |t| {
        t.restore_snapshot(compiled.snapshot)
    })
    .unwrap();
    assert!(actual == expected);
    let mouse = a.mouse.lock().unwrap();
    assert_eq!(mouse.macros.get(&0), Some(&macro_bytes));
    assert_eq!(
        mouse.other_keys,
        [before.state.buttons.clone(), before.state.buttons.clone()]
    );
    let settings = mouse
        .requests
        .iter()
        .filter(|(cmd, _)| *cmd == 0x0102)
        .collect::<Vec<_>>();
    assert_eq!(settings.len(), 1);
    assert_eq!(settings[0].1, expected.raw_settings);
    let chunks = mouse
        .requests
        .iter()
        .filter(|(cmd, _)| *cmd == 0x090b)
        .map(|(_, args)| args)
        .collect::<Vec<_>>();
    assert_eq!(
        chunks
            .iter()
            .map(|a| u16::from_le_bytes([a[1], a[2]]))
            .collect::<Vec<_>>(),
        [0, 19, 38, 57, 76, 95]
    );
    assert_eq!(chunks.last().unwrap()[3], 6);
    assert!(mouse
        .requests
        .iter()
        .filter(|(cmd, _)| [0x0101, 0x0102, 0x0103, 0x0109, 0x0910].contains(cmd))
        .all(|(_, args)| args[0] == 0));
    assert!(b.mouse.lock().unwrap().requests.is_empty());
    drop(mouse);
    let files = std::fs::read_dir(&a.directory).unwrap().collect::<Vec<_>>();
    assert_eq!(files.len(), 1);
    let backup: Snapshot =
        serde_json::from_slice(&std::fs::read(files[0].as_ref().unwrap().path()).unwrap()).unwrap();
    assert!(backup.state == before.state);
    assert!(backup.macros.is_empty());
}
#[test]
fn wrong_target_cannot_replay_a_compiled_template_or_create_backup() {
    let a = Rig::new();
    let device = crate::mchose_target::simulated_device("mchose-A");
    let target = crate::mchose_target::read_context(&device, &device.connection_key).unwrap();
    let other = crate::mchose_target::simulated_device("mchose-B");
    let before = baseline();
    let compiled = compile(&plan(), &before, &boards(&before)).unwrap();
    assert!(
        crate::mchose_target::with_target(&other, &a.t, &target, |t| t
            .restore_snapshot(compiled.snapshot))
        .is_err()
    );
    assert!(a.mouse.lock().unwrap().requests.is_empty());
    assert!(!a.directory.exists());
}

fn transact(rig: &Rig, plan: &Plan) -> Result<crate::mhub_transaction::Applied> {
    let device = crate::mchose_target::simulated_device("mchose-A");
    let target = crate::mchose_target::read_context(&device, &device.connection_key).unwrap();
    crate::mhub_transaction::apply(&device, &rig.t, &target, plan.board, plan)
}
fn set_records(mouse: &Mouse) -> Vec<(u16, Vec<u8>)> {
    mouse
        .requests
        .iter()
        .filter(|(cmd, _)| [0x0101, 0x0102, 0x0103, 0x0109, 0x090b, 0x0910].contains(cmd))
        .cloned()
        .collect()
}
fn transaction_error(rig: &Rig, p: &Plan) -> String {
    match transact(rig, p) {
        Ok(_) => panic!("fault injection unexpectedly succeeded"),
        Err(e) => e.to_string(),
    }
}
fn backups(rig: &Rig) -> Vec<Snapshot> {
    std::fs::read_dir(&rig.directory)
        .unwrap()
        .map(|e| serde_json::from_slice(&std::fs::read(e.unwrap().path()).unwrap()).unwrap())
        .collect()
}
#[test]
fn dedicated_transaction_uploads_only_delta_and_reapply_has_no_set_or_new_backup() {
    let a = Rig::new();
    let b = Rig::new();
    let p = plan();
    let before = a.t.full_snapshot().unwrap();
    let expected = compile(&p, &before, &boards(&before)).unwrap();
    let result = transact(&a, &p).unwrap();
    assert!(result.state == expected.snapshot.state);
    assert_eq!(result.uploaded_macros, [0]);
    assert!(result.backup.is_some());
    assert_eq!(result.slot_map[&10], 0);
    assert_eq!(backups(&a).len(), 1);
    assert!(backups(&a)[0].state == before.state);
    let count = set_records(&a.mouse.lock().unwrap()).len();
    let again = transact(&a, &p).unwrap();
    assert!(again.uploaded_macros.is_empty());
    assert!(again.backup.is_none());
    assert_eq!(set_records(&a.mouse.lock().unwrap()).len(), count);
    assert_eq!(backups(&a).len(), 1);
    assert!(b.mouse.lock().unwrap().requests.is_empty());
}
#[test]
fn protected_identical_macro_is_bound_without_reupload_and_full_storage_uses_only_exclusive_slot() {
    for reuse in [false, true] {
        let rig = Rig::new();
        let p = plan();
        {
            let mut m = rig.mouse.lock().unwrap();
            if reuse {
                let mut record = p.macros[0].clone();
                record.slot = 7;
                m.macros.insert(7, record.raw);
                m.other_keys[0][4] = vec![0x21, 7, 0];
            } else {
                for slot in 0..18 {
                    m.macros
                        .insert(slot, simple(slot, &format!("old{slot}"), 4).raw);
                }
                m.state.buttons[4] = vec![0x21, 5, 0];
                m.state.raw_buttons = m.state.buttons.concat();
                m.other_keys[0][4] = vec![0x21, 6, 0];
                m.other_keys[1][4] = vec![0x21, 7, 0];
            }
        }
        let before_macros = rig.mouse.lock().unwrap().macros.clone();
        let before_keys = rig.mouse.lock().unwrap().other_keys.clone();
        let result = transact(&rig, &p).unwrap();
        let m = rig.mouse.lock().unwrap();
        assert_eq!(m.other_keys, before_keys);
        if reuse {
            assert!(result.uploaded_macros.is_empty());
            assert_eq!(result.slot_map[&10], 7);
            assert!(!m.requests.iter().any(|(c, _)| *c == 0x090b));
            assert_eq!(m.macros, before_macros);
        } else {
            assert_eq!(result.uploaded_macros, [5]);
            assert_eq!(result.slot_map[&10], 5);
            assert!(m
                .requests
                .iter()
                .filter(|(c, _)| *c == 0x090b)
                .all(|(_, a)| a[0] == 5));
            for (slot, raw) in before_macros {
                if slot != 5 {
                    assert_eq!(m.macros[&slot], raw);
                }
            }
        }
    }
}
#[test]
fn transaction_moves_ordinary_left_before_replacing_old_button() {
    let rig = Rig::new();
    let mut p = plan();
    p.buttons[0] = vec![0, 2, 0];
    p.buttons[1] = vec![0, 1, 0];
    transact(&rig, &p).unwrap();
    let m = rig.mouse.lock().unwrap();
    let order = m
        .requests
        .iter()
        .filter(|(cmd, _)| *cmd == 0x0101)
        .map(|(_, a)| a[1])
        .collect::<Vec<_>>();
    assert_eq!(&order[..2], &[1, 0]);
    assert_eq!(m.state.buttons[1], [0, 1, 0]);
}
#[test]
fn stale_owner_connection_invalid_plan_and_board_refuse_before_any_set() {
    let rig = Rig::new();
    let p = plan();
    let device = crate::mchose_target::simulated_device("mchose-A");
    let target = crate::mchose_target::read_context(&device, &device.connection_key).unwrap();
    let mut fresh = crate::mchose_target::simulated_device("mchose-A");
    fresh.connection_key.push_str(":reopened");
    assert!(crate::mhub_transaction::apply(&fresh, &rig.t, &target, 0, &p).is_err());
    assert!(rig.mouse.lock().unwrap().requests.is_empty());
    fresh = crate::mchose_target::simulated_device("mchose-A");
    fresh.profile_key.push_str(":new-owner");
    assert!(crate::mhub_transaction::apply(&fresh, &rig.t, &target, 0, &p).is_err());
    assert!(rig.mouse.lock().unwrap().requests.is_empty());
    let mut invalid = p.clone();
    invalid.vendor = 0x046d;
    assert!(transact(&rig, &invalid).is_err());
    assert!(rig.mouse.lock().unwrap().requests.is_empty());
    assert!(crate::mhub_transaction::apply(&device, &rig.t, &target, 1, &p).is_err());
    assert!(rig.mouse.lock().unwrap().requests.is_empty());
    assert!(!rig.directory.exists());
    rig.mouse.lock().unwrap().state.raw_settings[0] = 1;
    assert!(transact(&rig, &p).is_err());
    assert!(set_records(&rig.mouse.lock().unwrap()).is_empty());
    assert!(!rig.directory.exists());
}
#[test]
fn changed_read_set_before_or_after_backup_never_starts_native_set() {
    for fault in [Fault::SettingsAfterMacroScan, Fault::RefDuringRecheck] {
        let rig = Rig::new();
        rig.mouse.lock().unwrap().fault = Some(fault);
        let e = transaction_error(&rig, &plan());
        assert!(e.contains("已改变"));
        assert!(set_records(&rig.mouse.lock().unwrap()).is_empty());
        if fault == Fault::SettingsAfterMacroScan {
            assert!(!rig.directory.exists());
        } else {
            assert_eq!(backups(&rig).len(), 1);
            assert!(e.contains("备份"));
        }
    }
}
#[test]
fn another_board_adding_macro_reference_mid_upload_stops_before_next_chunk() {
    let rig = Rig::new();
    rig.mouse.lock().unwrap().fault = Some(Fault::RefAfterFirstChunk);
    let e = transaction_error(&rig, &plan());
    assert!(e.contains("引用已改变"));
    assert!(e.contains("备份"));
    let m = rig.mouse.lock().unwrap();
    let sets = set_records(&m);
    assert_eq!(sets.len(), 1);
    assert_eq!(sets[0].0, 0x090b);
    assert_eq!(m.other_keys[0][4], [0x21, 0, 0]);
    assert!(!m.macros.contains_key(&0));
}
#[test]
fn board_switch_between_xy_sets_never_sends_y_to_new_or_old_board() {
    let rig = Rig::new();
    rig.mouse.lock().unwrap().fault = Some(Fault::SwitchAfterX);
    let e = transaction_error(&rig, &plan());
    assert!(e.contains("板载配置已改变"));
    assert!(e.contains("备份"));
    let m = rig.mouse.lock().unwrap();
    let dpi = m
        .requests
        .iter()
        .filter(|(cmd, _)| *cmd == 0x0103)
        .collect::<Vec<_>>();
    assert_eq!(dpi.len(), 1);
    assert_eq!(&dpi[0].1[..2], &[0, 0]);
    assert!(!m
        .requests
        .iter()
        .any(|(cmd, _)| [0x0109, 0x0101, 0x0910].contains(cmd)));
}
#[test]
fn failed_chunk_missing_readback_or_lost_commit_ack_is_never_retried() {
    for fault in [Fault::MacroChunk19, Fault::IgnoreMacro, Fault::LostMacroAck] {
        let rig = Rig::new();
        rig.mouse.lock().unwrap().fault = Some(fault);
        let e = transaction_error(&rig, &plan());
        assert!(e.contains("未自动重试"));
        assert!(e.contains("备份"));
        let m = rig.mouse.lock().unwrap();
        let sets = set_records(&m);
        assert!(sets.iter().all(|(cmd, _)| *cmd == 0x090b));
        assert_eq!(sets.len(), if fault == Fault::MacroChunk19 { 2 } else { 6 });
        assert_eq!(m.macros.contains_key(&0), fault == Fault::LostMacroAck);
        assert_eq!(backups(&rig).len(), 1);
    }
}
#[test]
fn settings_key_and_name_ack_without_matching_state_never_report_success() {
    for fault in [
        Fault::IgnoreSettings,
        Fault::IgnoreBinding,
        Fault::IgnoreName,
    ] {
        let rig = Rig::new();
        rig.mouse.lock().unwrap().fault = Some(fault);
        let e = transaction_error(&rig, &plan());
        assert!(e.contains("回读未匹配"));
        assert!(e.contains("备份"));
        let m = rig.mouse.lock().unwrap();
        let cmd = if fault == Fault::IgnoreSettings {
            0x0102
        } else if fault == Fault::IgnoreBinding {
            0x0101
        } else {
            0x0910
        };
        assert_eq!(
            m.requests.iter().filter(|(c, _)| *c == cmd).count(),
            if fault == Fault::IgnoreName { 3 } else { 1 }
        );
        if fault != Fault::IgnoreName {
            assert!(!m.requests.iter().any(|(c, _)| *c == 0x0910));
        }
    }
}
#[test]
fn backup_directory_failure_prevents_all_device_set_commands() {
    let rig = Rig::new();
    std::fs::write(&rig.directory, b"authored path conflict").unwrap();
    assert!(transact(&rig, &plan()).is_err());
    assert!(set_records(&rig.mouse.lock().unwrap()).is_empty());
    std::fs::remove_file(&rig.directory).unwrap();
}

#[test]
fn production_profile_route_applies_full_payload_and_rejects_bad_summary_before_hid() {
    let rig = Rig::new();
    let device = crate::mchose_target::simulated_device("mchose-A");
    let mut profile = to_profile(plan()).unwrap();
    profile.levels[0].x = 2000;
    assert!(!crate::profiles::apply_mchose_profile(&profile, &device, &rig.t).is_empty());
    assert!(rig.mouse.lock().unwrap().requests.is_empty());
    profile = to_profile(plan()).unwrap();
    let mut other = crate::mchose_target::simulated_device("other-brand");
    other.vendor_id = 0x046d;
    assert!(!crate::profiles::apply_mchose_profile(&profile, &other, &rig.t).is_empty());
    assert!(rig.mouse.lock().unwrap().requests.is_empty());
    assert!(crate::profiles::apply_mchose_profile(&profile, &device, &rig.t).is_empty());
    let actual = rig.t.hardware_state().unwrap();
    assert_eq!(actual.dpi_x, profile.mchose.as_ref().unwrap().dpi_x);
    assert_eq!(actual.buttons[4], [0x21, 0, 0]);
    assert_eq!(
        rig.mouse.lock().unwrap().macros[&0],
        profile.mchose.as_ref().unwrap().macros[0].raw
    );
}
#[test]
fn selected_connection_cannot_be_omitted_or_replaced_by_latest_device_metadata() {
    let rig = Rig::new();
    let mut device = crate::mchose_target::simulated_device("mchose-A");
    for token in [None, Some(""), Some("connection:mchose-A:old")] {
        assert!(crate::mhub_transaction::check_selected_connection(&device, token).is_err());
    }
    crate::mhub_transaction::check_selected_connection(&device, Some("connection:mchose-A:1"))
        .unwrap();
    device.connection_key = "connection:mchose-A:2".into();
    assert!(crate::mhub_transaction::check_selected_connection(
        &device,
        Some("connection:mchose-A:1")
    )
    .is_err());
    assert!(rig.mouse.lock().unwrap().requests.is_empty());
    assert!(!rig.directory.exists());
}

#[test]
#[ignore = "requires explicit OPEN_HUB_MHUB_FULL_UI_FIXTURE output path"]
fn export_full_mchose_profile_ui_fixture() {
    let path = std::env::var("OPEN_HUB_MHUB_FULL_UI_FIXTURE").unwrap();
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&to_profile(plan()).unwrap()).unwrap(),
    )
    .unwrap();
}
#[test]
#[ignore = "requires actual built UI capture via OPEN_HUB_MHUB_FULL_UI_CAPTURE"]
fn actual_full_profile_ui_requests_replay_connection_guard_routing_store_and_transaction() {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Args {
        id: String,
        device_id: Option<String>,
        #[serde(default)]
        as_copy: bool,
        expected_connection: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct Request {
        args: Args,
        server_generation: u8,
        expected_success: bool,
    }
    let bytes = std::fs::read(std::env::var("OPEN_HUB_MHUB_FULL_UI_CAPTURE").unwrap()).unwrap();
    assert!(bytes.len() < 128 * 1024);
    let requests: Vec<Request> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(requests.len(), 5);
    let mut store = crate::profiles::ProfileStore::default();
    let source = to_profile(plan()).unwrap();
    store.upsert(source.clone());
    let a = Rig::new();
    let b = Rig::new();
    let mut accepted = 0;
    for request in requests {
        assert_eq!(request.args.id, source.id);
        assert!(!request.args.as_copy);
        let selected = request.args.device_id.as_deref().unwrap();
        let (rig, other) = match selected {
            "mchose-A" => (&a, &b),
            "mchose-B" => (&b, &a),
            _ => panic!("unexpected selected device"),
        };
        let mut device = crate::mchose_target::simulated_device(selected);
        device.connection_key = format!("connection:{selected}:{}", request.server_generation);
        if request.server_generation > 1 {
            device.profile_key = format!("session:{selected}:{}", request.server_generation);
        }
        let peer = crate::profile_routing::Endpoint {
            id: device.id.clone(),
            name: device.name.clone(),
            key: device.profile_key.clone(),
            observed_key: device.profile_key.clone(),
            connection_key: Some(device.connection_key.clone()),
            vendor: device.vendor_id,
            models: device.model_ids.clone(),
            firmware: device.firmware_name.clone(),
            route: crate::profile_routing::Route::Native,
        };
        let mut other_peer = peer.clone();
        other_peer.id = "other-native".into();
        other_peer.key = "session:other-native".into();
        other_peer.observed_key = other_peer.key.clone();
        let prepared = crate::profile_routing::prepare(
            &store,
            &[peer, other_peer],
            &request.args.id,
            Some(selected),
            false,
            true,
        )
        .unwrap();
        let old_store = serde_json::to_value(&store).unwrap();
        let before = rig.mouse.lock().unwrap().requests.len();
        let other_before = other.mouse.lock().unwrap().requests.len();
        let check = crate::mhub_transaction::check_selected_connection(
            &device,
            request.args.expected_connection.as_deref(),
        );
        if !request.expected_success {
            assert!(check.is_err());
            assert_eq!(rig.mouse.lock().unwrap().requests.len(), before);
            assert_eq!(serde_json::to_value(&store).unwrap(), old_store);
            continue;
        }
        check.unwrap();
        assert!(
            crate::profiles::apply_mchose_profile(&prepared.profile, &device, &rig.t).is_empty()
        );
        assert_eq!(other.mouse.lock().unwrap().requests.len(), other_before);
        let mut updated = prepared.updated;
        updated
            .select_for(&prepared.target, &prepared.profile.id)
            .unwrap();
        store = updated;
        accepted += 1;
    }
    assert_eq!(accepted, 4);
    let owners = store.profile_owners.values().collect::<BTreeSet<_>>();
    assert!(owners.contains(&"session:mchose-A:2".to_owned()));
    assert!(owners.contains(&"session:mchose-B".to_owned()));
    assert_eq!(a.mouse.lock().unwrap().state.raw_settings[2] >> 4, 4);
    assert_eq!(b.mouse.lock().unwrap().state.raw_settings[2] >> 4, 4);
}
#[test]
#[ignore = "requires actual full import/editor UI saved profiles capture"]
fn actual_import_editor_saved_profiles_apply_via_production_transaction() {
    let bytes=std::fs::read(std::env::var("OPEN_HUB_MHUB_EDITOR_UI_SAVED").unwrap()).unwrap();
    let saved:Vec<crate::profiles::Profile>=serde_json::from_slice(&bytes).unwrap();assert_eq!(saved.len(),2);assert_eq!(saved[0].mchose,saved[1].mchose);
    let profile=&saved[0];crate::profile_transfer::validate(profile).unwrap();let p=profile.mchose.as_ref().unwrap();
    assert_eq!(p.dpi_x[1],1350);assert_eq!(p.dpi_y.as_ref().unwrap()[1],1450);assert_eq!((p.count,p.active),(3,2));
    assert_eq!((p.usb_rate,p.wireless_rate),(2,5));assert_eq!((p.left_debounce,p.right_debounce,p.sleep,p.never_sleep,p.rotation,p.lod),(10,6,9,false,12,4));
    assert_eq!(p.buttons[1],[0x13,1,14]);assert_eq!(p.buttons[3],[0x21,0,0]);assert_eq!(p.buttons[5],[255;3]);assert_eq!(p.macros.len(),2);
    let original=p.macros.iter().find(|m|m.slot==10).unwrap();assert_eq!(original.name,"界面编辑宏");assert_eq!(original.mode,1);
    assert_eq!(original.events[2].delay_ms,75);assert_eq!((original.events[5].kind.as_str(),original.events[5].code,original.events[5].down),("mouse",2,true));assert_eq!((original.events[6].code,original.events[6].down),(2,false));assert_eq!((original.events[7].kind.as_str(),original.events[7].code),("scroll",0));
    assert_eq!(original.events.len(),11);assert_eq!((original.events[8].code,original.events[8].down),(5,true));assert_eq!(original.events[9].kind,"delay");assert_eq!(original.events[9].delay_ms,50);assert_eq!((original.events[10].code,original.events[10].down),(5,false));
    let recorded=p.macros.iter().find(|m|m.slot==0).unwrap();let keys=recorded.events.iter().filter(|e|e.kind=="key").map(|e|(e.code,e.down)).collect::<Vec<_>>();assert_eq!(keys,[(224,true),(4,true),(4,false),(224,false)]);
    assert_ne!(p.sensor&8,0);
    let mut store=crate::profiles::ProfileStore::default();crate::profile_transfer::merge(&mut store,vec![profile.clone()]).unwrap();
    let a=Rig::new();let b=Rig::new();
    let device=crate::mchose_target::simulated_device("mchose-editor-A");
    assert!(crate::profiles::apply_mchose_profile(profile,&device,&a.t).is_empty());
    assert!(b.mouse.lock().unwrap().requests.is_empty());
    let actual=a.t.hardware_state().unwrap();assert_eq!(actual.dpi_x,p.dpi_x);assert_eq!(actual.dpi_y,p.dpi_y.clone().unwrap());assert_eq!(actual.sensor,p.sensor);
    for logical in [0,10] {let target=actual.buttons[if logical==0 {3}else{4}][1];let m=p.macros.iter().find(|m|m.slot==logical).unwrap();assert_eq!(a.mouse.lock().unwrap().macros[&target],m.raw);}
    let setters=set_records(&a.mouse.lock().unwrap()).len();assert!(crate::profiles::apply_mchose_profile(profile,&device,&a.t).is_empty());assert_eq!(set_records(&a.mouse.lock().unwrap()).len(),setters);
}
