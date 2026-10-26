//! Stateful tests through production framing, validation, backup and readback.
//! Only physical HID I/O is replaced; no OEM code or OS input is executed.
use super::*;
use crate::mchose_config::{Action, HardwareState, Snapshot};
use std::path::PathBuf;

struct Mouse {
    active: u8,
    boards: Vec<HardwareState>,
    writes: Vec<(u16, Vec<u8>)>,
    requests: Vec<u16>,
    ignore_binding_set: bool,
}

impl Mouse {
    fn new(active: u8) -> Self {
        let snapshot: Snapshot =
            serde_json::from_str(include_str!("../tests/fixtures/mchose.json")).unwrap();
        let boards = (0..3)
            .map(|p| {
                let mut state = snapshot.state.clone();
                state.adaptive.clear(); // No adaptive-rate feature on the current A7 model.
                state.profile = p;
                state.raw_settings[0] = p;
                state.raw_x[0] = p;
                state.raw_y[0] = p;
                // An unknown firmware action must survive reading, backup and
                // editing another button without being normalized or rejected.
                state.buttons[3] = vec![0x90, 0x6a, 0x5a];
                state.raw_buttons = state.buttons.concat();
                state
            })
            .collect();
        Self {
            active,
            boards,
            writes: vec![],
            requests: vec![],
            ignore_binding_set: false,
        }
    }

    fn request(&mut self, command: u16, args: &[u8]) -> Result<Vec<u8>> {
        self.requests.push(command);
        match command {
            0x0900 => {
                let mut identity = vec![0; 16];
                identity[..2].copy_from_slice(&VID.to_le_bytes());
                identity[2..4].copy_from_slice(&MODEL_PID.to_le_bytes());
                identity[10] = 2;
                identity[12] = 80;
                Ok(identity)
            }
            0x0002 => Ok(self.boards[self.active as usize].raw_settings.clone()),
            0x0003 => {
                let state = &self.boards[args[0] as usize];
                Ok(if args[1] == 0 {
                    state.raw_x.clone()
                } else {
                    state.raw_y.clone()
                })
            }
            0x0001 => Ok(self.boards[args[0] as usize].raw_buttons.clone()),
            0x0009 => Ok(vec![self.boards[args[0] as usize].lod]),
            0x000a | 0x010a | 0x010b => panic!("unsupported A7 adaptive-rate command"),
            0x0905 => {
                let name = self.boards[0].names[args[0] as usize].as_bytes();
                let mut raw = [0; 60];
                raw[..name.len()].copy_from_slice(name);
                Ok(raw[args[1] as usize..usize::from(args[1]) + usize::from(args[2])].to_vec())
            }
            0x0901 => Ok(vec![0, if args == [0] { 0x17 } else { 6 }]),
            0x090c => Ok(vec![0]), // all synthetic macro slots are empty
            0x0101 => {
                assert_eq!(args.len(), 6);
                assert_eq!(args[2], 11); // one mapping, control flags 3
                assert_eq!(args[0], self.active);
                self.writes.push((command, args.to_vec()));
                if !self.ignore_binding_set {
                    let state = &mut self.boards[args[0] as usize];
                    state.buttons[args[1] as usize] = args[3..].to_vec();
                    state.raw_buttons = state.buttons.concat();
                }
                Ok(vec![])
            }
            _ => Err(invalid(format!(
                "unexpected keybinding fixture command {command:04x}"
            ))),
        }
    }
}

struct Rig {
    transport: Transport,
    mouse: Arc<Mutex<Mouse>>,
    backup: PathBuf,
}

