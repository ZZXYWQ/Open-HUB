//! Stateful native requests over the production packet codec; no HID device.
use super::*;
use crate::mchose_config::{Action, HardwareState, Snapshot};
use std::path::PathBuf;

#[derive(Clone, Copy)]
enum Switch {
    AfterDerivedY,
    DuringBackup,
    BeforeOptionalLod,
    AfterFirstDpiSet,
}
struct Mouse {
    active: u8,
    boards: Vec<HardwareState>,
    writes: Vec<(u16, Vec<u8>)>,
    switch: Option<Switch>,
    dpi_written: bool,
    receiver_read_after_dpi: bool,
}
impl Mouse {
    fn new(switch: Option<Switch>) -> Self {
        let snapshot: Snapshot =
            serde_json::from_str(include_str!("../tests/fixtures/mchose.json")).unwrap();
        let boards = (0..3)
            .map(|p| {
                let mut state = snapshot.state.clone();
                state.profile = p;
                state.raw_settings[0] = p;
                state.names = vec!["Board A".into(), "Board B".into(), "Board C".into()];
                if p != 0 {
                    state.dpi_x = vec![500, 1000, 2000, 4000, 8000, 16000];
                    state.dpi_y = vec![600, 1200, 2400, 4800, 9600, 19200];
                    state.independent = true;
                    state.active = p;
                }
                state.raw_x = table(&state, 0);
                state.raw_y = table(&state, 1);
                state
            })
            .collect();
        Self {
            active: 0,
            boards,
            writes: vec![],
            switch,
            dpi_written: false,
            receiver_read_after_dpi: false,
        }
    }
    fn request(&mut self, command: u16, args: &[u8]) -> Result<Vec<u8>> {
        let response = match command {
            0x0900 => {
                if matches!(self.switch, Some(Switch::BeforeOptionalLod))
                    && self.receiver_read_after_dpi
                {
                    self.active = 1;
                    self.switch = None;
                }
                let mut identity = vec![0; 16];
                identity[..2].copy_from_slice(&VID.to_le_bytes());
                identity[2..4].copy_from_slice(&MODEL_PID.to_le_bytes());
                identity[10] = 2;
                identity[12] = 80;
                identity
            }
            0x0002 => self.boards[self.active as usize].raw_settings.clone(),
            0x0003 => {
                let board = &self.boards[args[0] as usize];
                if args[1] == 0 {
                    board.raw_x.clone()
                } else {
                    board.raw_y.clone()
                }
            }
            0x0001 => self.boards[args[0] as usize].raw_buttons.clone(),
            0x0009 => vec![self.boards[args[0] as usize].lod],
            0x000a => self.boards[args[0] as usize].adaptive.clone(),
            0x0905 => {
                let name = self.boards[0].names[args[0] as usize].as_bytes();
                let mut raw = [0u8; 60];
                raw[..name.len()].copy_from_slice(name);
                raw[args[1] as usize..usize::from(args[1]) + usize::from(args[2])].to_vec()
            }
            0x0901 => {
                if args == [1] && self.dpi_written {
                    self.receiver_read_after_dpi = true;
                }
                vec![0, if args == [0] { 0x17 } else { 6 }]
            }
            0x090c => vec![0], // all18 synthetic macro slots are empty
            0x0103 => {
                self.writes.push((command, args.to_vec()));
                self.dpi_written = true;
                let board = &mut self.boards[args[0] as usize];
                board.independent = args[2] != 0;
                board.count = args[3];
                board.active = args[4];
                let values = args[5..]
                    .chunks_exact(2)
                    .map(|v| u16::from_le_bytes([v[0], v[1]]))
                    .collect();
                if args[1] == 0 {
                    board.dpi_x = values;
                    board.raw_x = table(board, 0);
                } else {
                    board.dpi_y = values;
                    board.raw_y = table(board, 1);
                }
                vec![]
            }
            0x0109 => {
                self.writes.push((command, args.to_vec()));
                self.boards[args[0] as usize].lod = args[1];
                vec![]
            }
            0x0102 => {
                self.writes.push((command, args.to_vec()));
                self.boards[args[0] as usize].raw_settings = args.to_vec();
                vec![]
            }
            0x0906 => {
                self.writes.push((command, args.to_vec()));
                self.active = args[0];
                vec![]
            }
            _ => {
                return Err(invalid(format!(
                    "unexpected synthetic native request {command:04x}"
                )))
            }
        };
        let switch =
            matches!(self.switch, Some(Switch::AfterDerivedY)) && command == 3 && args == [0, 1]
                || matches!(self.switch, Some(Switch::DuringBackup)) && command == 0x0905
                || matches!(self.switch, Some(Switch::AfterFirstDpiSet))
                    && command == 0x0103
                    && args[1] == 0;
        if switch {
            self.active = 1;
            self.switch = None;
        }
        Ok(response)
    }
}
fn table(state: &HardwareState, axis: u8) -> Vec<u8> {
    let mut raw = vec![
        state.profile,
        axis,
        state.count,
        state.active,
        state.independent as u8,
    ];
    for value in if axis == 0 {
        &state.dpi_x
    } else {
        &state.dpi_y
    } {
        raw.extend_from_slice(&value.to_le_bytes());
    }
    raw
}
struct Rig {
    transport: Transport,
    mouse: Arc<Mutex<Mouse>>,
    backup: PathBuf,
}
impl Rig {
    fn new(switch: Option<Switch>) -> Self {
        let mouse = Arc::new(Mutex::new(Mouse::new(switch)));
        let source = mouse.clone();
        let backup = std::env::temp_dir().join(format!(
            "open-hub-mchose-board-{}",
            crate::profiles::new_id()
        ));
        let transport = Transport::memory(
            move |command, args| source.lock().unwrap().request(command, args),
            backup.clone(),
        );
        Self {
            transport,
            mouse,
            backup,
        }
    }
}
impl Drop for Rig {
    fn drop(&mut self) {
        // This isolated directory contains only this test's generated files.
        if let Ok(entries) = std::fs::read_dir(&self.backup) {
            for entry in entries.flatten() {
                let _ = std::fs::remove_file(entry.path());
            }
            let _ = std::fs::remove_dir(&self.backup);
        }
    }
}

#[test]
fn standard_dpi_never_retargets_tables_after_board_changes_before_apply() {
    let rig = Rig::new(Some(Switch::AfterDerivedY));
    let before = rig.mouse.lock().unwrap().boards.clone();
    assert!(rig.transport.set_dpi(3600, Some(4200), Some(2)).is_err());
    let mouse = rig.mouse.lock().unwrap();
    assert_eq!(mouse.active, 1);
    assert!(mouse.writes.is_empty());
    assert!(mouse.boards == before);
    assert!(!rig.backup.exists());
}

#[test]
fn explicit_stale_board_and_backup_board_switch_produce_no_native_set() {
    let stale = Rig::new(None);
    stale.mouse.lock().unwrap().active = 1;
    assert!(stale
        .transport
        .apply_hardware_for(0, Action::Lod { index: 2 })
        .is_err());
    assert!(stale.mouse.lock().unwrap().writes.is_empty());
    assert!(!stale.backup.exists());
    let rig = Rig::new(Some(Switch::DuringBackup));
    let before = rig.mouse.lock().unwrap().boards.clone();
    assert!(rig
        .transport
        .apply_hardware_for(0, Action::Lod { index: 2 })
        .is_err());
    let mouse = rig.mouse.lock().unwrap();
    assert_eq!(mouse.active, 1);
    assert!(mouse.writes.is_empty());
    assert!(mouse.boards == before);
    assert!(!rig.backup.exists());
}

#[test]
fn native_dpi_and_optional_lod_keep_all_stages_and_other_boards() {
    let rig = Rig::new(None);
    let before = rig.mouse.lock().unwrap().boards.clone();
    rig.transport.set_dpi(3600, Some(4200), Some(2)).unwrap();
    let actual = rig.transport.hardware_state().unwrap();
    let mut expected_x = before[0].dpi_x.clone();
    expected_x[3] = 3600;
    let mut expected_y = before[0].dpi_y.clone();
    expected_y[3] = 4200;
    assert_eq!(actual.dpi_x, expected_x);
    assert_eq!(actual.dpi_y, expected_y);
    assert_eq!(actual.count, 6);
    assert_eq!(actual.active, 3);
    assert!(actual.independent);
    assert_eq!(actual.lod, 2);
    assert_eq!(actual.raw_settings, before[0].raw_settings);
    assert_eq!(actual.buttons, before[0].buttons);
    let mouse = rig.mouse.lock().unwrap();
    assert!(mouse.boards[1] == before[1] && mouse.boards[2] == before[2]);
    assert_eq!(
        mouse
            .writes
            .iter()
            .map(|(command, args)| (*command, args[0]))
            .collect::<Vec<_>>(),
        [(0x0103, 0), (0x0103, 0), (0x0109, 0)]
    );
    let backups = std::fs::read_dir(&rig.backup)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 2);
    for file in backups {
        let snapshot: Snapshot = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
        assert_eq!(snapshot.state.profile, 0);
    }
}