impl Rig {
    fn new(active: u8) -> Self {
        let mouse = Arc::new(Mutex::new(Mouse::new(active)));
        let source = mouse.clone();
        let backup = std::env::temp_dir().join(format!(
            "open-hub-mchose-keybinding-{}",
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

    fn apply(&self, binding: &[u8]) -> Result<HardwareState> {
        let board = self.mouse.lock().unwrap().active;
        self.transport.apply_hardware_for(
            board,
            Action::Button {
                index: 4,
                binding: binding.to_vec(),
            },
        )
    }

    fn assert_only_binding_changed(&self, before: &[HardwareState], binding: &[u8]) {
        let mouse = self.mouse.lock().unwrap();
        let mut expected = before.to_vec();
        let state = &mut expected[mouse.active as usize];
        state.buttons[4] = binding.to_vec();
        state.raw_buttons = state.buttons.concat();
        assert_eq!(state.buttons[3], [0x90, 0x6a, 0x5a]);
        assert!(mouse.boards == expected);
    }

    fn backups(&self) -> Vec<Snapshot> {
        std::fs::read_dir(&self.backup)
            .unwrap()
            .map(|entry| {
                serde_json::from_slice(&std::fs::read(entry.unwrap().path()).unwrap()).unwrap()
            })
            .collect()
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        // Only files generated in this uniquely named test directory.
        if let Ok(entries) = std::fs::read_dir(&self.backup) {
            for entry in entries.flatten() {
                let _ = std::fs::remove_file(entry.path());
            }
            let _ = std::fs::remove_dir(&self.backup);
        }
    }
}

#[test]
fn modifiers_only_roundtrip_all_four_bit_masks_without_other_changes() {
    let rig = Rig::new(1);
    let before = rig.mouse.lock().unwrap().boards.clone();
    for mask in 1..=15 {
        let binding = [0x12, mask, 0];
        let state = rig.apply(&binding).unwrap();
        assert_eq!(state.buttons[4], binding);
        assert_eq!(state.profile, 1);
        rig.assert_only_binding_changed(&before, &binding);
        assert_eq!(
            rig.mouse.lock().unwrap().writes.last().unwrap(),
            &(0x0101, vec![1, 4, 11, 0x12, mask, 0])
        );
    }
    // Authored complete prefix for generic Ctrl+Shift, independently of the
    // renderer and production encoder. The remaining report bytes are zero.
    let packet = frame(0x0101, &[1, 4, 11, 0x12, 3, 0]).unwrap();
    assert_eq!(
        &packet[..15],
        &[0x4d, 1, 1, 6, 1, 1, 0, 0, 1, 4, 11, 0x12, 3, 0, 0x18]
    );
    assert!(packet[15..].iter().all(|v| *v == 0));
    assert_eq!(rig.mouse.lock().unwrap().writes.len(), 15);
    assert_eq!(rig.backups().len(), 15);
}

#[test]
fn general_chords_roundtrip_key_families_as_type13_not_special16() {
    let rig = Rig::new(2);
    let before = rig.mouse.lock().unwrap().boards.clone();
    // Letters, digits, controls, punctuation, F keys, navigation, locks,
    // numpad and Menu: independent representatives of the official key map.
    for usage in [
        4, 29, 30, 39, 40, 49, 51, 58, 69, 70, 76, 82, 83, 87, 98, 99, 101,
    ] {
        let binding = [0x13, 13, usage];
        let state = rig.apply(&binding).unwrap();
        assert_eq!(state.buttons[4], binding);
        rig.assert_only_binding_changed(&before, &binding);
        assert_eq!(
            rig.mouse.lock().unwrap().writes.last().unwrap(),
            &(0x0101, vec![2, 4, 11, 0x13, 13, usage])
        );
    }
    let packet = frame(0x0101, &[2, 4, 11, 0x13, 13, 4]).unwrap();
    assert_eq!(
        &packet[..15],
        &[0x4d, 1, 1, 6, 1, 1, 0, 0, 2, 4, 11, 0x13, 13, 4, 0x10]
    );
    assert!(packet[15..].iter().all(|v| *v == 0));
}

#[test]
fn existing_type16_named_presets_keep_their_raw_encoding() {
    let rig = Rig::new(0);
    let before = rig.mouse.lock().unwrap().boards.clone();
    for usage in [6, 27, 25] {
        let binding = [0x16, 1, usage]; // Copy, Cut, Paste
        assert_eq!(rig.apply(&binding).unwrap().buttons[4], binding);
        rig.assert_only_binding_changed(&before, &binding);
        assert_eq!(
            rig.mouse.lock().unwrap().writes.last().unwrap().1,
            [0, 4, 11, 0x16, 1, usage]
        );
    }
}

#[test]
fn malformed_masks_lengths_and_unknown_chord_main_keys_never_set_or_backup() {
    let rig = Rig::new(0);
    let before = rig.mouse.lock().unwrap().boards.clone();
    let mut rejected = vec![
        vec![0x12],
        vec![0x12, 1],
        vec![0x12, 1, 0, 0],
        vec![0x12, 0, 0],
        vec![0x12, 0x10, 0],
        vec![0x12, 0xff, 0],
        vec![0x12, 1, 1],
        vec![0x13],
        vec![0x13, 1],
        vec![0x13, 1, 4, 0],
        vec![0x13, 0, 4],
        vec![0x13, 0x10, 4],
        vec![0x13, 0xff, 4],
    ];
    for usage in [0, 3, 0x32, 0x64, 0x66, 0x73, 0xa4, 0xe0, 0xe7, 0xff] {
        rejected.push(vec![0x13, 1, usage]);
    }
    for binding in rejected {
        assert!(rig.apply(&binding).is_err(), "accepted {binding:02x?}");
    }
    let mouse = rig.mouse.lock().unwrap();
    assert!(mouse.boards == before);
    assert!(mouse.writes.is_empty());
    assert!(mouse.requests.iter().all(|v| [0x0900, 2].contains(v)));
    assert!(!rig.backup.exists());
}

#[test]
fn successful_ack_without_matching_key_readback_is_an_error_without_resend() {
    for binding in [[0x12, 3, 0], [0x13, 5, 4]] {
        let rig = Rig::new(0);
        let before = rig.mouse.lock().unwrap().boards.clone();
        rig.mouse.lock().unwrap().ignore_binding_set = true;
        assert!(rig.apply(&binding).is_err());
        let mouse = rig.mouse.lock().unwrap();
        assert!(mouse.boards == before);
        assert_eq!(
            mouse.writes,
            [(0x0101, [vec![0, 4, 11], binding.to_vec()].concat())]
        );
        drop(mouse);
        assert_eq!(rig.backups().len(), 1);
    }
}

#[test]
fn unknown_existing_raw_actions_remain_in_reads_and_original_backup() {
    let rig = Rig::new(0);
    let before = rig.transport.full_snapshot().unwrap();
    assert_eq!(before.state.buttons[3], [0x90, 0x6a, 0x5a]);
    assert_eq!(before.state.buttons[4], [0x11, 41, 0]);
    rig.apply(&[0x13, 9, 4]).unwrap();
    let snapshots = rig.backups();
    assert_eq!(snapshots.len(), 1);
    assert!(snapshots[0].state == before.state);
    assert_eq!(snapshots[0].state.raw_buttons, before.state.raw_buttons);
    let actual = rig.transport.hardware_state().unwrap();
    assert_eq!(actual.buttons[3], [0x90, 0x6a, 0x5a]);
    assert_eq!(actual.buttons[4], [0x13, 9, 4]);
    assert_eq!(actual.buttons[5], [255, 255, 255]);
}

#[test]
#[ignore = "requires actual built-UI IPC export via OPEN_HUB_MCHOSE_KEYS_UI_ACTIONS"]
fn actual_ui_key_actions_replay_through_two_isolated_native_transports() {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        id: String,
        #[serde(rename = "expectedProfile")]
        expected_profile: u8,
        action: Action,
        target: crate::mchose_target::Target,
    }

    let path = std::env::var("OPEN_HUB_MCHOSE_KEYS_UI_ACTIONS").unwrap();
    let data = std::fs::read(path).unwrap();
    assert!(data.len() < 1024 * 1024);
    let requests: Vec<Request> = serde_json::from_slice(&data).unwrap();
    assert_eq!(requests.len(), 120);
    let a = Rig::new(0);
    let b = Rig::new(0);
    let mut plain = std::collections::BTreeSet::new();
    let mut chord_masks = std::collections::BTreeSet::new();
    let mut modifier_masks = std::collections::BTreeSet::new();
    let mut special = std::collections::BTreeSet::new();
    let mut consumer = std::collections::BTreeSet::new();
    let mut a_count = 0;
    let mut b_count = 0;
    for request in requests {
        assert_eq!(request.expected_profile, 0);
        let binding = match &request.action {
            Action::Button { index, binding } => {
                assert_eq!(*index, 4);
                binding.clone()
            }
            _ => panic!("key UI export contained another setting"),
        };
        let (target, other) = match request.id.as_str() {
            "mchose-A" => {
                a_count += 1;
                match binding[0] {
                    0x11 => {
                        plain.insert(binding[1]);
                    }
                    0x12 => {
                        modifier_masks.insert(binding[1]);
                    }
                    0x13 => {
                        chord_masks.insert(binding[1]);
                    }
                    0x14 => {consumer.insert(binding.clone());}
                    0x16 => {
                        special.insert(binding.clone());
                    }
                    _ => panic!("unexpected UI binding type"),
                }
                (&a, &b)
            }
            "mchose-B" => {
                b_count += 1;
                assert_eq!(binding[0], 0x11);
                (&b, &a)
            }
            _ => panic!("unexpected UI device identity"),
        };
        let before = target.mouse.lock().unwrap().boards.clone();
        let other_before = other.mouse.lock().unwrap().boards.clone();
        let other_writes = other.mouse.lock().unwrap().writes.clone();
        let device = crate::mchose_target::simulated_device(&request.id);
        let state = crate::mchose_target::with_target(&device, &target.transport, &request.target,
            |t| t.apply_hardware_for(request.expected_profile, request.action)).unwrap();
        assert_eq!(state.buttons[4], binding);
        target.assert_only_binding_changed(&before, &binding);
        assert_eq!(
            target.mouse.lock().unwrap().writes.last().unwrap(),
            &(0x0101, [vec![0, 4, 11], binding].concat())
        );
        let other = other.mouse.lock().unwrap();
        assert!(other.boards == other_before);
        assert_eq!(other.writes, other_writes);
    }
    let known_main_keys = (4..=49).chain(51..=99).chain([101]).chain(224..=231).collect();
    assert_eq!(plain, known_main_keys);
    assert_eq!(chord_masks, [1, 3, 15].into_iter().collect());
    assert_eq!(modifier_masks, [1, 9, 15].into_iter().collect());
    assert_eq!(
        special,
        [vec![0x16, 1, 6], vec![0x16, 1, 25]].into_iter().collect()
    );
    assert_eq!(consumer,[vec![0x14,183,0],vec![0x14,111,0],vec![0x14,112,0]].into_iter().collect());
    assert_eq!((a_count, b_count), (119, 1));
    assert_eq!(a.backups().len(), a_count);
    assert_eq!(b.backups().len(), b_count);
}