#[test]
fn optional_lod_does_not_adopt_board_changed_after_successful_dpi() {
    let rig = Rig::new(Some(Switch::BeforeOptionalLod));
    let before = rig.mouse.lock().unwrap().boards.clone();
    assert!(rig.transport.set_dpi(3600, Some(4200), Some(2)).is_err());
    let mouse = rig.mouse.lock().unwrap();
    assert_eq!(mouse.active, 1);
    assert_eq!(mouse.boards[0].dpi_x[3], 3600);
    assert_eq!(mouse.boards[0].lod, before[0].lod);
    assert!(mouse.boards[1] == before[1]);
    assert!(mouse
        .writes
        .iter()
        .all(|(command, args)| *command == 0x0103 && args[0] == 0));
    assert_eq!(mouse.writes.len(), 2);
}

#[test]
fn switch_between_xy_sets_never_targets_new_board_and_is_reported() {
    let rig = Rig::new(Some(Switch::AfterFirstDpiSet));
    let before = rig.mouse.lock().unwrap().boards.clone();
    assert!(rig.transport.set_dpi(3600, Some(4200), None).is_err());
    let mouse = rig.mouse.lock().unwrap();
    assert_eq!(mouse.active, 1);
    assert!(mouse.boards[1] == before[1]);
    assert!(mouse
        .writes
        .iter()
        .all(|(command, args)| *command == 0x0103 && args[0] == 0));
    assert_eq!(mouse.writes.len(), 2);
}

#[test]
fn explicit_profile_selection_is_the_only_intended_board_transition() {
    let rig = Rig::new(None);
    let before = rig.mouse.lock().unwrap().boards.clone();
    let state = rig
        .transport
        .apply_hardware_for(0, Action::Profile { index: 1 })
        .unwrap();
    assert_eq!(state.profile, 1);
    let mouse = rig.mouse.lock().unwrap();
    assert!(mouse.boards == before);
    assert_eq!(mouse.writes, [(0x0906, vec![1])]);
}

#[test]
fn restore_backup_board_switch_prevents_every_native_write() {
    let rig = Rig::new(Some(Switch::DuringBackup));
    let snapshot: Snapshot =
        serde_json::from_str(include_str!("../tests/fixtures/mchose.json")).unwrap();
    let before = rig.mouse.lock().unwrap().boards.clone();
    assert!(rig.transport.restore_snapshot(snapshot).is_err());
    let mouse = rig.mouse.lock().unwrap();
    assert_eq!(mouse.active, 1);
    assert!(mouse.writes.is_empty());
    assert!(mouse.boards == before);
    assert!(!rig.backup.exists());
}

#[test]
fn report_rate_backup_cannot_adopt_a_different_board() {
    let rig = Rig::new(Some(Switch::AfterDerivedY));
    let before = rig.mouse.lock().unwrap().boards.clone();
    assert!(rig.transport.set_report_rate(6).is_err());
    let mouse = rig.mouse.lock().unwrap();
    assert_eq!(mouse.active, 1);
    assert!(mouse.writes.is_empty());
    assert!(mouse.boards == before);
    assert!(!rig.backup.exists());
}

#[test]
fn report_rate_keeps_original_board_settings_and_backups() {
    let rig = Rig::new(None);
    let before = rig.mouse.lock().unwrap().boards.clone();
    rig.transport.set_report_rate(6).unwrap();
    let mouse = rig.mouse.lock().unwrap();
    let mut expected = before[0].raw_settings.clone();
    expected[1] = 0x60 | (expected[1] & 15);
    expected[2] = 0x60 | (expected[2] & 15);
    assert_eq!(mouse.boards[0].raw_settings, expected);
    assert_eq!(mouse.boards[0].dpi_x, before[0].dpi_x);
    assert_eq!(mouse.boards[0].dpi_y, before[0].dpi_y);
    assert!(mouse.boards[1] == before[1] && mouse.boards[2] == before[2]);
    assert_eq!(mouse.writes, [(0x0102, expected)]);
    let files = std::fs::read_dir(&rig.backup)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 1);
    let backup: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&files[0]).unwrap()).unwrap();
    assert_eq!(backup["format"], "mchose-v3-raw-v1");
    assert_eq!(
        backup["settings"],
        serde_json::json!(before[0].raw_settings)
    );
}
